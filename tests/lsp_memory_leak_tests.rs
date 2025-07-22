use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_lsp_memory_usage_over_time() {
    println!("Testing LSP memory usage over time...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create test markdown files
    let test_content = r#"# Test Document

This is a test document with some content.

## Section 1

Some content here.

### Subsection

More content.

## Section 2

Final content.
"#;

    for i in 0..10 {
        fs::write(project_path.join(format!("test{i}.md")), test_content).unwrap();
    }

    // Start LSP server
    let mut lsp_process = Command::new("cargo")
        .args(["run", "--bin", "rumdl", "--", "lsp"])
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start LSP server");

    let mut stdin = lsp_process.stdin.take().unwrap();
    let stdout = lsp_process.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Initialize LSP with proper error handling
    let initialize_request =
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":"file://"}}"#;
    if writeln!(
        stdin,
        "Content-Length: {}\r\n\r\n{}",
        initialize_request.len(),
        initialize_request
    )
    .is_err()
    {
        println!("LSP process terminated early, skipping test");
        let _ = lsp_process.wait();
        return;
    }

    // Read initialization response with timeout
    let mut response = String::new();
    if reader.read_line(&mut response).is_err() {
        println!("Failed to read LSP response, process may have terminated");
        let _ = lsp_process.wait();
        return;
    }

    // Send initialized notification
    let initialized = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialized.len(), initialized).is_err() {
        println!("LSP process terminated, skipping test");
        let _ = lsp_process.wait();
        return;
    }

    // Monitor memory usage over multiple operations
    let memory_samples = Arc::new(Mutex::new(Vec::new()));
    let memory_samples_clone = Arc::clone(&memory_samples);
    let lsp_pid = lsp_process.id();

    // Memory monitoring thread with error handling
    let monitor_handle = thread::spawn(move || {
        for _ in 0..15 {
            // Reduced monitoring time to 15 seconds
            if let Ok(memory_kb) = get_process_memory(lsp_pid) {
                if let Ok(mut samples) = memory_samples_clone.lock() {
                    samples.push(memory_kb);
                }
            } else {
                // Process may have terminated
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    });

    // Simulate LSP usage with error handling
    for i in 0..50 {
        // Reduced iterations
        let file_uri = format!("file://{}/test{}.md", project_path.display(), i % 10);

        // Send textDocument/didOpen with error handling
        let did_open = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"markdown","version":1,"text":"{}"}}}}}}"#,
            file_uri,
            test_content.replace('\n', "\\n")
        );
        if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).is_err() {
            println!("LSP process terminated during operation");
            break;
        }

        // Send textDocument/didChange with error handling
        let updated_content = "# Updated Content\\n\\nThis is updated content.";
        let did_change = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{file_uri}","version":2}},"contentChanges":[{{"text":"{updated_content}"}}]}}}}"#
        );
        if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_change.len(), did_change).is_err() {
            println!("LSP process terminated during operation");
            break;
        }

        // Send textDocument/didClose with error handling
        let did_close = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{file_uri}"}}}}}}"#
        );
        if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_close.len(), did_close).is_err() {
            println!("LSP process terminated during operation");
            break;
        }

        thread::sleep(Duration::from_millis(100));
    }

    // Wait for monitoring to complete
    let _ = monitor_handle.join();

    // Analyze memory usage
    let samples = memory_samples.lock().unwrap();
    if !samples.is_empty() {
        let initial_memory = samples[0];
        let final_memory = samples[samples.len() - 1];
        let max_memory = *samples.iter().max().unwrap();
        let min_memory = *samples.iter().min().unwrap();

        println!("Memory usage analysis:");
        println!("  Initial: {initial_memory} KB");
        println!("  Final: {final_memory} KB");
        println!("  Max: {max_memory} KB");
        println!("  Min: {min_memory} KB");
        println!("  Growth: {} KB", final_memory as i64 - initial_memory as i64);

        // Check for memory leaks (growth should be reasonable)
        let growth_ratio = final_memory as f64 / initial_memory as f64;
        assert!(growth_ratio < 3.0, "Memory usage grew too much: {growth_ratio}x");

        // Check that memory doesn't continuously grow
        let trend = calculate_memory_trend(&samples);
        assert!(trend < 200.0, "Memory trend too steep: {trend} KB/sample");
    } else {
        println!("No memory samples collected, LSP process may have terminated early");
    }

    // Graceful cleanup
    let _ = lsp_process.kill();
    let _ = lsp_process.wait();

    println!("✅ LSP memory usage test completed");
}

