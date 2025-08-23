use rumdl_lib::config::Config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use std::sync::{Arc, Mutex};
use std::thread;
// Duration removed - no longer needed after optimizations

#[test]
fn test_concurrent_rule_execution() {
    println!("Testing concurrent rule execution safety...");

    let test_content = r#"# Heading 1

Some content here.

## Heading 2
Missing blank line above.
Another line.

### Heading 3

More content.
"#;

    let ctx = LintContext::new(test_content);
    let shared_ctx = Arc::new(ctx);
    let results = Arc::new(Mutex::new(Vec::new()));

    // Test multiple rules concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let ctx_clone = Arc::clone(&shared_ctx);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            let rule = MD022BlanksAroundHeadings::new();
            let warnings = rule.check(&ctx_clone).unwrap();

            let mut results = results_clone.lock().unwrap();
            results.push((i, warnings.len()));
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    let results = results.lock().unwrap();
    println!("Concurrent results: {:?}", *results);

    // All threads should produce the same number of warnings
    let first_result = results[0].1;
    for (thread_id, warning_count) in results.iter() {
        assert_eq!(
            *warning_count, first_result,
            "Thread {thread_id} produced different warning count: {warning_count} vs {first_result}"
        );
    }

    assert!(results.len() == 10, "Expected 10 thread results");
}

