use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::process::{Command, Stdio};
use tempfile::tempdir;
use std::fs;
use std::io::{BufRead, BufReader, Write};

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
        fs::write(project_path.join(format!("test{}.md", i)), test_content).unwrap();
    }

    // Start LSP server
    let mut lsp_process = Command::new("cargo")
        .args(&["run", "--bin", "rumdl", "--", "lsp"])
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start LSP server");

    let mut stdin = lsp_process.stdin.take().unwrap();
    let stdout = lsp_process.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Initialize LSP
    let initialize_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":"file://"}}"#;
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialize_request.len(), initialize_request).unwrap();

    // Read initialization response
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();

    // Send initialized notification
    let initialized = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialized.len(), initialized).unwrap();

        // Monitor memory usage over multiple operations
    let memory_samples = Arc::new(Mutex::new(Vec::new()));
    let memory_samples_clone = Arc::clone(&memory_samples);
    let lsp_pid = lsp_process.id();

    // Memory monitoring thread
    let monitor_handle = thread::spawn(move || {
        for _ in 0..30 { // Monitor for 30 seconds
            if let Ok(memory_kb) = get_process_memory(lsp_pid) {
                memory_samples_clone.lock().unwrap().push(memory_kb);
            }
            thread::sleep(Duration::from_secs(1));
        }
    });

    // Simulate heavy LSP usage
    for i in 0..100 {
        let file_uri = format!("file://{}/test{}.md", project_path.display(), i % 10);

        // Send textDocument/didOpen
        let did_open = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"markdown","version":1,"text":"{}"}}}}}}"#, file_uri, test_content.replace('\n', "\\n"));
        writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).unwrap();

        // Send textDocument/didChange
        let updated_content = "# Updated Content\\n\\nThis is updated content.";
        let did_change = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":2}},"contentChanges":[{{"text":"{}"}}]}}}}"#, file_uri, updated_content);
        writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_change.len(), did_change).unwrap();

        // Send textDocument/didClose
        let did_close = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{}"}}}}}}"#, file_uri);
        writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_close.len(), did_close).unwrap();

        thread::sleep(Duration::from_millis(100));
    }

    // Wait for monitoring to complete
    monitor_handle.join().unwrap();

    // Analyze memory usage
    let samples = memory_samples.lock().unwrap();
    if !samples.is_empty() {
        let initial_memory = samples[0];
        let final_memory = samples[samples.len() - 1];
        let max_memory = *samples.iter().max().unwrap();
        let min_memory = *samples.iter().min().unwrap();

        println!("Memory usage analysis:");
        println!("  Initial: {} KB", initial_memory);
        println!("  Final: {} KB", final_memory);
        println!("  Max: {} KB", max_memory);
        println!("  Min: {} KB", min_memory);
        println!("  Growth: {} KB", final_memory as i64 - initial_memory as i64);

        // Check for memory leaks (growth should be reasonable)
        let growth_ratio = final_memory as f64 / initial_memory as f64;
        assert!(growth_ratio < 2.0, "Memory usage grew too much: {}x", growth_ratio);

        // Check that memory doesn't continuously grow
        let trend = calculate_memory_trend(&samples);
        assert!(trend < 100.0, "Memory trend too steep: {} KB/sample", trend);
    }

    // Cleanup
    lsp_process.kill().unwrap();

    println!("✅ LSP memory usage test completed");
}

