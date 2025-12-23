use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::*;

/// Generate test content with various markdown issues that need fixing
fn generate_problematic_content(size: usize) -> String {
    let mut content = String::with_capacity(size * 100);

    for i in 0..size {
        // MD001 - Heading increment issues
        if i % 10 == 0 {
            content.push_str(&format!("### Heading {i} (should be H1)\n\n"));
        }

        // MD002 - First heading should be H1
        if i == 1 {
            content.push_str("## First heading (should be H1)\n\n");
        }

        // MD003 - Heading style inconsistency
        if i % 15 == 0 {
            content.push_str(&format!("Heading {i}\n"));
            content.push_str("=============\n\n");
        }

        // MD004 - Unordered list style inconsistency
        if i % 8 == 0 {
            content.push_str(&format!("* Item {i}\n"));
            content.push_str(&format!("- Item {}\n", i + 1));
            content.push_str(&format!("+ Item {}\n\n", i + 2));
        }

        // MD005 - List indentation issues
        if i % 12 == 0 {
            content.push_str(&format!("* Item {i}\n"));
            content.push_str(&format!("   * Badly indented item {i}\n\n"));
        }

        // MD006 - Start bullets at beginning
        if i % 20 == 0 {
            content.push_str(&format!("  * Indented bullet {i}\n\n"));
        }

        // MD007 - Unordered list indentation
        if i % 18 == 0 {
            content.push_str(&format!("* Item {i}\n"));
            content.push_str(&format!("   * Wrong indent {i}\n\n"));
        }

        // MD009 - Trailing spaces
        content.push_str(&format!("Line {i} with trailing spaces   \n"));

        // MD010 - Hard tabs
        if i % 25 == 0 {
            content.push_str(&format!("Line {i}\twith\ttabs\n"));
        }

        // MD012 - Multiple blank lines
        if i % 30 == 0 {
            content.push_str("\n\n\n");
        }

        // MD013 - Line length (create long lines)
        if i % 7 == 0 {
            content.push_str(&format!("This is a very long line {i} that exceeds the default line length limit of 80 characters and should be flagged by MD013 rule for being too long.\n"));
        }

        // MD018 - No space after hash on atx heading
        if i % 22 == 0 {
            content.push_str(&format!("#Heading without space {i}\n\n"));
        }

        // MD019 - Multiple spaces after hash on atx heading
        if i % 24 == 0 {
            content.push_str(&format!("##  Heading with extra spaces {i}\n\n"));
        }

        // MD020 - No space inside hashes on closed atx heading
        if i % 26 == 0 {
            content.push_str(&format!("#Closed heading{i}#\n\n"));
        }

        // MD021 - Multiple spaces inside hashes on closed atx heading
        if i % 28 == 0 {
            content.push_str(&format!("# Closed heading {i}  #\n\n"));
        }

        // MD022 - Headings should be surrounded by blank lines
        if i % 35 == 0 {
            content.push_str(&format!("Text before heading\n# Heading {i}\nText after heading\n\n"));
        }

        // MD023 - Headings must start at beginning of line
        if i % 40 == 0 {
            content.push_str(&format!("  # Indented heading {i}\n\n"));
        }

        // MD026 - Trailing punctuation in headings
        if i % 16 == 0 {
            content.push_str(&format!("# Heading with punctuation {i}!\n\n"));
        }

        // MD027 - Multiple spaces after blockquote symbol
        if i % 45 == 0 {
            content.push_str(&format!(">  Blockquote with extra spaces {i}\n\n"));
        }

        // MD030 - Spaces after list markers
        if i % 14 == 0 {
            content.push_str(&format!("-  Item with extra space {i}\n\n"));
        }

        // MD031 - Fenced code blocks should be surrounded by blank lines
        if i % 50 == 0 {
            content.push_str(&format!("Text before\n```\ncode {i}\n```\nText after\n\n"));
        }

        // MD032 - Lists should be surrounded by blank lines
        if i % 55 == 0 {
            content.push_str(&format!("Text before\n* List item {i}\nText after\n\n"));
        }

        // MD033 - Inline HTML
        if i % 17 == 0 {
            content.push_str(&format!("Text with <b>HTML tags</b> number {i}\n"));
        }

        // MD034 - Bare URL used
        if i % 19 == 0 {
            content.push_str(&format!("Visit http://example{i}.com for more info\n"));
        }

        // MD035 - Horizontal rule style
        if i % 60 == 0 {
            content.push_str("---\n\n***\n\n");
        }

        // MD037 - Spaces inside emphasis markers
        if i % 13 == 0 {
            content.push_str(&format!("Text with * bad emphasis * number {i}\n"));
        }

        // MD038 - Spaces inside code span elements
        if i % 21 == 0 {
            content.push_str(&format!("Text with ` bad code ` number {i}\n"));
        }

        // MD039 - Spaces inside link text
        if i % 32 == 0 {
            content.push_str(&format!("[ Link text ](http://example{i}.com)\n"));
        }

        // MD040 - Fenced code language
        if i % 65 == 0 {
            content.push_str("```\ncode without language\n```\n\n");
        }

        // MD042 - No empty links
        if i % 70 == 0 {
            content.push_str(&format!("[empty link {i}]()\n"));
        }

        // MD044 - Proper names
        if i % 11 == 0 {
            content.push_str(&format!("Text mentioning javascript and github number {i}\n"));
        }

        // MD045 - Images should have alternate text
        if i % 75 == 0 {
            content.push_str(&format!("![](image{i}.png)\n"));
        }

        // MD047 - Files should end with a single newline (we'll handle this separately)

        // MD049 - Emphasis style
        if i % 23 == 0 {
            content.push_str(&format!(
                "Text with _underscore emphasis_ and *asterisk emphasis* {i}\n"
            ));
        }

        // MD050 - Strong style
        if i % 27 == 0 {
            content.push_str(&format!(
                "Text with __underscore strong__ and **asterisk strong** {i}\n"
            ));
        }

        // MD053 - Link and image reference definitions should be needed
        if i % 80 == 0 {
            content.push_str(&format!("[unused{i}]: http://example{i}.com\n"));
        }

        // Regular content
        content.push_str(&format!("Regular paragraph {i} with some content.\n\n"));
    }

    content
}

