//! Real-binary tests for user-level configuration discovery and `config file --explain`.
//!
//! Every scenario runs in a throwaway HOME and never touches the real one. Each config
//! file carries a distinct `line-length`, so which file won is read from the effective
//! value (`config get`) and not only from the path the trace claims.

#![cfg(not(windows))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct Env {
    root: TempDir,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("home")).unwrap();
        Self { root }
    }

    fn home(&self) -> PathBuf {
        fs::canonicalize(self.root.path().join("home")).unwrap()
    }

    fn write(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.root.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn toml_with_width(&self, rel: &str, width: u32) -> PathBuf {
        self.write(rel, &format!("[MD013]\nline-length = {width}\n"))
    }

    fn rumdl(&self, cwd: &str, xdg: Option<&str>, args: &[&str]) -> Output {
        let cwd = self.root.path().join(cwd);
        fs::create_dir_all(&cwd).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_rumdl"));
        cmd.current_dir(cwd)
            .args(args)
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env_remove("XDG_CONFIG_HOME")
            .env("APPDATA", self.root.path().join("no-appdata"))
            .env_remove("RUST_LOG")
            .env("NO_COLOR", "1");
        if let Some(xdg) = xdg {
            cmd.env("XDG_CONFIG_HOME", self.root.path().join(xdg));
        }
        cmd.output().expect("failed to run rumdl")
    }

    fn stdout(&self, cwd: &str, xdg: Option<&str>, args: &[&str]) -> String {
        let out = self.rumdl(cwd, xdg, args);
        assert!(
            out.status.success(),
            "rumdl {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// The `line-length` rumdl resolves for MD013 in `cwd`.
    fn width(&self, cwd: &str, xdg: Option<&str>) -> String {
        let out = self.stdout(cwd, xdg, &["config", "get", "MD013.line-length"]);
        out.split_once(" = ")
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("unexpected config get output: {out}"))
            .to_string()
    }

    fn selected_path(&self, cwd: &str, xdg: Option<&str>) -> String {
        self.stdout(cwd, xdg, &["config", "file"]).trim().to_string()
    }
}

fn same(a: &str, b: &Path) -> bool {
    fs::canonicalize(a).map(|p| p == b).unwrap_or(false)
}

#[test]
fn user_config_applies_when_no_project_config_exists() {
    let env = Env::new();
    let user = env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    assert_eq!(env.width("home/proj", None), "71");
    assert!(same(&env.selected_path("home/proj", None), &user));
}

#[test]
fn negative_control_without_any_user_config_the_default_width_applies() {
    let env = Env::new();
    assert_eq!(env.width("home/proj", None), "80");
}

#[test]
fn xdg_config_home_elsewhere_wins_and_home_dot_config_is_not_a_second_source() {
    let env = Env::new();
    env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let xdg = env.toml_with_width("xdg/rumdl/rumdl.toml", 72);
    assert_eq!(env.width("home/proj", Some("xdg")), "72");
    assert!(same(&env.selected_path("home/proj", Some("xdg")), &xdg));
}

#[test]
fn home_dot_config_is_not_probed_when_xdg_points_elsewhere() {
    let env = Env::new();
    env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    fs::create_dir_all(env.root.path().join("xdg")).unwrap();
    assert_eq!(
        env.width("home/proj", Some("xdg")),
        "80",
        "only the platform config dir is consulted on this platform"
    );
}

#[test]
fn platform_dir_beats_home_dotfiles_and_dotfiles_keep_their_order() {
    let env = Env::new();
    env.toml_with_width("home/rumdl.toml", 73);
    assert_eq!(env.width("home/proj", None), "73");
    env.toml_with_width("home/.rumdl.toml", 74);
    assert_eq!(env.width("home/proj", None), "74");
    env.toml_with_width("home/.config/rumdl/.rumdl.toml", 75);
    assert_eq!(env.width("home/proj", None), "75");
}

#[test]
fn a_project_config_suppresses_the_user_config() {
    let env = Env::new();
    env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let project = env.toml_with_width("home/proj/.rumdl.toml", 91);
    assert_eq!(env.width("home/proj/sub", None), "91");
    assert!(same(&env.selected_path("home/proj/sub", None), &project));
}

#[test]
fn a_markdownlint_config_overlays_the_user_base() {
    let env = Env::new();
    env.write(
        "home/.config/rumdl/rumdl.toml",
        "[MD013]\nline-length = 71\n\n[MD004]\nstyle = \"asterisk\"\n",
    );
    env.write("home/proj/.markdownlint.json", "{\"MD013\": {\"line_length\": 92}}\n");
    assert_eq!(env.width("home/proj", None), "92", "markdownlint wins where it speaks");
    let style = env.stdout("home/proj", None, &["config", "get", "MD004.style"]);
    assert!(style.contains("asterisk"), "user base still supplies MD004: {style}");
}

#[test]
fn explicit_config_is_standalone_and_isolated_means_defaults() {
    let env = Env::new();
    env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let explicit = env.write("explicit.toml", "[global]\ndisable = [\"MD041\"]\n");

    let out = env.stdout(
        "home/proj",
        None,
        &["config", "file", "--config", explicit.to_str().unwrap()],
    );
    assert!(same(out.trim(), &explicit));
    let width = env.stdout(
        "home/proj",
        None,
        &[
            "config",
            "get",
            "MD013.line-length",
            "--config",
            explicit.to_str().unwrap(),
        ],
    );
    assert!(width.contains("= 80 "), "user config must not leak in: {width}");

    let isolated = env.stdout("home/proj", None, &["config", "file", "--isolated"]);
    assert!(isolated.contains("No configuration file loaded"));
}

#[test]
fn a_project_search_started_at_home_still_reads_home_but_stops_below_it() {
    let env = Env::new();
    // A config above home must never be picked up by a walk that stops at home.
    env.toml_with_width(".rumdl.toml", 99);
    let at_home = env.write("home/.rumdl.toml", "[MD013]\nline-length = 77\n");
    assert_eq!(env.width("home", None), "77");
    assert!(same(&env.selected_path("home", None), &at_home));
    assert_eq!(
        env.width("home/proj", None),
        "77",
        "the home dotfile is user-level here"
    );
}

// ---- config file --explain ----

#[test]
fn explain_reports_the_home_the_start_dir_and_a_found_user_file() {
    let env = Env::new();
    let user = env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let text = env.stdout("home/proj", None, &["config", "file", "--explain"]);
    assert!(text.contains("start directory:"), "{text}");
    assert!(text.contains("home directory:"), "{text}");
    assert!(
        text.contains("PlatformDefault") || text.contains("HOME or the user database"),
        "{text}"
    );
    assert!(
        text.contains(&format!("selected user config: {}", user.display())),
        "{text}"
    );
    assert!(
        text.contains("search ended: reached the home directory boundary"),
        "{text}"
    );
    assert!(!text.contains("line-length"), "contents are never printed: {text}");
}

#[test]
fn explain_states_that_a_project_config_skipped_the_user_search() {
    let env = Env::new();
    env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let project = env.toml_with_width("home/proj/.rumdl.toml", 91);
    let text = env.stdout("home/proj", None, &["config", "file", "--explain"]);
    assert!(
        text.contains(&format!("selected project config: {}", project.display())),
        "{text}"
    );
    assert!(
        text.contains("user-level search skipped: a project config was found"),
        "{text}"
    );
    assert!(!text.contains("selected user config"), "{text}");
}

#[test]
fn explain_lists_the_files_a_winner_shadows() {
    let env = Env::new();
    let winner = env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let shadowed = env.toml_with_width("home/.rumdl.toml", 72);
    let text = env.stdout("home/proj", None, &["config", "file", "--explain"]);
    assert!(
        text.contains(&format!("selected user config: {}", winner.display())),
        "{text}"
    );
    assert!(
        text.contains(&format!("shadows (lower priority): {}", shadowed.display())),
        "{text}"
    );
}

#[test]
fn explain_reports_a_pyproject_without_a_rumdl_section_as_ignored() {
    let env = Env::new();
    env.write("home/.config/rumdl/pyproject.toml", "[project]\nname = \"x\"\n");
    let text = env.stdout("home/proj", None, &["config", "file", "--explain"]);
    assert!(
        text.contains("pyproject.toml: ignored (pyproject.toml without a rumdl section)"),
        "{text}"
    );
    assert!(text.contains("selected: none (using defaults)"), "{text}");
}

#[test]
fn explain_with_nothing_found_lists_every_probe_as_missing() {
    let env = Env::new();
    let text = env.stdout("home/proj", None, &["config", "file", "--explain"]);
    assert!(text.contains("rumdl.toml: missing"), "{text}");
    assert!(text.contains("selected: none (using defaults)"), "{text}");
}

#[test]
fn explain_stops_at_the_git_root() {
    let env = Env::new();
    fs::create_dir_all(env.root.path().join("home/repo/.git")).unwrap();
    fs::create_dir_all(env.root.path().join("home/repo/docs")).unwrap();
    let text = env.stdout("home/repo/docs", None, &["config", "file", "--explain"]);
    assert!(text.contains("reached the git repository root"), "{text}");
}

#[test]
fn explain_json_is_valid_and_names_the_selection() {
    let env = Env::new();
    let user = env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let out = env.stdout("home/proj", None, &["config", "--output", "json", "file", "--explain"]);
    let value: serde_json::Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
    let selected = value["selected"].as_array().expect("selected array");
    assert_eq!(selected.len(), 1, "{out}");
    assert_eq!(selected[0]["stage"], "user");
    assert!(same(selected[0]["path"].as_str().unwrap(), &user));
    assert!(value["events"].as_array().unwrap().iter().any(|e| e["event"] == "home"));
}

#[test]
fn plain_config_file_stdout_is_only_the_path_and_the_hint_goes_to_stderr() {
    let env = Env::new();
    let user = env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let out = env.rumdl("home/proj", None, &["config", "file"]);
    assert!(same(String::from_utf8_lossy(&out.stdout).trim(), &user));
    assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));

    let env = Env::new();
    let out = env.rumdl("home/proj", None, &["config", "file"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "No configuration file found (using defaults)"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("rumdl config file --explain"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn debug_logging_carries_the_same_trace() {
    let env = Env::new();
    env.toml_with_width("home/.config/rumdl/rumdl.toml", 71);
    let cwd = env.root.path().join("home/proj");
    fs::create_dir_all(&cwd).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(&cwd)
        .args(["config", "file"])
        .env("HOME", env.home())
        .env("USERPROFILE", env.home())
        .env_remove("XDG_CONFIG_HOME")
        .env("RUST_LOG", "debug")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[rumdl-config] discovery: selected user config"),
        "{stderr}"
    );
}

#[test]
fn explain_does_not_report_an_unloadable_markdownlint_config_as_selected() {
    let env = Env::new();
    let broken = env.write("home/proj/.markdownlint.json", "{ this is not json\n");
    let text = env.stdout("home/proj", None, &["config", "file", "--explain"]);
    assert!(!text.contains("selected markdownlint config"), "{text}");
    assert!(
        text.contains(&format!(
            "{}: not usable (could not be loaded as a markdownlint config)",
            broken.display()
        )),
        "{text}"
    );
    assert!(text.contains("selected: none (using defaults)"), "{text}");
    assert!(!text.contains("this is not json"), "file text is never echoed: {text}");

    let ok = env.write("home/proj2/.markdownlint.json", "{\"MD013\": {\"line_length\": 92}}\n");
    let text = env.stdout("home/proj2", None, &["config", "file", "--explain"]);
    assert!(
        text.contains(&format!("selected markdownlint config: {}", ok.display())),
        "{text}"
    );
}
