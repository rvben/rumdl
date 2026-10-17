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

use super::config::{ToolDefinition, ToolVersionDefinition};
use super::executor::ToolExecutor;

mod hooks;
mod pyproject;

type Cache<K, V> = Mutex<HashMap<K, Arc<OnceLock<V>>>>;
type ExpectedVersions = BTreeMap<String, Option<ExpectedVersion>>;

#[derive(Debug, PartialEq)]
enum ExpectedVersion {
    Exact(Version),
    Constraints(pyproject::Constraints),
}

impl ExpectedVersion {
    fn is_resolved(&self) -> bool {
        match self {
            Self::Exact(_) => true,
            Self::Constraints(constraints) => constraints.is_resolved(),
        }
    }
}

#[derive(Debug)]
struct ProbedVersion {
    version: Version,
    raw: String,
}

/// Snapshots live only for one invocation (or one direct processor).
#[derive(Debug, Default)]
pub(super) struct VersionState {
    discovery: Cache<(ToolVersionDefinition, PathBuf), Result<SelectedMetadata, DiscoveryFailure>>,
    metadata: Cache<(ToolVersionDefinition, PathBuf), Result<ExpectedVersions, String>>,
    probes: Cache<(PathBuf, String, u64), Result<ProbedVersion, String>>,
}

#[derive(Debug)]
struct DiscoveryFailure {
    source: PathBuf,
    message: String,
}

#[derive(Debug)]
struct SelectedMetadata {
    source: ToolVersionDefinition,
    path: PathBuf,
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
        source: ToolVersionDefinition,
        timeout: u64,
    ) -> Result<(), VersionFailure> {
        let binary = tool.command.first().map_or("", String::as_str);
        let root = executor.project_root();
        let failure = |message: String| VersionFailure {
            // Tuple boundaries keep distinct warning facts distinct across directories.
            key: format!("tool-version:{:?}", (source, binary, &message)),
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
        let discovered = slot(&self.discovery, (source, root.to_path_buf()));
        let selected = discovered
            .get_or_init(|| find_metadata(root, source))
            .as_ref()
            .map_err(|problem| VersionFailure {
                key: format!("tool-version:{:?}", (source, &problem.source, binary, &problem.message)),
                message: problem.message.clone(),
            })?;
        let automatic = source == ToolVersionDefinition::Auto;
        let source = selected.source;
        let lockfile = &selected.path;
        let decorate = |message: String| {
            if automatic {
                format!(
                    "{message} (auto selected {}; other sources were not tried)",
                    source.filename()
                )
            } else {
                message
            }
        };
        let resolved = executor.resolve_binary(binary).ok_or_else(|| VersionFailure {
            key: format!("tool-version:{:?}", (source, lockfile, binary, name.as_str())),
            message: decorate(format!("Cannot resolve executable for '{name}' version verification")),
        })?;
        let failure = |message: String| VersionFailure {
            key: format!("tool-version:{:?}", (source, lockfile, &resolved, &message)),
            message: decorate(message),
        };
        let parsed = slot(&self.metadata, (source, lockfile.clone()));
        let versions = parsed
            .get_or_init(|| read_metadata(lockfile, source))
            .as_ref()
            .map_err(|message| failure(message.clone()))?;
        let expected = versions
            .get(&name)
            .and_then(Option::as_ref)
            .filter(|expected| expected.is_resolved())
            .ok_or_else(|| {
                failure(match source {
                    ToolVersionDefinition::Auto => "Cannot resolve automatic tool version source".into(),
                    ToolVersionDefinition::UvLock => {
                        format!("Cannot resolve one unambiguous version for '{name}' in uv.lock")
                    }
                    ToolVersionDefinition::Pyproject => {
                        format!("Cannot resolve declared version constraints for '{name}' in pyproject.toml")
                    }
                    ToolVersionDefinition::PreCommit
                    | ToolVersionDefinition::PreCommitYml
                    | ToolVersionDefinition::Prek => {
                        format!(
                            "Cannot resolve one unambiguous hook version for '{name}' in {}",
                            source.filename()
                        )
                    }
                })
            })?;
        // The mutex only protects slot lookup. Slow probes initialize their own
        // OnceLock after it is released, leaving unrelated tools independent.
        let timeout = if timeout == 0 { 10_000 } else { timeout.min(10_000) };
        let probed = slot(&self.probes, (resolved.clone(), name.clone(), timeout));
        let actual = probed
            .get_or_init(|| probe_version(&resolved, &name, timeout))
            .as_ref()
            .map_err(|message| failure(message.clone()))?;
        match expected {
            ExpectedVersion::Exact(expected) if expected != &actual.version => {
                return Err(failure(format!(
                    "Tool '{name}' version mismatch: expected {expected} from {}, found {}",
                    source.filename(),
                    actual.version
                )));
            }
            ExpectedVersion::Constraints(constraints) => {
                if let Some(reason) = constraints.mismatch(actual) {
                    return Err(failure(format!(
                        "Tool '{name}' version mismatch: found {}; {reason} from {}",
                        actual.version,
                        source.filename()
                    )));
                }
            }
            ExpectedVersion::Exact(_) => {}
        }
        Ok(())
    }
}

