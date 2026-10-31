use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const DOCUMENT_COUNT: usize = 2000;
const RUNS: usize = 7;
/// Cheap documents are the worst case for the lint cache: replaying a hit
/// costs about as much as linting them, so any fixed per-hit overhead shows up
/// as a fully cached run falling behind an uncached one.
const MAX_WARM_TO_UNCACHED_RATIO: f64 = 1.0;

/// One directory per document, with findings and cross-file links in each, so
/// every cached run replays warnings and reuses the workspace index.
fn write_corpus(root: &Path) {
    for index in 0..DOCUMENT_COUNT {
        let dir = root.join(format!("docs/section{}/page{index}", index % 40));
        std::fs::create_dir_all(&dir).unwrap();
        let next = (index + 1) % DOCUMENT_COUNT;
        let target = format!("../../section{}/page{next}/index.md", next % 40);
        let long_line = "This line is deliberately long so that the line length rule reports it. ".repeat(2);
        let content = format!(
            "# Page {index}\n\n{long_line}\n\n{long_line}\n\n\
             See [next]({target}#page-{next}) and [missing]({target}#nope).\n"
        );
        std::fs::write(dir.join("index.md"), content).unwrap();
    }
}

fn check(root: &Path, cache_dir: &Path, extra: &[&str]) -> (Duration, String) {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(root)
        .env("RUMDL_CACHE_DIR", cache_dir)
        .args(["check", "--no-config", "."])
        .args(extra)
        .output()
        .expect("failed to execute rumdl");
    let elapsed = started.elapsed();
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut lines: Vec<_> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(": ["))
        .map(str::to_owned)
        .collect();
    lines.sort();
    (elapsed, lines.join("\n"))
}

#[test]
fn warm_cache_run_is_not_slower_than_uncached_performance() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("corpus");
    let cache_dir = temp.path().join("cache");
    write_corpus(&root);

    let (_, uncached_findings) = check(&root, &cache_dir, &["--no-cache"]);
    let (_, populated_findings) = check(&root, &cache_dir, &[]);
    assert_eq!(populated_findings, uncached_findings);
    assert!(
        uncached_findings.contains("MD051"),
        "cross-file findings must be exercised"
    );

    // Interleaved runs share whatever load the machine is under; the fastest
    // run of each kind is the least disturbed measurement of it.
    let mut warm = Duration::MAX;
    let mut uncached = Duration::MAX;
    for _ in 0..RUNS {
        let (elapsed, findings) = check(&root, &cache_dir, &[]);
        assert_eq!(
            findings, uncached_findings,
            "a cached run must report the same findings"
        );
        warm = warm.min(elapsed);
        uncached = uncached.min(check(&root, &cache_dir, &["--no-cache"]).0);
    }

    let ratio = warm.as_secs_f64() / uncached.as_secs_f64();
    eprintln!("{DOCUMENT_COUNT} documents: warm {warm:?}, uncached {uncached:?}, ratio {ratio:.2}");
    assert!(
        ratio <= MAX_WARM_TO_UNCACHED_RATIO,
        "a fully cached run took {warm:?}, {ratio:.2}x the {uncached:?} of an uncached run"
    );
}
