//! Project root discovery for resolving project-relative paths.
//!
//! Walks up the directory tree from a starting point looking for a project
//! marker (`.git`, `.rumdl.toml`, `pyproject.toml`, or `.markdownlint.json`).
//! When a marker is found, its containing directory is returned as the project
//! root. When no marker is found, the start directory is returned as a
//! sensible fallback. The result is canonicalized when possible so callers
//! get a stable, symlink-resolved path.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use super::upward_walk::{UpwardWalk, absolutize};

/// Markers that anchor a project root, in priority order.
/// The first directory that contains any of these is the project root.
const PROJECT_MARKERS: &[&str] = &[".git", ".rumdl.toml", "pyproject.toml", ".markdownlint.json"];

/// The project root of the run, discovered once from the working directory.
///
/// This is the single answer to "which directory does a leading `/` in a link
/// name". MD051 resolves repository-absolute link targets against it and MD057
/// validates absolute destinations against it, so both rules have to be looking
/// at the same directory or one would report a link the other resolves.
///
/// Discovery is a filesystem walk and the working directory does not change
/// during a run, so it is done once. A rule handed an explicit base (MD057's
/// configured `roots`, a test's `with_path`) uses that instead.
static PROJECT_ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    discover_project_root_from(&current_dir)
});

/// The project root of the run. See [`PROJECT_ROOT`].
pub fn project_root() -> &'static Path {
    &PROJECT_ROOT
}

/// The working directory of the run in canonical form, or `None` where it
/// cannot be read.
///
/// Canonical because it is compared against paths built on [`project_root`],
/// which is itself canonical; the two otherwise spell the same directory
/// differently whenever a symlink or a Windows short name is involved. The
/// working directory does not change during a run, so it is read once.
static WORKING_DIRECTORY: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| std::env::current_dir().ok()?.canonicalize().ok());

/// The working directory of the run. See [`WORKING_DIRECTORY`].
pub fn working_directory() -> Option<&'static Path> {
    WORKING_DIRECTORY.as_deref()
}

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
    let found = UpwardWalk::new(start_dir).find(|dir| PROJECT_MARKERS.iter().any(|marker| dir.join(marker).exists()));
    canonicalize_or_keep(found.unwrap_or_else(|| absolutize(start_dir)))
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

    // Uses Unix symlinks; Windows symlink creation requires elevated privileges.
    #[cfg(unix)]
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
