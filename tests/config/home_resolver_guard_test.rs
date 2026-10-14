//! Every home-directory consumer must go through `config::user_paths`.
//!
//! The Windows home policy (HOME honored when it is a native path, otherwise the
//! profile) lives in one resolver. A consumer that asks the OS directly gets the
//! profile only, and silently disagrees with the others about where `~` is.

use std::fs;
use std::path::Path;

const FORBIDDEN: &[&str] = &[
    "etcetera::home_dir",
    "std::env::home_dir",
    "env::home_dir",
    "choose_base_strategy",
    "BaseStrategy",
    "var_os(\"HOME\")",
    "var(\"HOME\")",
    "var_os(\"USERPROFILE\")",
    "var(\"USERPROFILE\")",
];

fn visit(dir: &Path, hits: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, hits);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let file = path.to_string_lossy().replace('\\', "/");
            if file.ends_with("config/user_paths.rs") {
                continue;
            }
            for (n, line) in fs::read_to_string(&path).unwrap().lines().enumerate() {
                let code = line.split("//").next().unwrap_or(line);
                if FORBIDDEN.iter().any(|f| code.contains(f)) {
                    hits.push(format!("{file}:{}: {}", n + 1, line.trim()));
                }
            }
        }
    }
}

#[test]
fn only_user_paths_reads_the_home_directory_from_the_environment() {
    let mut hits = Vec::new();
    visit(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut hits);
    assert!(
        hits.is_empty(),
        "these lines resolve the home directory outside config::user_paths:\n{}",
        hits.join("\n")
    );
}