#[test]
fn test_lsp_memory_stress_with_large_files() {
    println!("Testing LSP memory usage with large files...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create large test file
    let mut large_content = String::new();
    for i in 0..1000 {
        large_content.push_str(&format!("# Heading {}\n\nContent for section {}.\n\n", i, i));
    }

    fs::write(project_path.join("large.md"), &large_content).unwrap();

    // Start LSP server
    let mut lsp_process = Command::new("cargo")
        .args(&["run", "--bin", "rumdl", "--", "lsp"])
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start LSP server");

    let mut stdin = lsp_process.stdin.take().unwrap();

    // Initialize LSP
    let initialize_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":"file://"}}"#;
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialize_request.len(), initialize_request).unwrap();

    let initialized = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialized.len(), initialized).unwrap();

    // Measure memory before and after processing large file
    let initial_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Open large file
    let file_uri = format!("file://{}/large.md", project_path.display());
    let did_open = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"markdown","version":1,"text":"{}"}}}}}}"#, file_uri, large_content.replace('\n', "\\n"));
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).unwrap();

    thread::sleep(Duration::from_secs(2)); // Allow processing time

    let after_open_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Make multiple edits
    for i in 0..10 {
        let edit_content = format!("# Updated Heading {}\\n\\nUpdated content.", i);
        let did_change = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":{}}},"contentChanges":[{{"text":"{}"}}]}}}}"#, file_uri, i + 2, edit_content);
        writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_change.len(), did_change).unwrap();
        thread::sleep(Duration::from_millis(200));
    }

    let after_edits_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Close file
    let did_close = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{}"}}}}}}"#, file_uri);
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_close.len(), did_close).unwrap();

    thread::sleep(Duration::from_secs(1)); // Allow cleanup time

    let after_close_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    println!("Large file memory analysis:");
    println!("  Initial: {} KB", initial_memory);
    println!("  After open: {} KB", after_open_memory);
    println!("  After edits: {} KB", after_edits_memory);
    println!("  After close: {} KB", after_close_memory);

    // Memory should be released after closing
    let memory_released = after_edits_memory > after_close_memory;
    if !memory_released {
        println!("Warning: Memory may not have been fully released after closing file");
    }

    // Memory growth should be reasonable
    let total_growth = after_edits_memory as f64 / initial_memory as f64;
    assert!(total_growth < 5.0, "Memory usage grew too much with large file: {}x", total_growth);

    // Cleanup
    lsp_process.kill().unwrap();

    println!("✅ LSP large file memory test completed");
}

#[test]
fn test_lsp_concurrent_document_handling() {
    println!("Testing LSP memory usage with concurrent document handling...");

    let temp_dir = tempdir().unwrap();
    let project_path = temp_dir.path();

    // Create multiple test files
    for i in 0..20 {
        let content = format!("# Document {}\n\nContent for document {}.\n", i, i);
        fs::write(project_path.join(format!("doc{}.md", i)), content).unwrap();
    }

    // Start LSP server
    let mut lsp_process = Command::new("cargo")
        .args(&["run", "--bin", "rumdl", "--", "lsp"])
        .current_dir(project_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start LSP server");

    let mut stdin = lsp_process.stdin.take().unwrap();

    // Initialize LSP
    let initialize_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":"file://"}}"#;
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialize_request.len(), initialize_request).unwrap();

    let initialized = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    writeln!(stdin, "Content-Length: {}\r\n\r\n{}", initialized.len(), initialized).unwrap();

    let initial_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Open all documents simultaneously
    for i in 0..20 {
        let file_uri = format!("file://{}/doc{}.md", project_path.display(), i);
        let content = format!("# Document {}\\n\\nContent for document {}.", i, i);
        let did_open = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"markdown","version":1,"text":"{}"}}}}}}"#, file_uri, content);
        writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).unwrap();
    }

    thread::sleep(Duration::from_secs(2));
    let after_open_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    // Close all documents
    for i in 0..20 {
        let file_uri = format!("file://{}/doc{}.md", project_path.display(), i);
        let did_close = format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{}"}}}}}}"#, file_uri);
        writeln!(stdin, "Content-Length: {}\r\n\r\n{}", did_close.len(), did_close).unwrap();
    }

    thread::sleep(Duration::from_secs(2));
    let after_close_memory = get_process_memory(lsp_process.id()).unwrap_or(0);

    println!("Concurrent document memory analysis:");
    println!("  Initial: {} KB", initial_memory);
    println!("  After opening 20 docs: {} KB", after_open_memory);
    println!("  After closing all docs: {} KB", after_close_memory);

    // Memory should be mostly released
    let memory_retention = (after_close_memory as f64 - initial_memory as f64) / (after_open_memory as f64 - initial_memory as f64);
    assert!(memory_retention < 0.5, "Too much memory retained after closing documents: {:.2}%", memory_retention * 100.0);

    // Cleanup
    lsp_process.kill().unwrap();

    println!("✅ LSP concurrent document memory test completed");
}

// Helper function to get process memory usage (Linux/macOS)
fn get_process_memory(pid: u32) -> Result<u64, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps")
            .args(&["-o", "rss=", "-p", &pid.to_string()])
            .output()?;

        let memory_str = String::from_utf8(output.stdout)?;
        let memory_kb: u64 = memory_str.trim().parse()?;
        Ok(memory_kb)
    }

    #[cfg(target_os = "linux")]
    {
        let status_path = format!("/proc/{}/status", pid);
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