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

/// Why a walk produced no further directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalkStop {
    /// The walk reached the exclusive boundary (the home directory).
    HomeBoundary(PathBuf),
    /// The walk reached the exclusive boundary that is the user profile directory
    /// (Windows, when `HOME` names a different directory).
    ProfileBoundary(PathBuf),
    /// The walk yielded a git repository root, the last directory it visits.
    GitRoot(PathBuf),
    /// The walk yielded its inclusive stop root, the last directory it visits.
    StopRoot(PathBuf),
    /// The walk gave up after its maximum search depth.
    DepthCap,
    /// The walk yielded the filesystem root, which has no parent.
    FilesystemRoot,
}

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
    profile_stop: Option<Boundary>,
    stop_at_git_root: bool,
    inclusive_stop: Option<Boundary>,
    always_yield_start: bool,
    started: bool,
    stop: Option<WalkStop>,
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
            profile_stop: None,
            stop_at_git_root: false,
            inclusive_stop: None,
            always_yield_start: false,
            started: false,
            stop: None,
        }
    }

    /// End the walk *before* yielding `boundary` (typically the home
    /// directory). `None` leaves the walk unbounded.
    pub fn stop_below(mut self, boundary: Option<PathBuf>) -> Self {
        self.exclusive_stop = boundary.map(Boundary::new);
        self
    }

    /// A second exclusive boundary with the same semantics as [`Self::stop_below`],
    /// reported as [`WalkStop::ProfileBoundary`]: the user profile directory when it
    /// is a home in its own right next to the resolved home.
    pub fn stop_below_profile(mut self, boundary: Option<PathBuf>) -> Self {
        self.profile_stop = boundary.map(Boundary::new);
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

    /// Why the walk ended, once it has. `None` while directories remain, and
    /// when the caller stopped iterating early (for instance on a match).
    pub fn stop_reason(&self) -> Option<&WalkStop> {
        self.stop.as_ref()
    }
}

impl Iterator for UpwardWalk {
    type Item = PathBuf;

    fn next(&mut self) -> Option<PathBuf> {
        let current = self.next.take()?;
        if self.remaining == 0 {
            log::debug!("[rumdl-config] Maximum upward traversal depth reached");
            self.stop = Some(WalkStop::DepthCap);
            return None;
        }
        self.remaining -= 1;

        let is_start = !self.started;
        self.started = true;

        let hit = if self.exclusive_stop.as_ref().is_some_and(|b| b.matches(&current)) {
            Some(WalkStop::HomeBoundary(current.clone()))
        } else if self.profile_stop.as_ref().is_some_and(|b| b.matches(&current)) {
            Some(WalkStop::ProfileBoundary(current.clone()))
        } else {
            None
        };
        if let Some(stop) = hit {
            self.stop = Some(stop);
            if !(is_start && self.always_yield_start) {
                return None;
            }
            // The start is spared from the boundary, but the walk must still
            // end here: directories above the boundary are never probed.
            return Some(current);
        }

        if self.stop_at_git_root && current.join(".git").exists() {
            self.stop = Some(WalkStop::GitRoot(current.clone()));
        } else if self.inclusive_stop.as_ref().is_some_and(|b| b.matches(&current)) {
            self.stop = Some(WalkStop::StopRoot(current.clone()));
        } else {
            self.next = current.parent().map(Path::to_path_buf);
            if self.next.is_none() {
                self.stop = Some(WalkStop::FilesystemRoot);
            }
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
    fn profile_boundary_is_not_yielded_and_is_reported_distinctly() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let profile = temp.path().join("profile");
        let project = profile.join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&project).unwrap();

        let mut walk = UpwardWalk::new(&project)
            .stop_below(Some(home))
            .stop_below_profile(Some(profile.clone()));
        let visited: Vec<PathBuf> = walk.by_ref().collect();
        assert_eq!(
            visited,
            vec![project],
            "the profile directory itself must not be probed"
        );
        assert_eq!(walk.stop_reason(), Some(&WalkStop::ProfileBoundary(profile)));
    }

    #[test]
    fn home_boundary_still_reports_the_home_reason_next_to_a_profile_boundary() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let profile = temp.path().join("profile");
        let project = home.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&profile).unwrap();

        let mut walk = UpwardWalk::new(&project)
            .stop_below(Some(home.clone()))
            .stop_below_profile(Some(profile));
        let visited: Vec<PathBuf> = walk.by_ref().collect();
        assert_eq!(visited, vec![project]);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::HomeBoundary(home)));
    }

    #[test]
    fn a_start_in_the_profile_is_yielded_when_the_start_is_spared_and_the_walk_ends_there() {
        let temp = tempdir().unwrap();
        let profile = temp.path().join("profile");
        fs::create_dir_all(&profile).unwrap();

        let mut walk = UpwardWalk::new(&profile)
            .stop_below_profile(Some(profile.clone()))
            .always_yield_start();
        let visited: Vec<PathBuf> = walk.by_ref().collect();
        assert_eq!(visited, vec![profile.clone()]);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::ProfileBoundary(profile)));
    }

    #[test]
    fn a_project_below_the_profile_is_still_walked_up_to_the_profile() {
        let temp = tempdir().unwrap();
        let profile = temp.path().join("profile");
        let nested = profile.join("proj").join("docs");
        fs::create_dir_all(&nested).unwrap();

        let visited: Vec<PathBuf> = UpwardWalk::new(&nested)
            .stop_below_profile(Some(profile.clone()))
            .collect();
        assert_eq!(visited, vec![nested, profile.join("proj")]);
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

    #[test]
    fn stop_reason_names_why_each_walk_ended() {
        let temp = tempdir().unwrap();
        let home = temp.path().join("home");
        let project = home.join("project");
        let repo = temp.path().join("repo");
        let repo_docs = repo.join("docs");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(&repo_docs).unwrap();

        let mut walk = UpwardWalk::new(&project).stop_below(Some(home.clone()));
        assert_eq!(walk.stop_reason(), None, "nothing has been walked yet");
        walk.by_ref().for_each(drop);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::HomeBoundary(home.clone())));

        // The start directory is exempt, but the walk still ends at the boundary.
        let mut walk = UpwardWalk::new(&home)
            .stop_below(Some(home.clone()))
            .always_yield_start();
        assert_eq!(walk.by_ref().count(), 1);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::HomeBoundary(home.clone())));

        let mut walk = UpwardWalk::new(&repo_docs).stop_at_git_root();
        walk.by_ref().for_each(drop);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::GitRoot(repo.clone())));

        let mut walk = UpwardWalk::new(&repo_docs).stop_at(&repo);
        walk.by_ref().for_each(drop);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::StopRoot(repo.clone())));

        let mut walk = UpwardWalk::new(&project);
        walk.by_ref().for_each(drop);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::FilesystemRoot));
    }

    #[test]
    fn stop_reason_is_depth_cap_when_the_walk_runs_out_of_depth() {
        let mut path = PathBuf::from(if cfg!(windows) { r"C:" } else { "/" });
        for i in 0..(MAX_DEPTH + 5) {
            path.push(format!("d{i}"));
        }
        let mut walk = UpwardWalk::new(&path);
        assert_eq!(walk.by_ref().count(), MAX_DEPTH);
        assert_eq!(walk.stop_reason(), Some(&WalkStop::DepthCap));
    }
}