#[test]
fn test_concurrent_different_rules() {
    println!("Testing concurrent execution of different rules...");

    let test_content = r#"# Heading!

Some content with trailing spaces

## Another Heading

- List item
-Another item missing space

```
code block without language
```

Text with *emphasis * and **strong **.
"#;

    let ctx = Arc::new(LintContext::new(test_content));
    let results = Arc::new(Mutex::new(Vec::new()));

    // Define different rules to test concurrently
    let rules: Vec<(String, Box<dyn Rule + Send>)> = vec![
        ("MD022".to_string(), Box::new(MD022BlanksAroundHeadings::new())),
        ("MD026".to_string(), Box::new(MD026NoTrailingPunctuation::default())),
        ("MD009".to_string(), Box::new(MD009TrailingSpaces::default())),
        ("MD018".to_string(), Box::new(MD018NoMissingSpaceAtx)),
        ("MD040".to_string(), Box::new(MD040FencedCodeLanguage)),
        ("MD037".to_string(), Box::new(MD037NoSpaceInEmphasis)),
        ("MD038".to_string(), Box::new(MD038NoSpaceInCode::default())),
    ];

    let mut handles = vec![];

    for (rule_name, rule) in rules {
        let ctx_clone = Arc::clone(&ctx);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            let warnings = rule.check(&ctx_clone).unwrap();

            let mut results = results_clone.lock().unwrap();
            results.push((rule_name, warnings.len()));
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    let results = results.lock().unwrap();
    println!("Rule execution results: {:?}", *results);

    // Verify all rules executed successfully
    assert!(results.len() == 7, "Expected 7 rule results");

    // Verify all rules executed (warning_count is usize, always non-negative)
    for (rule_name, _warning_count) in results.iter() {
        // Each rule executed successfully
        println!("Rule {rule_name} executed in parallel");
    }
}

#[test]
fn test_concurrent_configuration_access() {
    println!("Testing concurrent configuration access...");

    let config = Arc::new(Config::default());
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut handles = vec![];

    for i in 0..5 {
        let config_clone = Arc::clone(&config);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            // Access configuration from multiple threads
            let all_rules = rumdl_lib::rules::all_rules(&config_clone);

            let mut results = results_clone.lock().unwrap();
            results.push((i, all_rules.len()));
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    let results = results.lock().unwrap();
    println!("Configuration access results: {:?}", *results);

    // All threads should get the same number of rules
    let first_result = results[0].1;
    for (thread_id, rule_count) in results.iter() {
        assert_eq!(
            *rule_count, first_result,
            "Thread {thread_id} got different rule count: {rule_count} vs {first_result}"
        );
    }
}

#[test]
fn test_concurrent_fix_operations() {
    println!("Testing concurrent fix operations...");

    let test_content = r#"# Heading!
Some content.
## Another Heading
More content."#;

    let ctx = Arc::new(LintContext::new(test_content));
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut handles = vec![];

    for i in 0..5 {
        let ctx_clone = Arc::clone(&ctx);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            let rule = MD026NoTrailingPunctuation::default();
            let fixed_content = rule.fix(&ctx_clone);

            let mut results = results_clone.lock().unwrap();
            results.push((i, fixed_content.is_ok()));
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    let results = results.lock().unwrap();
    println!("Fix operation results: {:?}", *results);

    // All fix operations should succeed
    for (thread_id, success) in results.iter() {
        assert!(*success, "Thread {thread_id} fix operation failed");
    }
}

#[test]
fn test_high_concurrency_stress() {
    println!("Testing high concurrency stress scenarios...");

    let test_content = r#"# Document Title

This is a test document with various issues.

## Section 1
Missing blank line above.

### Subsection 1.1

Content here.

#### Subsection 1.1.1
Another missing blank line.

## Section 2!

Content with trailing punctuation.

```
code without language
```

Text with *bad emphasis * here.
"#;

    let ctx = Arc::new(LintContext::new(test_content));
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut handles = vec![];
    let thread_count = 50; // High concurrency

    for i in 0..thread_count {
        let ctx_clone = Arc::clone(&ctx);
        let results_clone = Arc::clone(&results);

        let handle = thread::spawn(move || {
            // Simulate different operations
            let operation = i % 4;

            match operation {
                0 => {
                    // Check operation
                    let rule = MD022BlanksAroundHeadings::new();
                    let warnings = rule.check(&ctx_clone).unwrap();
                    let mut results = results_clone.lock().unwrap();
                    results.push((i, "check".to_string(), warnings.len()));
                }
                1 => {
                    // Fix operation
                    let rule = MD026NoTrailingPunctuation::default();
                    let fixed = rule.fix(&ctx_clone);
                    let mut results = results_clone.lock().unwrap();
                    results.push((i, "fix".to_string(), if fixed.is_ok() { 1 } else { 0 }));
                }
                2 => {
                    // Multiple rule check
                    let rule1 = MD040FencedCodeLanguage;
                    let rule2 = MD037NoSpaceInEmphasis;
                    let warnings1 = rule1.check(&ctx_clone).unwrap();
                    let warnings2 = rule2.check(&ctx_clone).unwrap();
                    let mut results = results_clone.lock().unwrap();
                    results.push((i, "multi".to_string(), warnings1.len() + warnings2.len()));
                }
                _ => {
                    // Configuration access
                    let config = Config::default();
                    let all_rules = rumdl_lib::rules::all_rules(&config);
                    let mut results = results_clone.lock().unwrap();
                    results.push((i, "config".to_string(), all_rules.len()));
                }
            }

            // Add small delay to increase contention
            // Removed artificial delay for test performance
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    let results = results.lock().unwrap();
    println!("High concurrency results: {} operations completed", results.len());

    // Verify all threads completed successfully
    assert_eq!(results.len(), thread_count, "Expected {thread_count} thread results");

    // Group results by operation type
    let mut operation_counts = std::collections::HashMap::new();
    for (_, operation, _) in results.iter() {
        *operation_counts.entry(operation.clone()).or_insert(0) += 1;
    }

    println!("Operation distribution: {operation_counts:?}");

    // Each operation type should have been executed
    assert!(operation_counts.contains_key("check"));
    assert!(operation_counts.contains_key("fix"));
    assert!(operation_counts.contains_key("multi"));
    assert!(operation_counts.contains_key("config"));
}

#[test]
fn test_memory_safety_concurrent_access() {
    println!("Testing memory safety under concurrent access...");

    // Use a static large content instead of dynamically generated
    let large_content = r#"# Heading 1

Content for section 1.

# Heading 2

Content for section 2.

# Heading 3

Content for section 3.

# Heading 4

Content for section 4.

# Heading 5

Content for section 5.

# Heading 6

Content for section 6.

# Heading 7

Content for section 7.

# Heading 8

Content for section 8.

# Heading 9

Content for section 9.

# Heading 10

Content for section 10.
"#;

    let ctx = Arc::new(LintContext::new(large_content));
    let success_count = Arc::new(Mutex::new(0));

    let mut handles = vec![];

    for i in 0..20 {
        let ctx_clone = Arc::clone(&ctx);
        let success_count_clone = Arc::clone(&success_count);

        let handle = thread::spawn(move || {
            // Perform memory-intensive operations
            let rule = MD022BlanksAroundHeadings::new();

            for _ in 0..5 {
                match rule.check(&ctx_clone) {
                    Ok(warnings) => {
                        // Verify warnings are reasonable
                        assert!(warnings.len() < 100, "Too many warnings generated");

                        let mut count = success_count_clone.lock().unwrap();
                        *count += 1;
                    }
                    Err(_) => panic!("Rule check failed in thread {i}"),
                }

                // Small delay to allow other threads to access
                // Removed artificial delay for test performance
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    let final_count = *success_count.lock().unwrap();
    println!("Successful operations: {final_count}");

    // All operations should succeed
    assert_eq!(final_count, 20 * 5, "Expected 100 successful operations");
}