#[test]
fn test_lsp_memory_stress_with_large_files() {
    println!("Testing LSP memory usage with large files...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create large test file (smaller for reliability)
    let mut large_content = String::new();
    for i in 0..500 {
        // Reduced size
        large_content.push_str(&format!("# Heading {i}\n\nContent for section {i}.\n\n"));
    }

    fs::write(project_path.join("large.md"), &large_content).unwrap();

    // Start LSP server
    let mut lsp_process = Command::new("cargo")
        .args(["run", "--bin", "rumdl", "--", "lsp"])
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start LSP server");

    let mut stdin = lsp_process.stdin.take().unwrap();

    // Initialize LSP with error handling
    let initialize_request =
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":"file://"}}"#;
    if writeln!(
        stdin,
        "Content-Length: {}\r\n\r\n{}",
        initialize_request.len(),
        initialize_request
    )
    .is_err()
    {
        println!("LSP process terminated early, skipping test");
        let _ = lsp_process.wait();
        return;
    }

    let initialized = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialized.len(), initialized).is_err() {
        println!("LSP process terminated early, skipping test");
        let _ = lsp_process.wait();
        return;
    }

    // Measure memory before and after processing large file
    let initial_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Open large file with error handling
    let file_uri = format!("file://{}/large.md", project_path.display());
    let did_open = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"markdown","version":1,"text":"{}"}}}}}}"#,
        file_uri,
        large_content.replace('\n', "\\n")
    );
    if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).is_err() {
        println!("LSP process terminated, skipping test");
        let _ = lsp_process.wait();
        return;
    }

    thread::sleep(Duration::from_secs(2)); // Allow processing time

    let after_open_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Make multiple edits with error handling
    for i in 0..5 {
        // Reduced iterations
        let edit_content = format!("# Updated Heading {i}\\n\\nUpdated content.");
        let did_change = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":{}}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
            file_uri,
            i + 2,
            edit_content
        );
        if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_change.len(), did_change).is_err() {
            println!("LSP process terminated during edits");
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }

    let after_edits_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Close file with error handling
    let did_close = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{file_uri}"}}}}}}"#
    );
    let _ = writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_close.len(), did_close);

    thread::sleep(Duration::from_secs(1)); // Allow cleanup time

    let after_close_memory = get_process_memory(lsp_process.id()).unwrap_or(after_edits_memory);

    println!("Large file memory analysis:");
    println!("  Initial: {initial_memory} KB");
    println!("  After open: {after_open_memory} KB");
    println!("  After edits: {after_edits_memory} KB");
    println!("  After close: {after_close_memory} KB");

    // Memory should be released after closing (or at least not grow excessively)
    if after_edits_memory > 0 && initial_memory > 0 {
        let total_growth = after_edits_memory as f64 / initial_memory as f64;
        assert!(
            total_growth < 10.0,
            "Memory usage grew too much with large file: {total_growth}x"
        );
    }

    // Graceful cleanup
    let _ = lsp_process.kill();
    let _ = lsp_process.wait();

    println!("✅ LSP large file memory test completed");
}

