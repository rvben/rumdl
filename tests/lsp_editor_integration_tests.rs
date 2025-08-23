use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;
use std::time::Instant;

/// Test scenarios that simulate how LSP behaves in real editor environments
/// These tests validate editor integration patterns and user workflows
#[test]
fn test_incremental_editing_simulation() {
    // Simulate a user editing a document incrementally
    let _initial_content = r#"# My Document
This is the first paragraph.

## Section 1
Some content here."#;

    let editing_steps = [
        // Step 1: User adds trailing spaces (common typing scenario)
        r#"# My Document
This is the first paragraph.

## Section 1
Some content here."#,
        // Step 2: User adds a new heading without proper spacing
        r#"# My Document
This is the first paragraph.

## Section 1
Some content here.
### Subsection
More content."#,
        // Step 3: User fixes some issues manually
        r#"# My Document
This is the first paragraph.

## Section 1
Some content here.

### Subsection
More content."#,
    ];

    let rule = MD009TrailingSpaces::default();

    for (step, content) in editing_steps.iter().enumerate() {
        let ctx = LintContext::new(content);

        let start_time = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let check_duration = start_time.elapsed();

        // LSP needs to be responsive - under 100ms for typical document sizes
        assert!(
            check_duration.as_millis() < 100,
            "Step {}: Rule check took too long: {}ms (should be under 100ms for editor responsiveness)",
            step,
            check_duration.as_millis()
        );

        println!(
            "Step {}: {} warnings in {}ms",
            step,
            warnings.len(),
            check_duration.as_millis()
        );

        // Test LSP fix performance
        if !warnings.is_empty() {
            let fix_start = Instant::now();
            let _fixed = apply_warning_fixes(content, &warnings).expect("LSP fix should succeed");
            let fix_duration = fix_start.elapsed();

            assert!(
                fix_duration.as_millis() < 50,
                "Step {}: LSP fix took too long: {}ms (should be under 50ms for editor responsiveness)",
                step,
                fix_duration.as_millis()
            );
        }
    }
}

#[test]
fn test_editor_save_workflow() {
    // Simulate editor save workflow: check -> fix -> check again
    let content_with_issues = r#"#Heading Without Space
Content with trailing spaces

##Another Heading
- List item
-Missing space in list

```
code block without language
```

Final paragraph."#;

    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(MD018NoMissingSpaceAtx),
        Box::new(MD009TrailingSpaces::default()),
        Box::new(MD004UnorderedListStyle::new(
            rumdl_lib::rules::md004_unordered_list_style::UnorderedListStyle::Consistent,
        )),
        Box::new(MD040FencedCodeLanguage),
    ];

    // Phase 1: Initial check (editor save trigger)
    let ctx = LintContext::new(content_with_issues);
    let mut all_warnings = Vec::new();

    let check_start = Instant::now();
    for rule in &rules {
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        all_warnings.extend(warnings);
    }
    let total_check_time = check_start.elapsed();

    // Should be very fast for editor integration
    assert!(
        total_check_time.as_millis() < 200,
        "Initial check took too long: {}ms (should be under 200ms for save workflow)",
        total_check_time.as_millis()
    );

    assert!(
        !all_warnings.is_empty(),
        "Should find some issues in problematic content"
    );
    println!(
        "Found {} warnings in {}ms",
        all_warnings.len(),
        total_check_time.as_millis()
    );

    // Phase 2: Apply fixes (auto-fix on save)
    let fix_start = Instant::now();
    let mut fixed_content = content_with_issues.to_string();

    for rule in &rules {
        let ctx = LintContext::new(&fixed_content);
        let warnings = rule.check(&ctx).expect("Rule check should succeed");

        if !warnings.is_empty() {
            // Test that CLI and LSP produce the same result
            let cli_fixed = rule.fix(&ctx).expect("CLI fix should succeed");
            let lsp_fixed = apply_warning_fixes(&fixed_content, &warnings).expect("LSP fix should succeed");

            assert_eq!(
                cli_fixed,
                lsp_fixed,
                "Rule {} produced different results in save workflow",
                rule.name()
            );

            fixed_content = lsp_fixed;
        }
    }
    let total_fix_time = fix_start.elapsed();

    assert!(
        total_fix_time.as_millis() < 300,
        "Fix process took too long: {}ms (should be under 300ms for save workflow)",
        total_fix_time.as_millis()
    );

    // Phase 3: Verify fixes (post-save check)
    let verify_ctx = LintContext::new(&fixed_content);
    let mut remaining_warnings = Vec::new();

    for rule in &rules {
        let warnings = rule.check(&verify_ctx).expect("Rule check should succeed");
        remaining_warnings.extend(warnings);
    }

    // Should have significantly fewer warnings after fixes
    assert!(
        remaining_warnings.len() < all_warnings.len(),
        "Fixes should reduce warning count: {} -> {}",
        all_warnings.len(),
        remaining_warnings.len()
    );

    println!("After fixes: {} warnings remaining", remaining_warnings.len());
}

