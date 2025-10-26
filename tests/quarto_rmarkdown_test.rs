use std::fs;
use tempfile::TempDir;

/// Test that .qmd and .rmd files are discovered
#[test]
fn test_qmd_rmd_file_discovery() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    // Create .qmd file
    let qmd_file = temp_path.join("test.qmd");
    fs::write(&qmd_file, "# Test Quarto\n").unwrap();

    // Create .rmd file
    let rmd_file = temp_path.join("test.rmd");
    fs::write(&rmd_file, "# Test RMarkdown\n").unwrap();

    let args = rumdl::CheckArgs {
        paths: vec![temp_path.to_string_lossy().to_string()],
        verbose: false,
        quiet: false,
        fix: false,
        diff: false,
        include: None,
        exclude: None,
        no_exclude: false,
        enable: None,
        disable: None,
        extend_enable: None,
        extend_disable: None,
        respect_gitignore: false,
        output_format: None,
        statistics: false,
        config: None,
    };

    let config = rumdl_lib::config::Config::default();
    let files = rumdl::file_processor::find_markdown_files(&args.paths, &args, &config)
        .expect("Failed to find markdown files");

    assert!(
        files.iter().any(|f| f.ends_with("test.qmd")),
        "QMD file should be discovered. Found files: {:?}",
        files
    );
    assert!(
        files.iter().any(|f| f.ends_with("test.rmd")),
        "RMD file should be discovered. Found files: {:?}",
        files
    );
}

/// Test that Quarto-specific syntax doesn't cause false positives
#[test]
fn test_quarto_syntax_handling() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let qmd_file = temp_path.join("test.qmd");
    fs::write(
        &qmd_file,
        r#"---
title: "Test Document"
format: html
---

# Introduction

Cross-reference: See @fig-plot for details.

```{r}
#| label: fig-plot
#| echo: false
plot(1:10)
```

:::{.callout-note}
This is a callout block.
:::

Inline code: `r 1 + 1`
"#,
    )
    .unwrap();

    let args = rumdl::CheckArgs {
        paths: vec![qmd_file.to_string_lossy().to_string()],
        verbose: false,
        quiet: false,
        fix: false,
        diff: false,
        include: None,
        exclude: None,
        no_exclude: false,
        enable: None,
        disable: None,
        extend_enable: None,
        extend_disable: None,
        respect_gitignore: false,
        output_format: None,
        statistics: false,
        config: None,
    };

    let config = rumdl_lib::config::Config::default();
    let rules = rumdl_lib::rules::all_rules(&config);

    let warnings = rumdl::file_processor::process_file_collect_warnings(
        &qmd_file.to_string_lossy(),
        &rules,
        false,
        false,
        true, // quiet mode
        &config,
        None,
    );

    // Should not trigger MD034 (bare URLs) for cross-references like @fig-plot
    assert!(
        !warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD034")),
        "Cross-references should not trigger MD034. Warnings: {:?}",
        warnings
    );

    // Should not trigger MD033 (inline HTML) for callout blocks
    assert!(
        !warnings.iter().any(|w| w.rule_name.as_deref() == Some("MD033")),
        "Callout blocks should not trigger MD033. Warnings: {:?}",
        warnings
    );
}

/// Test that RMarkdown-specific syntax doesn't cause false positives
#[test]
fn test_rmarkdown_syntax_handling() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let rmd_file = temp_path.join("test.rmd");
    fs::write(
        &rmd_file,
        r#"---
title: "RMarkdown Test"
output: html_document
---

# Analysis

```{r setup, include=FALSE}
knitr::opts_chunk$set(echo = TRUE)
```

```{r plot, echo=FALSE, fig.cap="My Plot"}
plot(cars)
```

Inline R code: `r mean(1:10)`
"#,
    )
    .unwrap();

    let args = rumdl::CheckArgs {
        paths: vec![rmd_file.to_string_lossy().to_string()],
        verbose: false,
        quiet: false,
        fix: false,
        diff: false,
        include: None,
        exclude: None,
        no_exclude: false,
        enable: None,
        disable: None,
        extend_enable: None,
        extend_disable: None,
        respect_gitignore: false,
        output_format: None,
        statistics: false,
        config: None,
    };

    let config = rumdl_lib::config::Config::default();
    let rules = rumdl_lib::rules::all_rules(&config);

    let warnings = rumdl::file_processor::process_file_collect_warnings(
        &rmd_file.to_string_lossy(),
        &rules,
        false,
        false,
        true, // quiet mode
        &config,
        None,
    );

    // Code chunks with options in braces should be treated as valid code blocks
    assert!(
        !warnings
            .iter()
            .any(|w| w.rule_name.as_deref() == Some("MD040")
                && w.message.contains("should have a language")),
        "Code chunks with {r} should be recognized as having a language. Warnings: {:?}",
        warnings
    );
}

/// Test explicit path handling for .qmd and .rmd files
#[test]
fn test_qmd_rmd_explicit_paths() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    for ext in &["qmd", "rmd"] {
        let file_path = temp_path.join(format!("test.{}", ext));
        fs::write(&file_path, format!("# Test {}\n", ext.to_uppercase())).unwrap();

        let args = rumdl::CheckArgs {
            paths: vec![file_path.to_string_lossy().to_string()],
            verbose: false,
            quiet: false,
            fix: false,
            diff: false,
            include: None,
            exclude: None,
            no_exclude: false,
            enable: None,
            disable: None,
            extend_enable: None,
            extend_disable: None,
            respect_gitignore: false,
            output_format: None,
            statistics: false,
            config: None,
        };

        let config = rumdl_lib::config::Config::default();
        let files = rumdl::file_processor::find_markdown_files(&args.paths, &args, &config)
            .expect("Failed to find markdown files");

        assert_eq!(
            files.len(),
            1,
            "Should find exactly one {} file",
            ext.to_uppercase()
        );
        assert!(
            files[0].ends_with(&format!("test.{}", ext)),
            "Should find the {} file. Found: {:?}",
            ext.to_uppercase(),
            files[0]
        );
    }
}
