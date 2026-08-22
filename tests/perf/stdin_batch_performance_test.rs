use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DOCUMENT_COUNT: usize = 770;
const SMOKE_BUDGET: Duration = Duration::from_secs(15);

#[test]
#[ignore = "performance smoke test; run explicitly"]
fn stdin_batch_processes_770_documents_within_smoke_budget() {
    let temp = tempfile::tempdir().unwrap();
    let mut input = Vec::with_capacity(DOCUMENT_COUNT * 768);

    for index in 0..DOCUMENT_COUNT {
        let path = format!("docs/{index:04}.md");
        let target = if index + 1 < DOCUMENT_COUNT {
            format!("{:04}.md", index + 1)
        } else {
            "missing.md".to_string()
        };
        let content = format!(
            "# Guide {index}\n\n\
             This guide explains a representative workflow and links to the next document.\n\n\
             ## Overview\n\n\
             Use the following checklist when working through the guide:\n\n\
             - Review the prerequisites.\n\
             - Apply the configuration.\n\
             - Verify the result.\n\n\
             ## Example\n\n\
             ```toml\n\
             enabled = true\n\
             document = {index}\n\
             ```\n\n\
             Continue with the [next guide]({target}).\n"
        );

        input.extend_from_slice(path.as_bytes());
        input.push(0);
        input.extend_from_slice(content.as_bytes());
        input.push(0);
    }

    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp.path())
        .args([
            "check",
            "--stdin-batch",
            "--stdin-batch-closed-world",
            "--no-cache",
            "--enable",
            "MD057",
            "--quiet",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute rumdl");
    child
        .stdin
        .as_mut()
        .expect("stdin must be piped")
        .write_all(&input)
        .expect("failed to write batch stdin");
    let output = child.wait_with_output().expect("failed to collect rumdl output");
    let elapsed = started.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    eprintln!(
        "processed {DOCUMENT_COUNT} documents ({} KiB) in {elapsed:?}",
        input.len() / 1024
    );

    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert_eq!(stdout.matches("MD057").count(), 1, "stdout:\n{stdout}");
    assert!(
        stdout.contains("docs/0769.md") && stdout.contains("missing.md"),
        "the final document must be processed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.is_empty(), "unexpected stderr:\n{stderr}");
    assert!(
        elapsed < SMOKE_BUDGET,
        "processing {DOCUMENT_COUNT} documents took {elapsed:?}, exceeding the {SMOKE_BUDGET:?} smoke budget"
    );
}
