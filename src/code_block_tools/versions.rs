//! Opt-in verification of the selected executable against project metadata.

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use pep440_rs::Version;
use serde::Deserialize;

use super::config::ToolDefinition;
use super::executor::ToolExecutor;

type Cache<K, V> = Mutex<HashMap<K, Arc<OnceLock<V>>>>;
type LockedVersions = BTreeMap<String, Option<Version>>;

/// Snapshots live only for one invocation (or one direct processor).
#[derive(Debug, Default)]
pub(super) struct VersionState {
    discovery: Cache<PathBuf, Result<PathBuf, DiscoveryFailure>>,
    locks: Cache<PathBuf, Result<LockedVersions, String>>,
    probes: Cache<(PathBuf, String, u64), Result<Version, String>>,
}

#[derive(Debug)]
struct DiscoveryFailure {
    source: PathBuf,
    message: &'static str,
}

pub(super) struct VersionFailure {
    pub key: String,
    pub message: String,
}

fn slot<K: Eq + Hash, V>(cache: &Cache<K, V>, key: K) -> Arc<OnceLock<V>> {
    Arc::clone(
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key)
            .or_default(),
    )
}

#[derive(Deserialize)]
struct UvLock {
    version: u32,
    #[serde(default)]
    package: Vec<LockedPackage>,
}

#[derive(Deserialize)]
struct LockedPackage {
    name: String,
    version: Option<toml::Value>,
}

impl VersionState {
    pub(super) fn verify(
        &self,
        executor: &ToolExecutor,
        tool: &ToolDefinition,
        timeout: u64,
    ) -> Result<(), VersionFailure> {
        let binary = tool.command.first().map_or("", String::as_str);
        let root = executor.project_root();
        let failure = |message: String| VersionFailure {
            // Tuple boundaries keep distinct warning facts distinct across directories.
            key: format!("tool-version:{:?}", (binary, &message)),
            message,
        };
        let name = Path::new(binary)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| failure("Cannot map the executable to a Python package".into()))?;
        let name = if cfg!(windows) {
            let lower = name.to_ascii_lowercase();
            lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
        } else {
            name.to_string()
        };
        if !matches!(name.as_str(), "ruff" | "black" | "djlint" | "sqlfluff") {
            return Err(failure(format!(
                "Cannot resolve a Python package version for tool '{name}'"
            )));
        }
        let discovered = slot(&self.discovery, root.to_path_buf());
        let lockfile = discovered
            .get_or_init(|| find_lockfile(root))
            .as_ref()
            .map_err(|problem| VersionFailure {
                key: format!("tool-version:{:?}", (&problem.source, binary, problem.message)),
                message: problem.message.into(),
            })?;
        let resolved = executor
            .resolve_binary(binary)
            .ok_or_else(|| failure(format!("Cannot resolve executable for '{name}' version verification")))?;
        let failure = |message: String| VersionFailure {
            key: format!("tool-version:{:?}", (lockfile, &resolved, &message)),
            message,
        };
        let parsed = slot(&self.locks, lockfile.clone());
        let versions = parsed
            .get_or_init(|| read_lockfile(lockfile))
            .as_ref()
            .map_err(|message| failure(message.clone()))?;
        let expected = versions.get(&name).and_then(Option::as_ref).ok_or_else(|| {
            failure(format!(
                "Cannot resolve one unambiguous version for '{name}' in uv.lock"
            ))
        })?;
        // The mutex only protects slot lookup. Slow probes initialize their own
        // OnceLock after it is released, leaving unrelated tools independent.
        let timeout = if timeout == 0 { 10_000 } else { timeout.min(10_000) };
        let probed = slot(&self.probes, (resolved.clone(), name.clone(), timeout));
        let actual = probed
            .get_or_init(|| probe_version(&resolved, &name, timeout))
            .as_ref()
            .map_err(|message| failure(message.clone()))?;
        if actual != expected {
            return Err(failure(format!(
                "Tool '{name}' version mismatch: expected {expected} from uv.lock, found {actual}"
            )));
        }
        Ok(())
    }
}

