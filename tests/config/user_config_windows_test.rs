//! Windows-only checks of user-level discovery with the real binary.
//!
//! These compile on every host (`cargo check --tests --target x86_64-pc-windows-msvc`)
//! but only execute on Windows.
#![cfg(windows)]

use std::fs;
use std::process::Command;

struct Case {
    root: tempfile::TempDir,
}

impl Case {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn write(&self, rel: &str, content: &str) -> std::path::PathBuf {
        let path = self.root.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn run(&self, home: Option<&std::path::Path>, profile: &std::path::Path, args: &[&str]) -> String {
        // Independent of the HOME under test, which may be deliberately unusable.
        let cwd = self.root.path().join("cwd");
        fs::create_dir_all(&cwd).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_rumdl"));
        cmd.current_dir(&cwd)
            .args(args)
            .env("USERPROFILE", profile)
            .env("APPDATA", self.root.path().join("no-appdata"))
            .env_remove("XDG_CONFIG_HOME");
        match home {
            Some(h) => cmd.env("HOME", h),
            None => cmd.env_remove("HOME"),
        };
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }
}

#[test]
fn home_dot_config_rumdl_is_found_under_the_profile() {
    let case = Case::new();
    let user = case.write("profile/.config/rumdl/rumdl.toml", "[MD013]\nline-length = 71\n");
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();
    let out = case.run(None, &profile, &["config", "file"]);
    assert_eq!(fs::canonicalize(out.trim()).unwrap(), user);
}

#[test]
fn a_native_home_that_differs_from_the_profile_is_honored() {
    let case = Case::new();
    let user = case.write("home/.config/rumdl/rumdl.toml", "[MD013]\nline-length = 72\n");
    case.write("profile/.config/rumdl/rumdl.toml", "[MD013]\nline-length = 99\n");
    let home = fs::canonicalize(case.root.path().join("home")).unwrap();
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();
    let out = case.run(Some(&home), &profile, &["config", "file"]);
    assert_eq!(fs::canonicalize(out.trim()).unwrap(), user);
}

#[test]
fn an_msys_style_home_is_ignored_and_reported() {
    let case = Case::new();
    let user = case.write("profile/.config/rumdl/rumdl.toml", "[MD013]\nline-length = 73\n");
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();
    let out = case.run(
        Some(std::path::Path::new("/c/Users/nobody")),
        &profile,
        &["config", "file", "--explain"],
    );
    assert!(out.contains("ignored HOME="), "{out}");
    assert!(out.contains("MSYS-style"), "{out}");
    assert!(out.contains(&user.display().to_string()), "{out}");
}

#[test]
fn the_home_directory_dotfile_still_applies_and_loses_to_the_config_dir() {
    let case = Case::new();
    case.write("profile/.rumdl.toml", "[MD013]\nline-length = 74\n");
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();
    let out = case.run(None, &profile, &["config", "get", "MD013.line-length"]);
    assert!(out.contains("= 74 "), "{out}");
    case.write("profile/.config/rumdl/.rumdl.toml", "[MD013]\nline-length = 75\n");
    let out = case.run(None, &profile, &["config", "get", "MD013.line-length"]);
    assert!(out.contains("= 75 "), "{out}");
}

#[test]
fn tilde_in_an_exclude_pattern_follows_the_resolved_home() {
    let case = Case::new();
    // Both homes hold the same tree; only the HOME one carries the config, and its
    // `~/.cursor/plans` exclude must resolve against HOME, not the profile.
    let violation = "#  Heading with two spaces\n";
    for dir in ["home", "profile"] {
        case.write(&format!("{dir}/.cursor/plans/plan.md"), violation);
        case.write(&format!("{dir}/docs/guide.md"), violation);
    }
    case.write("home/.rumdl.toml", "[global]\nexclude = [\"~/.cursor/plans\"]\n");
    let home = fs::canonicalize(case.root.path().join("home")).unwrap();
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "."])
        .current_dir(&home)
        .env("HOME", &home)
        .env("USERPROFILE", &profile)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("guide.md"),
        "control: the non-excluded file is linted: {text}"
    );
    assert!(
        !text.contains("plan.md"),
        "~ must resolve to HOME for the exclude: {text}"
    );
}

#[test]
fn a_config_only_in_the_profile_is_still_found_when_home_names_another_directory() {
    let case = Case::new();
    let user = case.write("profile/.rumdl.toml", "[MD013]\nline-length = 76\n");
    fs::create_dir_all(case.root.path().join("home")).unwrap();
    let home = fs::canonicalize(case.root.path().join("home")).unwrap();
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();

    let out = case.run(Some(&home), &profile, &["config", "file"]);
    assert_eq!(fs::canonicalize(out.trim()).unwrap(), user);

    let out = case.run(Some(&home), &profile, &["config", "file", "--explain"]);
    assert!(out.contains("profile fallback"), "{out}");
    assert!(out.contains(&user.display().to_string()), "{out}");
}

#[test]
fn the_home_config_wins_over_the_profile_fallback_and_shadows_it() {
    let case = Case::new();
    let winner = case.write("home/.rumdl.toml", "[MD013]\nline-length = 77\n");
    let shadowed = case.write("profile/.rumdl.toml", "[MD013]\nline-length = 78\n");
    let home = fs::canonicalize(case.root.path().join("home")).unwrap();
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();

    let out = case.run(Some(&home), &profile, &["config", "file", "--explain"]);
    assert!(
        out.contains(&format!("selected user config: {}", winner.display())),
        "{out}"
    );
    assert!(out.contains(&shadowed.display().to_string()), "{out}");
}

#[test]
fn a_home_spelled_differently_from_the_profile_lists_the_profile_once() {
    let case = Case::new();
    case.write("profile/.rumdl.toml", "[MD013]\nline-length = 79\n");
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();
    let plain = profile.display().to_string();
    let plain = plain.trim_start_matches(r"\\?\");
    let respelled = format!("{}\\", plain.to_uppercase().replace('\\', "/"));

    let out = case.run(
        Some(std::path::Path::new(&respelled)),
        &profile,
        &["config", "file", "--explain"],
    );
    assert!(out.contains("home directory: "), "{out}");
    assert!(
        !out.contains("ignored HOME"),
        "the respelled HOME must be accepted: {out}"
    );
    assert!(!out.contains("profile fallback"), "{out}");
}

#[test]
fn a_profile_dotfile_is_a_user_config_and_not_a_project_config_when_home_names_another_directory() {
    let case = Case::new();
    case.write("profile/.rumdl.toml", "[MD013]\nline-length = 81\n");
    case.write("profile/notes/doc.md", "# Doc\n");
    case.write("home/.rumdl.toml", "[MD013]\nline-length = 82\n");
    let home = fs::canonicalize(case.root.path().join("home")).unwrap();
    let profile = fs::canonicalize(case.root.path().join("profile")).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["config", "file", "--explain"])
        .current_dir(profile.join("notes"))
        .env("HOME", &home)
        .env("USERPROFILE", &profile)
        .env("APPDATA", case.root.path().join("no-appdata"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("reached the profile directory (fallback home)"), "{text}");
    assert!(
        text.contains(&format!("selected user config: {}", home.join(".rumdl.toml").display())),
        "the HOME config outranks the profile file as a user config: {text}"
    );
}
