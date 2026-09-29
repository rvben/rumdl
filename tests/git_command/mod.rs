//! The only way tests spawn `git`.
//!
//! Tests run inside git hooks (the full suite is a pre-push hook), and a hook
//! in a linked worktree inherits `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE`
//! pointing at that worktree. A `git init`, `git add` or `git apply` run for a
//! temp directory with those set acts on the developer's repository instead:
//! `git init` writes `core.worktree` (or `core.bare = true`) into its shared
//! config, which breaks the main checkout with "fatal: this operation must be
//! run in a work tree" once the worktree is removed.

use std::path::Path;
use std::process::Command;

/// The variables `git rev-parse --local-env-vars` reports: everything that
/// points git at a particular repository rather than the one around `dir`.
const REPOSITORY_ENV: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// A `git` command that runs in `dir` and acts on the repository found there.
pub fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir);
    for name in REPOSITORY_ENV {
        command.env_remove(name);
    }
    command
}

/// Runs `git args` in `dir` and panics with git's stderr unless it succeeds.
pub fn run_git(dir: &Path, args: &[&str]) {
    let output = git(dir).args(args).output().expect("failed to spawn git");
    assert!(
        output.status.success(),
        "git {} failed in {}: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn shared_config(repo: &Path) -> String {
        fs::read_to_string(repo.join(".git/config")).unwrap()
    }

    /// A repository with a linked worktree, and the environment a git hook
    /// running in that worktree receives.
    fn repository_with_hook_env() -> (tempfile::TempDir, PathBuf, Vec<(&'static str, PathBuf)>) {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        let worktree = root.path().join("worktree");
        fs::create_dir(&repo).unwrap();
        run_git(&repo, &["init", "-q"]);
        run_git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
        );
        run_git(&repo, &["worktree", "add", "-q", worktree.to_str().unwrap()]);
        let admin = repo.join(".git/worktrees/worktree");
        let env = vec![
            ("GIT_DIR", admin.clone()),
            ("GIT_WORK_TREE", worktree),
            ("GIT_INDEX_FILE", admin.join("index")),
        ];
        (root, repo, env)
    }

    #[test]
    fn git_in_a_temp_dir_leaves_the_hook_repository_alone() {
        let (root, repo, hook_env) = repository_with_hook_env();
        let before = shared_config(&repo);

        // Control: plain git under the hook environment re-initializes the hook's
        // repository, proving this test can observe the corruption.
        let raw = root.path().join("raw");
        fs::create_dir(&raw).unwrap();
        let mut command = Command::new("git");
        command.current_dir(&raw).args(["init", "-q"]).envs(hook_env.clone());
        assert!(command.status().unwrap().success());
        let corrupted = shared_config(&repo);
        assert!(
            corrupted.contains("worktree = "),
            "control did not corrupt:\n{corrupted}"
        );
        assert!(!raw.join(".git").exists());
        fs::write(repo.join(".git/config"), &before).unwrap();

        // The real path: a test process that inherits the hook environment and
        // calls `run_git`. Re-run this test binary with only the probe selected.
        let isolated = root.path().join("isolated");
        fs::create_dir(&isolated).unwrap();
        // libtest names a test by its module path without the crate name.
        let (_crate, module) = module_path!().split_once("::").unwrap();
        let probe = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{module}::probe_init"),
                "--ignored",
                "--test-threads=1",
            ])
            .env(PROBE_DIR, &isolated)
            .envs(hook_env)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&probe.stdout);
        assert!(
            probe.status.success() && stdout.contains("1 passed"),
            "probe did not run:\n{stdout}\n{}",
            String::from_utf8_lossy(&probe.stderr)
        );

        assert_eq!(shared_config(&repo), before);
        assert!(
            isolated.join(".git").is_dir(),
            "git init did not create the temp repository"
        );
    }

    const PROBE_DIR: &str = "RUMDL_GIT_COMMAND_PROBE_DIR";

    /// Run only by the test above, in a child process holding a hook environment.
    #[test]
    #[ignore = "spawned by git_in_a_temp_dir_leaves_the_hook_repository_alone"]
    fn probe_init() {
        let dir = std::env::var_os(PROBE_DIR).expect("run only as a probe");
        run_git(Path::new(&dir), &["init", "-q"]);
    }

    #[test]
    fn every_repository_variable_git_knows_is_removed() {
        let output = Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let missing: Vec<_> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter(|name| !REPOSITORY_ENV.contains(name))
            .map(str::to_owned)
            .collect();
        assert!(missing.is_empty(), "add to REPOSITORY_ENV: {missing:?}");
    }

    /// Any other `Command::new("git")` would inherit a hook's environment.
    #[test]
    fn tests_spawn_git_only_through_this_module() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let this_file = manifest.join(file!());
        let mut offenders = Vec::new();
        let mut pending = vec![manifest.join("src"), manifest.join("tests")];
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") && path != this_file {
                    let source = fs::read_to_string(&path).unwrap();
                    if source.contains(concat!("Command::new(", "\"git\")")) {
                        offenders.push(path.strip_prefix(manifest).unwrap().display().to_string());
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "use tests/git_command/mod.rs instead: {offenders:?}"
        );
    }
}
