//! Validate a complete formatting batch before applying its planned writes.

use std::collections::BTreeMap;
use std::fs::{self, Permissions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Original {
    bytes: Vec<u8>,
    target: PathBuf,
    permissions: Permissions,
}

pub struct PreflightFailure {
    pub path: PathBuf,
    pub message: String,
    pub written: usize,
}

/// Input snapshots and output plans for one invocation. External commands are
/// run while planning, never rerun to apply the plan.
pub struct PreflightPlan {
    originals: BTreeMap<PathBuf, Original>,
    writes: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
    diagnostics: Mutex<BTreeMap<PathBuf, Vec<rumdl_lib::rule::LintWarning>>>,
}

impl PreflightFailure {
    pub fn warning(&self) -> rumdl_lib::rule::LintWarning {
        rumdl_lib::rule::LintWarning {
            message: self.message.clone(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 1,
            severity: rumdl_lib::rule::Severity::Error,
            fix: None,
            rule_name: Some("preflight".into()),
        }
    }
}

impl PreflightPlan {
    pub fn read(paths: &[String]) -> Result<Self, PreflightFailure> {
        let mut originals = BTreeMap::new();
        for path in paths {
            let path = Path::new(path);
            let fail = |message: String| PreflightFailure {
                path: path.to_path_buf(),
                message,
                written: 0,
            };
            let metadata = fs::metadata(path).map_err(|error| fail(error.to_string()))?;
            if !metadata.is_file() {
                return Err(fail("input is not a regular file".into()));
            }
            let bytes = fs::read(path).map_err(|error| fail(error.to_string()))?;
            std::str::from_utf8(&bytes).map_err(|error| fail(format!("input is not valid UTF-8: {error}")))?;
            let target = fs::canonicalize(path).map_err(|error| fail(error.to_string()))?;
            originals.insert(
                path.to_path_buf(),
                Original {
                    bytes,
                    target,
                    permissions: metadata.permissions(),
                },
            );
        }
        Ok(Self {
            originals,
            writes: Mutex::new(BTreeMap::new()),
            diagnostics: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn record_diagnostics(&self, path: &Path, warnings: Vec<rumdl_lib::rule::LintWarning>) {
        self.diagnostics.lock().unwrap().insert(path.to_path_buf(), warnings);
    }

    pub fn original_diagnostics(&self, written: usize) -> BTreeMap<PathBuf, Vec<rumdl_lib::rule::LintWarning>> {
        let committed: std::collections::BTreeSet<_> =
            self.writes.lock().unwrap().keys().take(written).cloned().collect();
        self.diagnostics
            .lock()
            .unwrap()
            .iter()
            .filter(|(path, _)| !committed.contains(*path))
            .map(|(path, warnings)| (path.clone(), warnings.clone()))
            .collect()
    }

    pub fn stage(&self, path: &Path, content: Vec<u8>) {
        self.writes.lock().unwrap().insert(path.to_path_buf(), content);
    }

    fn validate(&self, path: &Path, writing: bool) -> Result<(), String> {
        let original = &self.originals[path];
        if fs::canonicalize(path).map_err(|error| error.to_string())? != original.target {
            return Err("input target changed while formatting was planned".into());
        }
        if fs::read(path).map_err(|error| error.to_string())? != original.bytes {
            return Err("input content changed while formatting was planned".into());
        }
        let permissions = fs::metadata(path).map_err(|error| error.to_string())?.permissions();
        let changed = permissions.readonly() != original.permissions.readonly();
        #[cfg(unix)]
        let changed = {
            use std::os::unix::fs::PermissionsExt;
            changed || permissions.mode() != original.permissions.mode()
        };
        if changed {
            return Err("input permissions changed while formatting was planned".into());
        }
        if writing && permissions.readonly() {
            return Err("planned output would replace a read-only file".into());
        }
        Ok(())
    }

    /// Validate all selected inputs again before the first write. Errors during
    /// the later write phase retain already-applied files; this is preflight,
    /// not a multi-file rollback transaction.
    pub fn apply(&self) -> Result<usize, PreflightFailure> {
        let writes = self.writes.lock().unwrap();
        for path in self.originals.keys() {
            self.validate(path, writes.contains_key(path))
                .map_err(|message| PreflightFailure {
                    path: path.clone(),
                    message,
                    written: 0,
                })?;
        }
        let mut written = 0;
        for (path, content) in writes.iter() {
            // Protect an input edited after the batch validation too. As with
            // any read followed by an atomic replacement, a race remains
            // between this check and the rename.
            self.validate(path, true)
                .and_then(|()| {
                    rumdl_lib::utils::atomic_write::write_atomically(path, content).map_err(|e| e.to_string())
                })
                .map_err(|message| PreflightFailure {
                    path: path.clone(),
                    message,
                    written,
                })?;
            written += 1;
        }
        Ok(written)
    }
}
