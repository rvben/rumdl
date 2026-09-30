//! Corpus sweep: run the MD013 reflow semantics oracle over real Markdown.
//!
//! Every file is reflowed under each reflow mode at line lengths 40 and 80, and
//! each result must render the same as the source and survive a second pass
//! unchanged (see `fuzz/oracle/reflow_semantics.rs`). Violations are written to
//! the output directory, one subdirectory per violation, with the input, the
//! output, the rendered HTML where it differs, and the `rumdl fmt` command that
//! reproduces it.
//!
//! Ignored by default because it reads files outside the repository; run it
//! with `make reflow-sweep`. Environment:
//!
//! - `RUMDL_REFLOW_SWEEP_PATHS`: files and directories to sweep, separated like
//!   `PATH` (required).
//! - `RUMDL_REFLOW_SWEEP_OUT`: where violations go (default
//!   `target/reflow-sweep`, cleared first).
//! - `RUMDL_REFLOW_SWEEP_BIN`: a `rumdl` binary to run as `<bin> fmt` instead
//!   of reflowing in-process. A sweep that finds nothing is only meaningful once
//!   it has found known defects in a build of a commit that still has them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::reflow_semantics::{Mode, Outcome, ReflowSettings, Violation, ViolationKind, check, check_with};

const LINE_LENGTHS: [u64; 2] = [40, 80];
const MAX_FILE_BYTES: u64 = 512 * 1024;

fn markdown_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        for entry in ignore::WalkBuilder::new(path).build().flatten() {
            let path = entry.path();
            let is_markdown = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"));
            let small_enough = entry.metadata().is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE_BYTES);
            if is_markdown && small_enough {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Reflow through a `rumdl` binary, the way a user's `rumdl fmt` does.
fn reflow_with_binary(binary: &Path, scratch: &Path, input: &str, settings: &ReflowSettings) -> Result<String, String> {
    let file = scratch.join("input.md");
    fs::write(&file, input).map_err(|e| e.to_string())?;
    let output = Command::new(binary)
        .arg("fmt")
        .args(settings.cli_args())
        .arg(&file)
        .output()
        .map_err(|e| format!("running {}: {e}", binary.display()))?;
    match output.status.code() {
        Some(0 | 1) => fs::read_to_string(&file).map_err(|e| e.to_string()),
        status => Err(format!(
            "rumdl fmt exited with {status:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )),
    }
}

fn record_violation(dir: &Path, file: &Path, settings: &ReflowSettings, violation: &Violation) {
    fs::create_dir_all(dir).expect("create violation directory");
    fs::write(dir.join("input.md"), &violation.input).expect("write input");
    fs::write(dir.join("output.md"), &violation.output).expect("write output");
    match &violation.kind {
        ViolationKind::RenderChanged { before, after, .. } => {
            fs::write(dir.join("before.html"), before).expect("write before.html");
            fs::write(dir.join("after.html"), after).expect("write after.html");
        }
        ViolationKind::NotIdempotent { second } => {
            fs::write(dir.join("second.md"), second).expect("write second pass");
        }
        ViolationKind::FixFailed(error) => {
            fs::write(dir.join("error.txt"), error).expect("write error");
        }
    }
    let repro = format!(
        "source: {}\nviolation: {}\nreproduce: rumdl fmt {} input.md\n",
        file.display(),
        violation.label(),
        settings.shell_args()
    );
    fs::write(dir.join("repro.txt"), repro).expect("write repro");
}

#[test]
#[ignore = "sweeps files outside the repository; run with `make reflow-sweep`"]
fn reflow_sweep() {
    let paths: Vec<PathBuf> = std::env::split_paths(
        &std::env::var_os("RUMDL_REFLOW_SWEEP_PATHS").expect("set RUMDL_REFLOW_SWEEP_PATHS to the files to sweep"),
    )
    .collect();
    let out =
        std::env::var_os("RUMDL_REFLOW_SWEEP_OUT").map_or_else(|| PathBuf::from("target/reflow-sweep"), PathBuf::from);
    let binary = std::env::var_os("RUMDL_REFLOW_SWEEP_BIN")
        .filter(|bin| !bin.is_empty())
        .map(PathBuf::from);

    let files = markdown_files(&paths);
    assert!(!files.is_empty(), "no Markdown files under {paths:?}");
    let settings: Vec<ReflowSettings> = Mode::ALL
        .into_iter()
        .flat_map(|mode| LINE_LENGTHS.map(|len| ReflowSettings::with_mode(mode, len)))
        .collect();
    if out.exists() {
        fs::remove_dir_all(&out).expect("clear the previous sweep's output");
    }
    fs::create_dir_all(&out).expect("create output directory");

    let unchanged = AtomicUsize::new(0);
    let rewritten = AtomicUsize::new(0);
    let violations: Mutex<BTreeMap<String, usize>> = Mutex::default();
    let written = AtomicUsize::new(0);
    let next_file = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, usize::from);

    std::thread::scope(|scope| {
        for worker in 0..workers {
            let scratch = out.join(format!(".scratch-{worker}"));
            let (files, settings, binary, out) = (&files, &settings, &binary, &out);
            let (unchanged, rewritten, violations, written, next_file) =
                (&unchanged, &rewritten, &violations, &written, &next_file);
            scope.spawn(move || {
                fs::create_dir_all(&scratch).expect("create scratch directory");
                while let Some(file) = files.get(next_file.fetch_add(1, Ordering::Relaxed)) {
                    let Ok(content) = fs::read_to_string(file) else {
                        continue;
                    };
                    for setting in settings {
                        let result = match binary {
                            Some(binary) => check_with(&content, setting, |input, s| {
                                reflow_with_binary(binary, &scratch, input, s)
                            }),
                            None => check(&content, setting),
                        };
                        match result {
                            Ok(Outcome::Unchanged) => {
                                unchanged.fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(Outcome::Rewritten) => {
                                rewritten.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(violation) => {
                                *violations.lock().unwrap().entry(violation.label()).or_default() += 1;
                                let n = written.fetch_add(1, Ordering::Relaxed);
                                let label = violation.label().replace([':', '+'], "-");
                                record_violation(&out.join(format!("{n:05}-{label}")), file, setting, &violation);
                            }
                        }
                    }
                }
                let _ = fs::remove_dir_all(&scratch);
            });
        }
    });

    let violations = violations.into_inner().unwrap();
    let total: usize = violations.values().sum();
    println!("files:      {}", files.len());
    println!("configs:    {} per file", settings.len());
    println!("unchanged:  {}", unchanged.into_inner());
    println!("rewritten:  {} (every property held)", rewritten.into_inner());
    println!("violations: {total}");
    for (label, count) in &violations {
        println!("  {label}: {count}");
    }
    assert_eq!(total, 0, "reflow violations written to {}", out.display());
}
