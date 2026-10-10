//! Project-scoped executable discovery shared by execution and cache identity.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::config::BinaryPreference;
use super::lookup;

/// Anchor discovery at the nearest project manifest or repository boundary.
/// A rumdl configuration is the fallback for projects without either.
pub fn project_root(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let start = if absolute.is_dir() {
        absolute.as_path()
    } else {
        absolute.parent().unwrap_or(&absolute)
    };
    let mut config_root = None;
    for directory in start.ancestors() {
        if [".git", "pyproject.toml", "package.json"]
            .iter()
            .any(|name| directory.join(name).exists())
        {
            return directory.to_path_buf();
        }
        if config_root.is_none()
            && [".rumdl.toml", "rumdl.toml", ".config/rumdl.toml"]
                .iter()
                .any(|name| directory.join(name).is_file())
        {
            config_root = Some(directory.to_path_buf());
        }
    }
    config_root.unwrap_or_else(|| start.to_path_buf())
}

/// Resolve only already-installed executables; never install or download tools.
pub fn resolve(
    binary: &str,
    preferences: &BTreeMap<String, BinaryPreference>,
    root: &Path,
    search_path: Option<&OsStr>,
) -> Option<PathBuf> {
    // Explicit paths retain the existing cwd-relative semantics.
    if binary.contains('/') || binary.contains('\\') {
        return lookup::resolve_program(OsStr::new(binary), search_path);
    }
    let preference = preferences.get(binary).copied().unwrap_or_default();
    let system = || lookup::resolve_program(OsStr::new(binary), search_path);
    let project = || {
        let locations = if cfg!(windows) {
            [".venv/Scripts", "venv/Scripts", "node_modules/.bin"]
        } else {
            [".venv/bin", "venv/bin", "node_modules/.bin"]
        };
        locations.iter().find_map(|location| {
            let mut candidate = root.join(location).join(binary);
            // Windows PATH lookup does not select extensionless Unix shims.
            // Apply the same rule in project directories rather than shadowing
            // a usable system executable with node_modules' shell script.
            if cfg!(windows) && !binary.contains('.') {
                candidate.set_extension("exe");
            }
            lookup::resolve_program(candidate.as_os_str(), None)
        })
    };
    match preference {
        BinaryPreference::Project => project().or_else(system),
        BinaryPreference::System => system().or_else(project),
        BinaryPreference::OnlyProject => project(),
        BinaryPreference::OnlySystem => system(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn executable(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\ncat\n").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn preferences_select_the_same_binary_from_controlled_search_paths() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join(if cfg!(windows) {
            ".venv/Scripts/tool.exe"
        } else {
            ".venv/bin/tool"
        });
        let system = dir.path().join(if cfg!(windows) {
            "system/tool.exe"
        } else {
            "system/tool"
        });
        executable(&project);
        executable(&system);
        let path = system.parent().unwrap().as_os_str();
        for (preference, expected) in [
            (BinaryPreference::Project, &project),
            (BinaryPreference::OnlyProject, &project),
            (BinaryPreference::System, &system),
            (BinaryPreference::OnlySystem, &system),
        ] {
            let preferences = BTreeMap::from([("tool".into(), preference)]);
            assert_eq!(
                resolve("tool", &preferences, dir.path(), Some(path)).as_ref(),
                Some(expected)
            );
        }
        let preferences = BTreeMap::from([("tool".into(), BinaryPreference::OnlyProject)]);
        std::fs::remove_file(&project).unwrap();
        assert!(resolve("tool", &preferences, dir.path(), Some(path)).is_none());
        assert_eq!(
            resolve(system.to_str().unwrap(), &preferences, dir.path(), None),
            Some(system)
        );
    }

    #[test]
    fn nested_documents_use_the_nearest_manifest_and_stop_at_repository_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs/nested")).unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        assert_eq!(project_root(&dir.path().join("docs/nested/test.md")), dir.path());
        std::fs::write(dir.path().join("docs/package.json"), "{}").unwrap();
        assert_eq!(
            project_root(&dir.path().join("docs/nested/test.md")),
            dir.path().join("docs")
        );
    }
}
