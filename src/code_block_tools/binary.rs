//! Project-scoped executable discovery shared by execution and cache identity.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::config::BinaryPreference;
use super::lookup;

/// Start project discovery at the document directory, preserving nearer installs.
/// Ancestor lookup is bounded by the first repository, or a rumdl config if
/// there is no repository. Without either marker, only this directory is used.
pub fn project_root(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let directory = if absolute.is_dir() {
        absolute
    } else {
        absolute.parent().unwrap_or(&absolute).to_path_buf()
    };
    // Match discovery's canonical paths, including Windows verbatim prefixes.
    std::fs::canonicalize(&directory).unwrap_or(directory)
}

fn project_directories(start: &Path) -> Vec<&Path> {
    use crate::config::file_source::{ConfigFileSource, FsConfigFiles};

    let home = FsConfigFiles
        .home_dir()
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path));
    project_directories_with_home(start, home.as_deref())
}

fn project_directories_with_home<'a>(start: &'a Path, home: Option<&Path>) -> Vec<&'a Path> {
    let boundary = start
        .ancestors()
        .find(|directory| directory.join(".git").exists())
        .or_else(|| {
            start.ancestors().find(|directory| {
                // A home configuration is user-wide, not a project boundary.
                if Some(*directory) == home {
                    return false;
                }
                [".rumdl.toml", "rumdl.toml", ".config/rumdl.toml"]
                    .iter()
                    .any(|name| directory.join(name).is_file())
                    || std::fs::read_to_string(directory.join("pyproject.toml"))
                        .ok()
                        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
                        .is_some_and(|value| value.get("tool").and_then(|tool| tool.get("rumdl")).is_some())
            })
        })
        .unwrap_or(start);
    start
        .ancestors()
        .take_while(|directory| directory.starts_with(boundary))
        .collect()
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
        project_directories(root).into_iter().find_map(|directory| {
            locations.iter().find_map(|location| {
                let mut candidate = directory.join(location).join(binary);
                // Windows PATH lookup does not select extensionless Unix shims.
                // Apply the same rule in project directories rather than shadowing
                // a usable system executable with node_modules' shell script.
                if cfg!(windows) && !binary.contains('.') {
                    candidate.set_extension("exe");
                }
                lookup::resolve_program(candidate.as_os_str(), None)
            })
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
    fn workspace_lookup_is_nearest_first_and_stops_at_git_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        let nested = root.join("packages/app/docs");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join(".git"), "gitdir: elsewhere").unwrap();
        std::fs::write(root.join("packages/app/package.json"), "{}").unwrap();
        let suffix = if cfg!(windows) {
            "node_modules/.bin/tool.exe"
        } else {
            "node_modules/.bin/tool"
        };
        let outer = dir.path().join(suffix);
        let hoisted = root.join(suffix);
        let nearest = root.join("packages/app").join(suffix);
        executable(&outer);
        executable(&hoisted);
        let preferences = BTreeMap::from([("tool".into(), BinaryPreference::OnlyProject)]);
        assert_eq!(resolve("tool", &preferences, &nested, None), Some(hoisted.clone()));
        executable(&nearest);
        assert_eq!(resolve("tool", &preferences, &nested, None), Some(nearest.clone()));
        std::fs::remove_file(nearest).unwrap();
        std::fs::remove_file(hoisted).unwrap();
        assert!(resolve("tool", &preferences, &nested, None).is_none());
        assert!(resolve("tool", &BTreeMap::new(), &nested, None).is_none());
    }
    #[test]
    fn pyproject_rumdl_bounds_non_git_workspace_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("packages/app/docs");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[tool.rumdl]\n").unwrap();
        std::fs::write(
            dir.path().join("packages/app/pyproject.toml"),
            "[project]\nname='app'\n",
        )
        .unwrap();
        let suffix = if cfg!(windows) {
            ".venv/Scripts/tool.exe"
        } else {
            ".venv/bin/tool"
        };
        let tool = dir.path().join(suffix);
        executable(&tool);
        let preferences = BTreeMap::from([("tool".into(), BinaryPreference::OnlyProject)]);
        assert_eq!(resolve("tool", &preferences, &nested, None), Some(tool));
    }

    #[cfg(windows)]
    #[test]
    fn project_lookup_ignores_cmd_wrappers_without_an_explicit_shell() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join(".git"), "").unwrap();
        executable(&dir.path().join("node_modules/.bin/tool.cmd"));
        let preferences = BTreeMap::from([("tool".into(), BinaryPreference::OnlyProject)]);
        assert!(resolve("tool", &preferences, &dir.path().join("docs"), None).is_none());
        executable(&dir.path().join(".venv/Scripts/tool.exe"));
        assert!(resolve("tool", &preferences, &dir.path().join("docs"), None).is_some());
    }
    #[test]
    fn user_home_config_is_not_a_project_but_an_explicit_git_root_is() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("docs/nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.path().join(".rumdl.toml"), "").unwrap();
        assert_eq!(
            project_directories_with_home(&nested, Some(dir.path())),
            vec![nested.as_path()]
        );
        std::fs::write(dir.path().join(".git"), "").unwrap();
        assert_eq!(
            project_directories_with_home(&nested, Some(dir.path())).last().copied(),
            Some(dir.path())
        );
    }
}