#[test]
fn test_partial_document_editing() {
    // Simulate editing small parts of a large document (common in editors)
    let large_document = create_large_document_with_issues();

    // Simulate editing just one section of the document
    let edit_scenarios = vec![
        // Scenario 1: Edit in the middle of document
        ("Middle edit", 1000, 1100),
        // Scenario 2: Edit at the beginning
        ("Beginning edit", 0, 200),
        // Scenario 3: Edit at the end
        ("End edit", large_document.len() - 200, large_document.len()),
    ];

    let rule = MD022BlanksAroundHeadings::default();

    for (scenario_name, start_pos, end_pos) in edit_scenarios {
        // Extract the edited section
        let section = &large_document[start_pos..end_pos];
        let ctx = LintContext::new(section);

        let start_time = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let check_duration = start_time.elapsed();

        // Even partial document checking should be very fast
        assert!(
            check_duration.as_millis() < 50,
            "{}: Partial check took too long: {}ms (should be under 50ms)",
            scenario_name,
            check_duration.as_millis()
        );

        println!(
            "{}: {} warnings in {}ms",
            scenario_name,
            warnings.len(),
            check_duration.as_millis()
        );

        // Test that partial fixing works correctly
        if !warnings.is_empty() {
            let fix_start = Instant::now();
            let _fixed = apply_warning_fixes(section, &warnings).expect("Partial fix should succeed");
            let fix_duration = fix_start.elapsed();

            assert!(
                fix_duration.as_millis() < 30,
                "{}: Partial fix took too long: {}ms (should be under 30ms)",
                scenario_name,
                fix_duration.as_millis()
            );
        }
    }
}

#[test]
fn test_concurrent_editing_simulation() {
    // Simulate multiple rapid edits (like fast typing or paste operations)
    let _base_content = r#"# Document Title

## Section One
Content goes here.

## Section Two
More content."#;

    let rapid_edits = [
        "# Document Title\n\n## Section One\nContent goes here.   \n\n## Section Two\nMore content.",
        "# Document Title\n\n## Section One\nContent goes here.\n\n## Section Two\nMore content.   ",
        "# Document Title\n\n## Section One   \nContent goes here.\n\n## Section Two\nMore content.",
        "# Document Title   \n\n## Section One\nContent goes here.\n\n## Section Two\nMore content.",
    ];

    let rule = MD009TrailingSpaces::default();

    // Simulate rapid successive edits (like when user is typing fast)
    let overall_start = Instant::now();
    for (i, content) in rapid_edits.iter().enumerate() {
        let ctx = LintContext::new(content);

        let edit_start = Instant::now();
        let warnings = rule.check(&ctx).expect("Rule check should succeed");
        let edit_duration = edit_start.elapsed();

        // Each edit check should be very fast to not block the editor
        assert!(
            edit_duration.as_millis() < 20,
            "Edit {}: Check took too long: {}ms (should be under 20ms for rapid editing)",
            i,
            edit_duration.as_millis()
        );

        // Apply fixes if needed
        if !warnings.is_empty() {
            let fix_start = Instant::now();
            let _fixed = apply_warning_fixes(content, &warnings).expect("Fix should succeed");
            let fix_duration = fix_start.elapsed();

            assert!(
                fix_duration.as_millis() < 15,
                "Edit {}: Fix took too long: {}ms (should be under 15ms for rapid editing)",
                i,
                fix_duration.as_millis()
            );
        }
    }
    let total_time = overall_start.elapsed();

    // All rapid edits together should complete very quickly
    assert!(
        total_time.as_millis() < 100,
        "Total rapid editing sequence took too long: {}ms (should be under 100ms)",
        total_time.as_millis()
    );

    println!(
        "Completed {} rapid edits in {}ms",
        rapid_edits.len(),
        total_time.as_millis()
    );
}

#[test]
fn test_undo_redo_consistency() {
    // Test that undo/redo operations maintain consistency
    let original = r#"# Title
Content here.
##Bad Heading
More content."#;

    let after_edit = format!(
        "# Title{}
Content here.
##Bad Heading
More content.",
        "   "
    ); // Add trailing spaces programmatically

    let rule = MD009TrailingSpaces::default();

    // Original state
    let ctx1 = LintContext::new(original);
    let warnings1 = rule.check(&ctx1).expect("Rule check should succeed");

    // After edit (simulate typing spaces)
    let ctx2 = LintContext::new(&after_edit);
    let warnings2 = rule.check(&ctx2).expect("Rule check should succeed");

    // After undo (back to original)
    let ctx3 = LintContext::new(original);
    let warnings3 = rule.check(&ctx3).expect("Rule check should succeed");

    // Undo should restore original state exactly
    assert_eq!(
        warnings1.len(),
        warnings3.len(),
        "Undo should restore original warning count: {} != {}",
        warnings1.len(),
        warnings3.len()
    );

    // Edit should have different warning count
    assert_ne!(warnings1.len(), warnings2.len(), "Edit should change warning count");

    // Test that fixes are also consistent across undo/redo
    if !warnings1.is_empty() {
        let fix1 = apply_warning_fixes(original, &warnings1).expect("Fix should succeed");
        let fix3 = apply_warning_fixes(original, &warnings3).expect("Fix should succeed");

        assert_eq!(fix1, fix3, "Fixes should be identical after undo");
    }

    if !warnings2.is_empty() {
        let _fix2 = apply_warning_fixes(&after_edit, &warnings2).expect("Fix should succeed");
    }

    println!(
        "Undo/redo consistency verified: {} -> {} -> {} warnings",
        warnings1.len(),
        warnings2.len(),
        warnings3.len()
    );
}

fn create_large_document_with_issues() -> String {
    let mut content = String::with_capacity(10000);

    for i in 1..=100 {
        if i % 10 == 1 {
            // Some headings with issues
            content.push_str(&format!("#Section {}   \n", i / 10 + 1));
        } else if i % 5 == 0 {
            // Some lines with trailing spaces
            content.push_str(&format!("Line {i} with trailing spaces   \n"));
        } else {
            content.push_str(&format!("Regular line {i} content.\n"));
        }
    }

    content
}
