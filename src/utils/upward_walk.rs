//! Upward directory traversal with shared stop semantics.
//!
//! Config and project-root discovery all walk from a starting directory toward
//! the filesystem root, probing each directory on the way. The stop conditions
//! are the subtle part and live here, in one place:
//!
//! - **Home boundary (exclusive):** the walk ends *before* yielding the home
//!   directory. A config in `$HOME` is user-level, not a project config, and
//!   must reach the loader only through the user-config fallback.
//! - **Git root (inclusive):** a directory containing `.git` is yielded and
//!   then the walk ends, so a config in the repository root is still found.
//! - **Stop root (inclusive):** an explicit directory (e.g. a project root) is
//!   yielded and then the walk ends.
//! - **Depth cap:** a guard against runaway traversal.
//!
//! Boundary comparisons canonicalize both sides: on Windows the walked path
//! and the boundary can be different representations of the same directory
//! (8.3 short names vs `\\?\` long names), and on Unix symlinks differ.
//! Comparing raw forms would never match and the walk would overshoot. When
//! canonicalization fails (path no longer exists), the raw forms are compared
//! as a fallback. Yielded paths keep their original representation; only the
//! stop checks canonicalize.

use std::path::{Path, PathBuf};

/// Maximum number of directories visited before the walk gives up.
const MAX_DEPTH: usize = 100;

/// A directory the walk compares itself against, with its canonical form
/// resolved once at construction.
struct Boundary {
    raw: PathBuf,
    canonical: Option<PathBuf>,
}

impl Boundary {
    fn new(raw: PathBuf) -> Self {
        let canonical = std::fs::canonicalize(&raw).ok();
        Self { raw, canonical }
    }

    /// Whether `dir` is this boundary, comparing canonically with a raw
    /// fallback when canonicalization fails.
    fn matches(&self, dir: &Path) -> bool {
        match (&self.canonical, std::fs::canonicalize(dir).ok()) {
            (Some(boundary), Some(current)) => boundary == &current,
            _ => self.raw == dir,
        }
    }
}

/// Iterator over a directory and its ancestors, ending at the configured
/// stop conditions. See the module docs for the stop semantics.
pub struct UpwardWalk {
    next: Option<PathBuf>,
    remaining: usize,
    exclusive_stop: Option<Boundary>,
    stop_at_git_root: bool,
    inclusive_stop: Option<Boundary>,
    always_yield_start: bool,
    started: bool,
}

impl UpwardWalk {
    /// Start a walk at `start`. Relative paths are resolved against the
    /// current directory first, so `parent()` traversal sees the full
    /// ancestor chain instead of running out at `""`.
    pub fn new(start: &Path) -> Self {
        Self {
            next: Some(absolutize(start)),
            remaining: MAX_DEPTH,
            exclusive_stop: None,
            stop_at_git_root: false,
            inclusive_stop: None,
            always_yield_start: false,
            started: false,
        }
    }

    /// End the walk *before* yielding `boundary` (typically the home
    /// directory). `None` leaves the walk unbounded.
    pub fn stop_below(mut self, boundary: Option<PathBuf>) -> Self {
        self.exclusive_stop = boundary.map(Boundary::new);
        self
    }

    /// Yield a directory containing `.git`, then end the walk.
    pub fn stop_at_git_root(mut self) -> Self {
        self.stop_at_git_root = true;
        self
    }

    /// Yield `root` itself, then end the walk.
    pub fn stop_at(mut self, root: &Path) -> Self {
        self.inclusive_stop = Some(Boundary::new(root.to_path_buf()));
        self
    }

    /// Yield the start directory even when it is the exclusive boundary; the
    /// walk still ends there, so ancestors of the boundary are never probed.
    ///
    /// The exclusive boundary guards against the walk *escaping into* home
    /// territory from a project below it. The start directory is different: it
    /// is an explicitly chosen context (the cwd of a CLI run), so a config
    /// there is a project config by intent, even when that directory happens to
    /// be `$HOME` (pre-commit.ci sets `HOME` to the git checkout).
    pub fn always_yield_start(mut self) -> Self {
        self.always_yield_start = true;
        self
    }
}

impl Iterator for UpwardWalk {
    type Item = PathBuf;

    fn next(&mut self) -> Option<PathBuf> {
        let current = self.next.take()?;
        if self.remaining == 0 {
            log::debug!("[rumdl-config] Maximum upward traversal depth reached");
            return None;
        }
        self.remaining -= 1;

        let is_start = !self.started;
        self.started = true;

        if let Some(boundary) = &self.exclusive_stop
            && boundary.matches(&current)
        {
            if !(is_start && self.always_yield_start) {
                return None;
            }
            // The start is spared from the boundary, but the walk must still
            // end here: directories above the boundary are never probed.
            return Some(current);
        }

        let stop_after = (self.stop_at_git_root && current.join(".git").exists())
            || self.inclusive_stop.as_ref().is_some_and(|b| b.matches(&current));
        if !stop_after {
            self.next = current.parent().map(Path::to_path_buf);
        }

        Some(current)
    }
}