fn find_lockfile(root: &Path) -> Result<PathBuf, DiscoveryFailure> {
    let directories = super::binary::project_directories(root);
    for directory in &directories {
        let candidate = directory.join("uv.lock");
        let failure = || DiscoveryFailure {
            source: candidate.clone(),
            message: "Cannot read uv.lock for tool version verification",
        };
        match candidate.symlink_metadata() {
            Ok(_) => return Ok(candidate.canonicalize().unwrap_or(candidate)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(failure()),
        }
    }
    Err(DiscoveryFailure {
        source: directories.last().copied().unwrap_or(root).to_path_buf(),
        message: "Cannot resolve tool versions: uv.lock not found within the project",
    })
}

fn read_lockfile(path: &Path) -> Result<LockedVersions, String> {
    const MAX_LOCK_BYTES: u64 = 64 * 1024 * 1024;
    let file = std::fs::File::open(path).map_err(|_| "Cannot read uv.lock for tool version verification")?;
    let mut text = String::new();
    file.take(MAX_LOCK_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| "Cannot read uv.lock for tool version verification")?;
    if text.len() as u64 > MAX_LOCK_BYTES {
        return Err("Cannot resolve tool versions: uv.lock exceeds 64 MiB".into());
    }
    // Parse errors can contain arbitrary lockfile values. Do not quote them.
    let lock: UvLock = toml::from_str(&text).map_err(|_| "Cannot parse uv.lock for tool version verification")?;
    if lock.version != 1 {
        return Err("Cannot resolve tool versions: unsupported uv.lock schema version".into());
    }
    let mut versions = BTreeMap::new();
    for package in lock.package {
        let name = package.name.to_ascii_lowercase();
        let version = package
            .version
            .as_ref()
            .and_then(toml::Value::as_str)
            .and_then(|version| version.parse::<Version>().ok());
        versions
            .entry(name)
            .and_modify(|previous| {
                if previous != &version {
                    *previous = None;
                }
            })
            .or_insert(version);
    }
    Ok(versions)
}

/// Probes have a finite deadline and bounded pipes independent of normal tools.
/// Pipe EOF is also bounded by the deadline. Reader threads are abandoned on
/// failure so descendants retaining a pipe cannot delay the caller further.
fn probe_version(resolved: &Path, name: &str, timeout: u64) -> Result<Version, String> {
    let mut child = Command::new(resolved)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| format!("Cannot determine version for '{name}': --version could not start"))?;
    let deadline = Instant::now() + Duration::from_millis(timeout);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let stdout_sender = sender.clone();
    std::thread::spawn(move || {
        let _ = stdout_sender.send((true, read_probe_pipe(stdout)));
    });
    std::thread::spawn(move || {
        let _ = sender.send((false, read_probe_pipe(stderr)));
    });
    let result = (|| {
        let mut output = String::new();
        for _ in 0..2 {
            let (is_stdout, bytes) = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "--version exceeded its time limit")?;
            let bytes = bytes?;
            let text = String::from_utf8(bytes).map_err(|_| "--version output is not valid UTF-8")?;
            if is_stdout {
                output = text;
            } else if !text.trim().is_empty() {
                return Err("unexpected --version stderr");
            }
        }
        let status = super::wait::wait_timeout(&mut child, deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "--version could not finish")?
            .ok_or("--version exceeded its time limit")?;
        if !status.success() {
            return Err("--version exited unsuccessfully");
        }
        parse_version_output(name, &output).ok_or("unrecognized --version output")
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result.map_err(|reason| format!("Cannot determine version for '{name}': {reason}"))
}

fn read_probe_pipe(pipe: impl Read) -> Result<Vec<u8>, &'static str> {
    const MAX_PROBE_BYTES: u64 = 4096;
    let mut bytes = Vec::new();
    pipe.take(MAX_PROBE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read --version output")?;
    if bytes.len() as u64 > MAX_PROBE_BYTES {
        return Err("--version output exceeded 4096 bytes");
    }
    Ok(bytes)
}

fn parse_version_output(name: &str, output: &str) -> Option<Version> {
    let mut lines = output.trim().lines();
    let first = lines.next()?.trim();
    let version = if name == "ruff" {
        first.strip_prefix("ruff ")?
    } else {
        let (program, rest) = first.split_once(", ")?;
        let program = program.to_ascii_lowercase();
        let program = program.strip_suffix(".exe").unwrap_or(&program);
        if program != name {
            return None;
        }
        if name == "black" {
            let rest = rest.strip_prefix("version ").unwrap_or(rest);
            match rest.split_once(" (compiled: ") {
                Some((version, "yes)" | "no)")) => version,
                Some(_) => return None,
                None => rest,
            }
        } else {
            rest.strip_prefix("version ")?
        }
    };
    if name == "black" {
        if lines.next().is_some_and(|line| !line.starts_with("Python (")) || lines.next().is_some() {
            return None;
        }
    } else if lines.next().is_some() {
        return None;
    }
    version.parse().ok()
}