fn find_metadata(root: &Path, source: ToolVersionDefinition) -> Result<SelectedMetadata, DiscoveryFailure> {
    let directories = super::binary::project_directories(root);
    let candidates = if source == ToolVersionDefinition::Auto {
        &[
            ToolVersionDefinition::UvLock,
            ToolVersionDefinition::Prek,
            ToolVersionDefinition::PreCommit,
            ToolVersionDefinition::PreCommitYml,
            ToolVersionDefinition::Pyproject,
        ][..]
    } else {
        std::slice::from_ref(&source)
    };
    for directory in &directories {
        for &candidate_source in candidates {
            let candidate = directory.join(candidate_source.filename());
            match candidate.symlink_metadata() {
                Ok(_) => {
                    return Ok(SelectedMetadata {
                        source: candidate_source,
                        path: candidate.canonicalize().unwrap_or(candidate),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    let message = format!(
                        "Cannot read {} for tool version verification",
                        candidate_source.filename()
                    );
                    return Err(DiscoveryFailure {
                        source: candidate,
                        message: if source == ToolVersionDefinition::Auto {
                            format!(
                                "{message} (auto selected {}; other sources were not tried)",
                                candidate_source.filename()
                            )
                        } else {
                            message
                        },
                    });
                }
            }
        }
    }
    Err(DiscoveryFailure {
        source: directories.last().copied().unwrap_or(root).to_path_buf(),
        message: if source == ToolVersionDefinition::Auto {
            "Cannot resolve tool versions: no supported metadata file found within the project".into()
        } else {
            format!(
                "Cannot resolve tool versions: {} not found within the project",
                source.filename()
            )
        },
    })
}

fn read_metadata(path: &Path, source: ToolVersionDefinition) -> Result<ExpectedVersions, String> {
    const MAX_METADATA_BYTES: u64 = 64 * 1024 * 1024;
    let read_error = || format!("Cannot read {} for tool version verification", source.filename());
    let file = std::fs::File::open(path).map_err(|_| read_error())?;
    let mut text = String::new();
    file.take(MAX_METADATA_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| read_error())?;
    if text.len() as u64 > MAX_METADATA_BYTES {
        return Err(format!(
            "Cannot resolve tool versions: {} exceeds 64 MiB",
            source.filename()
        ));
    }
    match source {
        ToolVersionDefinition::Auto => Err("Cannot resolve automatic tool version source".into()),
        ToolVersionDefinition::UvLock => parse_lockfile(&text),
        ToolVersionDefinition::Pyproject => pyproject::parse(&text),
        ToolVersionDefinition::PreCommit | ToolVersionDefinition::PreCommitYml | ToolVersionDefinition::Prek => {
            hooks::parse(&text, source)
        }
    }
}

fn parse_lockfile(text: &str) -> Result<ExpectedVersions, String> {
    // Parse errors can contain arbitrary lockfile values. Do not quote them.
    let lock: UvLock = toml::from_str(text).map_err(|_| "Cannot parse uv.lock for tool version verification")?;
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
            .and_then(|version| version.parse::<Version>().ok())
            .map(ExpectedVersion::Exact);
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
fn probe_version(resolved: &Path, name: &str, timeout: u64) -> Result<ProbedVersion, String> {
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

fn parse_version_output(name: &str, output: &str) -> Option<ProbedVersion> {
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
    Some(ProbedVersion {
        version: version.parse().ok()?,
        raw: version.into(),
    })
}