/// Resolve a possibly-relative path against the current directory without
/// canonicalizing, so the representation (symlinks, Windows short names) is
/// preserved.
pub fn absolutize(path: &Path) -> PathBuf {
    if path.is_relative() {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn walks_from_start_to_filesystem_root_by_default() {
        let temp = tempdir().unwrap();
        let nested = temp.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&nested).collect();
        assert_eq!(visited[0], nested);
        assert_eq!(visited[1], temp.path().join("a"));
        assert_eq!(visited[2], temp.path());
        let last = visited.last().unwrap();
        assert!(last.parent().is_none(), "walk should end at the filesystem root");
    }

    #[test]
    fn git_root_is_yielded_then_walk_ends() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let nested = repo.join("docs");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(&nested).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&nested).stop_at_git_root().collect();
        assert_eq!(visited, vec![nested, repo]);
    }

    #[test]
    fn home_boundary_is_not_yielded() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("project");
        fs::create_dir_all(&project).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&project).stop_below(Some(home.clone())).collect();
        assert_eq!(visited, vec![project], "the home directory itself must not be probed");
    }

    #[test]
    fn stop_root_is_yielded_then_walk_ends() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        let nested = root.join("docs").join("api");
        fs::create_dir_all(&nested).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&nested).stop_at(&root).collect();
        assert_eq!(visited, vec![nested, root.join("docs"), root]);
    }

    #[test]
    fn start_equal_to_stop_root_yields_exactly_the_root() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(&root).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&root).stop_at(&root).collect();
        assert_eq!(visited, vec![root]);
    }

    // Uses Unix symlinks; Windows symlink creation requires elevated privileges.
    #[cfg(unix)]
    #[test]
    fn stop_root_matches_through_differing_path_representations() {
        let temp = tempdir().unwrap();
        let real_root = temp.path().join("real");
        let nested = real_root.join("docs");
        fs::create_dir_all(&nested).unwrap();
        let link = temp.path().join("link");
        if std::os::unix::fs::symlink(&real_root, &link).is_err() {
            return;
        }

        // The walk runs over the real path while the stop root is the symlink:
        // raw comparison would never match, canonical comparison must.
        let visited: Vec<PathBuf> = UpwardWalk::new(&nested).stop_at(&link).collect();
        assert_eq!(visited, vec![nested, real_root]);
    }

    #[cfg(unix)]
    #[test]
    fn home_boundary_matches_through_differing_path_representations() {
        let temp = tempdir().unwrap();
        let real_home = temp.path().join("real-home");
        let project = real_home.join("project");
        fs::create_dir_all(&project).unwrap();
        let link = temp.path().join("link-home");
        if std::os::unix::fs::symlink(&real_home, &link).is_err() {
            return;
        }

        let visited: Vec<PathBuf> = UpwardWalk::new(&project).stop_below(Some(link)).collect();
        assert_eq!(visited, vec![project]);
    }

    #[test]
    fn boundary_comparison_falls_back_to_raw_paths_when_canonicalization_fails() {
        let temp = tempdir().unwrap();
        let ghost = temp.path().join("does-not-exist");
        let nested = temp.path().join("a");
        fs::create_dir_all(&nested).unwrap();

        // A nonexistent boundary can't canonicalize; the raw fallback must
        // still terminate the walk if the raw forms match.
        let visited: Vec<PathBuf> = UpwardWalk::new(&nested).stop_at(temp.path()).collect();
        assert_eq!(visited, vec![nested.clone(), temp.path().to_path_buf()]);
        let visited: Vec<PathBuf> = UpwardWalk::new(&nested).stop_below(Some(ghost)).collect();
        assert!(
            visited.contains(&nested),
            "unrelated ghost boundary must not stop the walk early"
        );
    }

    #[test]
    fn start_equal_to_exclusive_boundary_yields_nothing_by_default() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&home).stop_below(Some(home.clone())).collect();
        assert!(
            visited.is_empty(),
            "without the start exemption, a walk starting at the boundary must yield nothing"
        );
    }

    #[test]
    fn start_equal_to_exclusive_boundary_is_yielded_with_always_yield_start() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&home)
            .stop_below(Some(home.clone()))
            .always_yield_start()
            .collect();
        assert_eq!(
            visited,
            vec![home],
            "the start directory must be probed even when it is the boundary, and the walk must end there"
        );
    }

    #[test]
    fn always_yield_start_does_not_exempt_ancestors_from_the_boundary() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("project");
        fs::create_dir_all(&project).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&project)
            .stop_below(Some(home.clone()))
            .always_yield_start()
            .collect();
        assert_eq!(
            visited,
            vec![project],
            "the exemption applies only to the start directory; the boundary still blocks ancestors"
        );
    }

    // Uses Unix symlinks; Windows symlink creation requires elevated privileges.
    #[cfg(unix)]
    #[test]
    fn always_yield_start_matches_boundary_through_differing_path_representations() {
        let temp = tempdir().unwrap();
        let real_home = temp.path().join("real-home");
        fs::create_dir_all(&real_home).unwrap();
        let link = temp.path().join("link-home");
        if std::os::unix::fs::symlink(&real_home, &link).is_err() {
            return;
        }

        // Start at the real path with the symlink as boundary: the canonical
        // comparison must recognize them as the same directory, yield the start,
        // and end the walk there (never probing above the home directory).
        let visited: Vec<PathBuf> = UpwardWalk::new(&real_home)
            .stop_below(Some(link))
            .always_yield_start()
            .collect();
        assert_eq!(visited, vec![real_home]);
    }

    #[test]
    fn depth_cap_bounds_the_walk() {
        let temp = tempdir().unwrap();
        let visited: Vec<PathBuf> = UpwardWalk::new(temp.path()).collect();
        assert!(visited.len() <= MAX_DEPTH);
    }

    #[test]
    fn relative_start_is_resolved_against_the_current_directory() {
        let visited: Vec<PathBuf> = UpwardWalk::new(Path::new("src")).take(2).collect();
        assert!(visited[0].is_absolute(), "relative starts must be absolutized");
        assert!(visited[0].ends_with("src"));
    }
}