/// Benchmark fix performance for rules that support fixing
fn bench_fix_performance(c: &mut Criterion) {
    let content = generate_problematic_content(100);
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD001 - Heading increment
    c.bench_function("MD001 fix", |b| {
        let rule = MD001HeadingIncrement;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD002 - First heading H1
    c.bench_function("MD002 fix", |b| {
        let rule = MD002FirstHeadingH1::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD003 - Heading style
    c.bench_function("MD003 fix", |b| {
        let rule = MD003HeadingStyle::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD004 - Unordered list style
    c.bench_function("MD004 fix", |b| {
        let rule = MD004UnorderedListStyle::new(rumdl_lib::rules::UnorderedListStyle::Consistent);
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD005 - List indentation
    c.bench_function("MD005 fix", |b| {
        let rule = MD005ListIndent::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD006 - Start bullets at beginning
    c.bench_function("MD006 fix", |b| {
        let rule = MD006StartBullets;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD007 - Unordered list indentation
    c.bench_function("MD007 fix", |b| {
        let rule = MD007ULIndent::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD009 - Trailing spaces
    c.bench_function("MD009 fix", |b| {
        let rule = MD009TrailingSpaces::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD010 - Hard tabs
    c.bench_function("MD010 fix", |b| {
        let rule = MD010NoHardTabs::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD012 - Multiple blank lines
    c.bench_function("MD012 fix", |b| {
        let rule = MD012NoMultipleBlanks::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD018 - No space after hash
    c.bench_function("MD018 fix", |b| {
        let rule = MD018NoMissingSpaceAtx;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD019 - Multiple spaces after hash
    c.bench_function("MD019 fix", |b| {
        let rule = MD019NoMultipleSpaceAtx;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD020 - No space inside hashes on closed atx
    c.bench_function("MD020 fix", |b| {
        let rule = MD020NoMissingSpaceClosedAtx;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD021 - Multiple spaces inside hashes on closed atx
    c.bench_function("MD021 fix", |b| {
        let rule = MD021NoMultipleSpaceClosedAtx;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD022 - Headings surrounded by blank lines
    c.bench_function("MD022 fix", |b| {
        let rule = MD022BlanksAroundHeadings::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD023 - Headings start at beginning
    c.bench_function("MD023 fix", |b| {
        let rule = MD023HeadingStartLeft;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD026 - Trailing punctuation
    c.bench_function("MD026 fix", |b| {
        let rule = MD026NoTrailingPunctuation::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD027 - Multiple spaces after blockquote
    c.bench_function("MD027 fix", |b| {
        let rule = MD027MultipleSpacesBlockquote;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD030 - Spaces after list markers
    c.bench_function("MD030 fix", |b| {
        let rule = MD030ListMarkerSpace::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD031 - Blanks around fences
    c.bench_function("MD031 fix", |b| {
        let rule = MD031BlanksAroundFences::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD032 - Blanks around lists
    c.bench_function("MD032 fix", |b| {
        let rule = MD032BlanksAroundLists;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD034 - Bare URLs
    c.bench_function("MD034 fix", |b| {
        let rule = MD034NoBareUrls;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD035 - Horizontal rule style
    c.bench_function("MD035 fix", |b| {
        let rule = MD035HRStyle::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD037 - Spaces inside emphasis
    c.bench_function("MD037 fix", |b| {
        let rule = MD037NoSpaceInEmphasis;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD038 - Spaces inside code spans
    c.bench_function("MD038 fix", |b| {
        let rule = MD038NoSpaceInCode::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD039 - Spaces inside links
    c.bench_function("MD039 fix", |b| {
        let rule = MD039NoSpaceInLinks;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD040 - Fenced code language
    c.bench_function("MD040 fix", |b| {
        let rule = MD040FencedCodeLanguage;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD044 - Proper names
    c.bench_function("MD044 fix", |b| {
        let proper_names = vec!["JavaScript".to_string(), "GitHub".to_string()];
        let rule = MD044ProperNames::new(proper_names, true);
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD047 - File end newline
    c.bench_function("MD047 fix", |b| {
        let rule = MD047SingleTrailingNewline;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD049 - Emphasis style
    c.bench_function("MD049 fix", |b| {
        let rule = MD049EmphasisStyle::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    // MD050 - Strong style (skip - requires enum parameter)
    // c.bench_function("MD050 fix", |b| {
    //     let rule = MD050StrongStyle::new(rumdl_lib::rules::StrongStyle::Consistent);
    //     b.iter(|| rule.fix(black_box(&ctx)))
    // });

    // MD053 - Link image reference definitions
    c.bench_function("MD053 fix", |b| {
        let rule = MD053LinkImageReferenceDefinitions::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });
}

/// Benchmark fix performance with large content
fn bench_fix_performance_large(c: &mut Criterion) {
    let content = generate_problematic_content(1000); // 10x larger
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Test the most commonly used fix rules with large content
    c.bench_function("MD009 fix large", |b| {
        let rule = MD009TrailingSpaces::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    c.bench_function("MD012 fix large", |b| {
        let rule = MD012NoMultipleBlanks::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    c.bench_function("MD018 fix large", |b| {
        let rule = MD018NoMissingSpaceAtx;
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    c.bench_function("MD026 fix large", |b| {
        let rule = MD026NoTrailingPunctuation::default();
        b.iter(|| rule.fix(black_box(&ctx)))
    });

    c.bench_function("MD037 fix large", |b| {
        let rule = MD037NoSpaceInEmphasis;
        b.iter(|| rule.fix(black_box(&ctx)))
    });
}

/// Benchmark string manipulation patterns commonly used in fixes
fn bench_string_operations(c: &mut Criterion) {
    let content = "Line with trailing spaces   \n".repeat(1000);

    // Test different approaches to removing trailing spaces
    c.bench_function("trim_end approach", |b| {
        b.iter(|| {
            content
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n")
        })
    });

    c.bench_function("regex replace approach", |b| {
        use regex::Regex;
        let re = Regex::new(r" +$").unwrap();
        b.iter(|| re.replace_all(black_box(&content), "").to_string())
    });

    c.bench_function("manual char iteration", |b| {
        b.iter(|| {
            let mut result = String::with_capacity(content.len());
            for line in content.lines() {
                let trimmed = line.trim_end();
                result.push_str(trimmed);
                result.push('\n');
            }
            result
        })
    });
}

/// Benchmark memory allocation patterns in fixes
fn bench_memory_patterns(c: &mut Criterion) {
    let content = generate_problematic_content(500);

    c.bench_function("string_with_capacity", |b| {
        b.iter(|| {
            let mut result = String::with_capacity(content.len());
            for line in content.lines() {
                result.push_str(line.trim_end());
                result.push('\n');
            }
            result
        })
    });

    c.bench_function("string_without_capacity", |b| {
        b.iter(|| {
            let mut result = String::new();
            for line in content.lines() {
                result.push_str(line.trim_end());
                result.push('\n');
            }
            result
        })
    });

    c.bench_function("collect_approach", |b| {
        b.iter(|| {
            content
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n")
        })
    });
}

criterion_group!(
    benches,
    bench_fix_performance,
    bench_fix_performance_large,
    bench_string_operations,
    bench_memory_patterns
);
criterion_main!(benches);