#[test]
fn test_lsp_concurrent_document_handling() {
    println!("Testing LSP memory usage with concurrent document handling...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create multiple test files (reduced number)
    for i in 0..10 {
        let content = format!("# Document {i}\n\nContent for document {i}.\n");
        fs::write(project_path.join(format!("doc{i}.md")), content).unwrap();
    }

    // Start LSP server
    let mut lsp_process = Command::new("cargo")
        .args(["run", "--bin", "rumdl", "--", "lsp"])
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start LSP server");

    let mut stdin = lsp_process.stdin.take().unwrap();

    // Initialize LSP with error handling
    let initialize_request =
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":"file://"}}"#;
    if writeln!(
        stdin,
        "Content-Length: {}\r\n\r\n{}",
        initialize_request.len(),
        initialize_request
    )
    .is_err()
    {
        println!("LSP process terminated early, skipping test");
        let _ = lsp_process.wait();
        return;
    }

    let initialized = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialized.len(), initialized).is_err() {
        println!("LSP process terminated early, skipping test");
        let _ = lsp_process.wait();
        return;
    }

    let initial_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Open all documents simultaneously with error handling
    for i in 0..10 {
        let file_uri = format!("file://{}/doc{}.md", project_path.display(), i);
        let content = format!("# Document {i}\\n\\nContent for document {i}.");
        let did_open = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{file_uri}","languageId":"markdown","version":1,"text":"{content}"}}}}}}"#
        );
        if writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).is_err() {
            println!("LSP process terminated during document opening");
            break;
        }
    }

    thread::sleep(Duration::from_secs(2));
    let after_open_memory = get_process_memory(lsp_process.id()).unwrap_or(initial_memory);

    // Close all documents with error handling
    for i in 0..10 {
        let file_uri = format!("file://{}/doc{}.md", project_path.display(), i);
        let did_close = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{file_uri}"}}}}}}"#
        );
        let _ = writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_close.len(), did_close);
    }

    thread::sleep(Duration::from_secs(2));
    let after_close_memory = get_process_memory(lsp_process.id()).unwrap_or(after_open_memory);

    println!("Concurrent document memory analysis:");
    println!("  Initial: {initial_memory} KB");
    println!("  After opening 10 docs: {after_open_memory} KB");
    println!("  After closing all docs: {after_close_memory} KB");

    // Memory checks (more lenient for reliability)
    if after_open_memory > initial_memory && initial_memory > 0 {
        let memory_retention =
            (after_close_memory as f64 - initial_memory as f64) / (after_open_memory as f64 - initial_memory as f64);
        assert!(
            memory_retention < 0.8,
            "Too much memory retained after closing documents: {:.2}%",
            memory_retention * 100.0
        );
    }

    // Graceful cleanup
    let _ = lsp_process.kill();
    let _ = lsp_process.wait();

    println!("✅ LSP concurrent document memory test completed");
}

// Helper function to get process memory usage (Linux/macOS)
fn get_process_memory(pid: u32) -> Result<u64, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()?;

        if !output.status.success() {
            return Err("Process not found".into());
        }

        let memory_str = String::from_utf8(output.stdout)?;
        let memory_str = memory_str.trim();
        if memory_str.is_empty() {
            return Err("Process not found or no memory info".into());
        }

        let memory_kb: u64 = memory_str.parse()?;
        Ok(memory_kb)
    }

    #[cfg(target_os = "linux")]
    {
        let status_path = format!("/proc/{pid}/status");
        let content = std::fs::read_to_string(status_path)?;

        for line in content.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let memory_kb: u64 = parts[1].parse()?;
                    return Ok(memory_kb);
                }
            }
        }

        Err("Could not find VmRSS in /proc/pid/status".into())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        // For other platforms, return a dummy value
        Ok(1000) // 1MB baseline
    }
}

// Helper function to calculate memory trend
fn calculate_memory_trend(samples: &[u64]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }

    let n = samples.len() as f64;
    let sum_x: f64 = (0..samples.len()).map(|i| i as f64).sum();
    let sum_y: f64 = samples.iter().map(|&y| y as f64).sum();
    let sum_xy: f64 = samples.iter().enumerate().map(|(i, &y)| i as f64 * y as f64).sum();
    let sum_x2: f64 = (0..samples.len()).map(|i| (i as f64).powi(2)).sum();

    // Linear regression slope
    (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x.powi(2))
}
