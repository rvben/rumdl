//! Project root discovery for resolving project-relative paths.
//!
//! Walks up the directory tree from a starting point looking for a project
//! marker (`.git`, `.rumdl.toml`, `pyproject.toml`, or `.markdownlint.json`).
//! When a marker is found, its containing directory is returned as the project
//! root. When no marker is found within `MAX_DEPTH` levels, the start directory
//! is returned as a sensible fallback. The result is canonicalized when
//! possible so callers get a stable, symlink-resolved path.

use std::path::{Path, PathBuf};

/// Maximum number of parent directories to traverse before giving up.
/// Matches `Config::find_project_root_from` to keep the two implementations
/// consistent for the same input.
const MAX_DEPTH: usize = 100;

/// Markers that anchor a project root, in priority order.
/// The first directory that contains any of these is the project root.
const PROJECT_MARKERS: &[&str] = &[".git", ".rumdl.toml", "pyproject.toml", ".markdownlint.json"];

/// Discover the project root by walking up from `start_dir`.
///
/// Returns the directory containing the first project marker (`.git`,
/// `.rumdl.toml`, `pyproject.toml`, or `.markdownlint.json`) found while
/// traversing parent directories. Falls back to `start_dir` itself when
/// no marker is found.
///
/// The result is canonicalized to resolve symlinks; if canonicalization
/// fails (e.g. because the path no longer exists), the un-canonicalized
/// path is returned instead.
pub fn discover_project_root_from(start_dir: &Path) -> PathBuf {
    let absolute_start = if start_dir.is_relative() {
        std::env::current_dir().map_or_else(|_| start_dir.to_path_buf(), |cwd| cwd.join(start_dir))
    } else {
        start_dir.to_path_buf()
    };

    let mut current = absolute_start.clone();
    for _ in 0..MAX_DEPTH {
        if PROJECT_MARKERS.iter().any(|marker| current.join(marker).exists()) {
            return canonicalize_or_keep(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    canonicalize_or_keep(absolute_start)
}

fn canonicalize_or_keep(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_discovers_root_via_git_marker() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        let nested = root.join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(discover_project_root_from(&nested), root);
    }

    #[test]
    fn test_discovers_root_via_rumdl_toml_marker() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(root.join(".rumdl.toml"), "").unwrap();
        let nested = root.join("docs");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(discover_project_root_from(&nested), root);
    }

    #[test]
    fn test_discovers_root_via_pyproject_toml_marker() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(root.join("pyproject.toml"), "").unwrap();
        let nested = root.join("src");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(discover_project_root_from(&nested), root);
    }

    #[test]
    fn test_marker_at_ancestor_wins_over_deeper_start() {
        // When the marker sits several levels above the start directory, that
        // ancestor is the project root — the function returns it, not the
        // start directory or any intermediate parent.
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(root.join(".git"), "stub").unwrap();
        let deeply_nested = root.join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deeply_nested).unwrap();

        assert_eq!(discover_project_root_from(&deeply_nested), root);
    }

    #[test]
    fn test_first_marker_wins_when_nested_projects() {
        // When markers exist at multiple ancestor levels, the *closest* ancestor
        // wins — the walk stops at the first marker, not the topmost.
        let temp = tempdir().unwrap();
        let outer = temp.path().canonicalize().unwrap();
        fs::write(outer.join(".git"), "stub").unwrap();
        let inner = outer.join("subproject");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join(".rumdl.toml"), "").unwrap();
        let start = inner.join("docs");
        fs::create_dir_all(&start).unwrap();

        assert_eq!(discover_project_root_from(&start), inner, "closest marker should win");
    }

    #[test]
    fn test_canonicalizes_symlinked_root() {
        let temp = tempdir().unwrap();
        let real_root = temp.path().canonicalize().unwrap().join("real");
        fs::create_dir_all(&real_root).unwrap();
        fs::create_dir_all(real_root.join(".git")).unwrap();

        let link = temp.path().canonicalize().unwrap().join("link");
        if std::os::unix::fs::symlink(&real_root, &link).is_err() {
            return;
        }

        let from_link = discover_project_root_from(&link);
        assert_eq!(from_link, real_root, "symlink should canonicalize to real path");
    }
}
