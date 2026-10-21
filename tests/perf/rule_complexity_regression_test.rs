//! Comprehensive algorithmic complexity tests for all line-processing rules
//!
//! These tests verify that rules maintain O(n) complexity by measuring how
//! execution time scales with input size. They catch O(n²) regressions like
//! issue #148 where MD007's `has_mixed_list_nesting()` scanned the full
//! document per line.
//!
//! ## Key Design Principles
//!
//! 1. **Test growth ratios, not absolute times** - Avoids flaky CI failures
//! 2. **Warm-up run + median** - Statistical rigor, reduces noise
//! 3. **Large input sizes** - 500/1000/2000 entries minimizes jitter impact
//! 4. **Per-case thresholds** - Allow modest variance while catching repeated scans
//! 5. **Thread CPU time on Unix** - Excludes scheduling and other test workers; other platforms use elapsed time
//!
//! ## When These Tests Run
//!
//! These tests run in the `performance` profile with serial execution to
//! minimize system noise. They are excluded from `ci` and `dev` profiles.
//!
//! To run manually:
//! ```sh
//! cargo nextest run --profile performance -E 'test(linear_complexity)'
//! # Or use the make target:
//! make test-complexity
//! ```

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::{Fix, LintWarning, Rule, Severity};
use rumdl_lib::rules::CodeBlockStyle;
use rumdl_lib::rules::code_fence_utils::CodeFenceStyle;
use rumdl_lib::rules::md013_line_length::md013_config::{MD013Config, ReflowMode};
use rumdl_lib::rules::*;
use rumdl_lib::types::LineLength;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(not(unix))]
use std::time::Instant;

// =============================================================================
// Measurement Infrastructure
// =============================================================================

/// Thread CPU time excludes scheduling delays and other libtest workers.
/// Rule checks and context construction measured here run on this thread.
#[cfg(unix)]
fn thread_cpu_time() -> Duration {
    let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: clock_gettime writes a timespec through the supplied pointer.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, time.as_mut_ptr()) };
    assert_eq!(result, 0, "clock_gettime failed");
    // SAFETY: a successful clock_gettime call initialized the record above.
    let time = unsafe { time.assume_init() };
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

#[cfg(unix)]
pub(super) struct MeasurementClock(Duration);
#[cfg(not(unix))]
pub(super) struct MeasurementClock(Instant);

impl MeasurementClock {
    pub(super) fn now() -> Self {
        #[cfg(unix)]
        {
            Self(thread_cpu_time())
        }
        #[cfg(not(unix))]
        {
            Self(Instant::now())
        }
    }

    pub(super) fn elapsed(&self) -> Duration {
        #[cfg(unix)]
        {
            thread_cpu_time().saturating_sub(self.0)
        }
        #[cfg(not(unix))]
        {
            self.0.elapsed()
        }
    }
}

/// Concurrent libtest cases must not be charged to this test's CPU timer.
#[cfg(unix)]
#[test]
fn measurement_clock_excludes_other_test_threads() {
    let parent = MeasurementClock::now();
    let child = std::thread::spawn(|| {
        let clock = MeasurementClock::now();
        while clock.elapsed() < Duration::from_millis(30) {
            for value in 0..10_000_u64 {
                std::hint::black_box(value.wrapping_mul(value));
            }
        }
        clock.elapsed()
    });
    let child_cpu = child.join().unwrap();
    let parent_cpu = parent.elapsed();
    assert!(
        parent_cpu < child_cpu / 2,
        "other test threads contaminated the clock: parent={parent_cpu:?}, child={child_cpu:?}"
    );
}

/// Run with warm-up and multiple iterations for stable measurements
fn measure_rule_time<R: Rule + ?Sized>(rule: &R, content: &str, iterations: usize) -> Duration {
    measure_rule_time_for_flavor(rule, content, MarkdownFlavor::Standard, iterations)
}

fn measure_rule_time_for_flavor<R: Rule + ?Sized>(
    rule: &R,
    content: &str,
    flavor: MarkdownFlavor,
    iterations: usize,
) -> Duration {
    let ctx = LintContext::new(content, flavor, None);
    measure_rule_time_with_context(rule, &ctx, iterations)
}

fn measure_rule_time_with_context<R: Rule + ?Sized>(rule: &R, ctx: &LintContext, iterations: usize) -> Duration {
    // Warm-up run (discard - avoids cold cache artifacts)
    let _ = rule.check(ctx);

    // Batch fast checks so the timer's resolution cannot produce zero samples.
    // Normalize by the batch size to compare the cost of one check.
    let mut repetitions = 1_u32;
    while repetitions < (1 << 20) {
        let start = MeasurementClock::now();
        for _ in 0..repetitions {
            let _ = std::hint::black_box(rule.check(std::hint::black_box(ctx)));
        }
        if start.elapsed() >= Duration::from_millis(5) {
            break;
        }
        repetitions *= 2;
    }

    // Collect multiple measurements
    let mut times: Vec<Duration> = (0..iterations)
        .map(|_| {
            let start = MeasurementClock::now();
            for _ in 0..repetitions {
                let _ = std::hint::black_box(rule.check(std::hint::black_box(ctx)));
            }
            start.elapsed() / repetitions
        })
        .collect();

    // Use median (robust to outliers from system jitter)
    times.sort();
    times[iterations / 2]
}

/// Measure LintContext construction time
fn measure_context_time(content: &str, iterations: usize) -> Duration {
    // Warm-up
    let _ = LintContext::new(content, MarkdownFlavor::Standard, None);

    let mut times: Vec<Duration> = (0..iterations)
        .map(|_| {
            let start = MeasurementClock::now();
            let _ = LintContext::new(content, MarkdownFlavor::Standard, None);
            start.elapsed()
        })
        .collect();

    times.sort();
    times[iterations / 2]
}

#[test]
fn test_comment_code_range_queries_linear_complexity() {
    for (unit, block) in [
        ("`literal <!-- -->` <!-- real --> text\n\n", false),
        ("```\n<!-- literal -->\n```\n\n<!-- real -->\n\n", true),
    ] {
        let durations: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = unit.repeat(count);
                let code_end = if block {
                    unit.find("\n\n").unwrap() + 1
                } else {
                    unit[1..].find('`').unwrap() + 2
                };
                let ranges: Vec<_> = (0..count)
                    .map(|i| (i * unit.len(), i * unit.len() + code_end))
                    .collect();
                let (spans, blocks) = if block {
                    (&[][..], ranges.as_slice())
                } else {
                    (ranges.as_slice(), &[][..])
                };
                let scan = rumdl_lib::utils::skip_context::scan_html_comments(&content, spans, blocks, 0);
                assert_eq!(scan.ranges.len(), count);
                assert!(scan.unterminated.is_none());
                let mut samples: Vec<_> = (0..5)
                    .map(|_| {
                        let start = MeasurementClock::now();
                        for _ in 0..8 {
                            std::hint::black_box(rumdl_lib::utils::skip_context::scan_html_comments(
                                std::hint::black_box(&content),
                                spans,
                                blocks,
                                0,
                            ));
                        }
                        start.elapsed() / 8
                    })
                    .collect();
                samples.sort();
                samples[2]
            })
            .collect();
        assert_linear_complexity(&format!("HTML comment exclusions block={block}"), &durations, 3.0);
    }
}

#[test]
fn test_comment_line_classification_linear_complexity() {
    for (unit, block) in [
        ("`literal <!-- -->` <!-- real --> text\n\n", false),
        ("```\n<!-- literal -->\n```\n\n<!-- real -->\n\n", true),
    ] {
        let durations: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = unit.repeat(count);
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert_eq!(ctx.html_comment_ranges().len(), count);
                assert_eq!(
                    ctx.lines.iter().filter(|line| line.in_html_comment).count(),
                    if block { count } else { 0 }
                );
                measure_context_time(&content, 5)
            })
            .collect();
        assert_linear_complexity(&format!("HTML comment line flags block={block}"), &durations, 3.0);
    }
}

#[test]
fn test_mdx_jsx_positions_linear_complexity() {
    let durations: Vec<_> = [1024, 2048, 4096]
        .into_iter()
        .map(|count| {
            let content = "é日本🙂 <span>x</span> <Card /> ".repeat(count);
            let mut times: Vec<_> = (0..5)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
                    assert_eq!(ctx.html_tags().len(), count * 2);
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[times.len() / 2]
        })
        .collect();
    assert_linear_complexity("MDX JSX positions", &durations, 3.0);
}

#[test]
fn test_mdx_links_linear_complexity() {
    let durations: Vec<_> = [1024, 2048, 4096]
        .into_iter()
        .map(|count| {
            let content = "é日本🙂 [label](https://example.com) ![图](image.png) ".repeat(count);
            let mut times: Vec<_> = (0..5)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
                    assert_eq!(ctx.links().len(), count);
                    assert_eq!(ctx.images().len(), count);
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[times.len() / 2]
        })
        .collect();
    assert_linear_complexity("MDX links and images", &durations, 3.0);
}

/// Unmatched openers must stop at the first blocking delimiter.
#[test]
fn test_unmatched_emphasis_pairing_linear_complexity() {
    for unit in ["*word ", "__word "] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = unit.repeat(count);
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(MD037NoSpaceInEmphasis.check(&ctx).unwrap().is_empty());
                measure_rule_time(&MD037NoSpaceInEmphasis, &content, 5)
            })
            .collect();
        assert_linear_complexity(&format!("MD037 unmatched {unit:?}"), &times, 3.0);
    }
    let times: Vec<_> = [1024, 2048, 4096]
        .into_iter()
        .map(|count| measure_rule_time(&MD049EmphasisStyle::default(), &"*word ".repeat(count), 5))
        .collect();
    assert_linear_complexity("MD049 unmatched emphasis", &times, 3.0);
}

#[test]
fn test_md037_warning_filter_linear_complexity() {
    for (unit, expected) in [
        ("é日本🙂 * bad * _ other _ ", 2),
        ("é日本🙂 * bad * <span>text</span> ", 1),
        ("é日本🙂 `literal` $x$ ![image](image.png) * bad * ", 1),
        ("*foo * bar* ", 0),
    ] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("{}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert_eq!(MD037NoSpaceInEmphasis.check(&ctx).unwrap().len(), count * expected);
                measure_rule_time(&MD037NoSpaceInEmphasis, &content, 5)
            })
            .collect();
        assert_linear_complexity(&format!("MD037 filter {unit:?}"), &times, 3.0);
    }
}

#[test]
fn test_md049_warning_filter_linear_complexity() {
    for (unit, flavor, expected) in [
        ("é日本🙂 *good* _other_ ", MarkdownFlavor::Standard, 1),
        ("*foo * bar* ", MarkdownFlavor::Standard, 0),
        (
            "é日本🙂 *good* _other_ ^^_insert_^^ ++ctrl+_key_++ :icon_name: ",
            MarkdownFlavor::MkDocs,
            1,
        ),
        ("é日本🙂 *good* _other_ ==_mark_== ", MarkdownFlavor::Obsidian, 1),
    ] {
        let rule = MD049EmphasisStyle::default();
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("{}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, flavor, None);
                assert_eq!(rule.check(&ctx).unwrap().len(), count * expected);
                measure_rule_time_for_flavor(&rule, &content, flavor, 5)
            })
            .collect();
        assert_linear_complexity(&format!("MD049 filter {flavor:?} {unit:?}"), &times, 3.0);
    }
}

/// Workspace extraction must reuse character columns and searches for later
/// angle destinations, including a failed search over an angle-free suffix.
#[test]
fn test_workspace_link_extraction_linear_complexity() {
    for (name, unit, suffix) in [
        ("relative", "é日本🙂 [label](target.md) ", ""),
        ("absolute", "é日本🙂 [label](/target.md) ", ""),
        ("angle", "é日本🙂 [label](<target.md>) ", ""),
        ("tail-angle", "é日本🙂 [label](target.md) ", " [last](<last.md>)"),
        ("tail-marker", "é日本🙂 [label](target.md) ", " <no destination>"),
    ] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("{}{suffix}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                let links = rumdl_lib::workspace_index::extract_cross_file_links(&ctx);
                assert_eq!(
                    links.relative.len() + links.root_relative.len(),
                    count + usize::from(name == "tail-angle")
                );
                if name == "tail-angle" {
                    assert!(links.relative.iter().all(|link| link.target_path == "last.md"));
                }
                let mut times: Vec<_> = (0..5)
                    .map(|_| {
                        let clock = MeasurementClock::now();
                        for _ in 0..4 {
                            std::hint::black_box(rumdl_lib::workspace_index::extract_cross_file_links(
                                std::hint::black_box(&ctx),
                            ));
                        }
                        clock.elapsed() / 4
                    })
                    .collect();
                times.sort();
                times[2]
            })
            .collect();
        assert_linear_complexity(&format!("workspace {name}"), &times, 3.0);
    }
}

/// A source path is essential: MD057 otherwise skips filesystem diagnostics.
#[test]
fn test_md057_warning_columns_linear_complexity() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("sub");
    std::fs::create_dir(&base).unwrap();
    std::fs::write(base.join("target.md"), "target").unwrap();
    let source = base.join("page.md");
    std::fs::write(&source, "source").unwrap();
    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::Warn,
        compact_paths: true,
        self_referential_links: true,
        ..Default::default()
    };
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&base);
    for (name, unit, fixable) in [
        ("missing", "é日本🙂 [missing](missing.md) ", false),
        ("absolute", "é日本🙂 [absolute](/missing.md) ", false),
        ("compact", "é日本🙂 [compact](../sub/target.md#part) ", true),
        ("self", "é日本🙂 [self](page.md#part) ", true),
    ] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("{}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, Some(source.clone()));
                assert!(!rule.should_skip(&ctx));
                let warnings = rule.check(&ctx).unwrap();
                assert_eq!(warnings.len(), count);
                assert!(warnings.iter().all(|warning| warning.fix.is_some() == fixable));
                measure_rule_time_with_context(&rule, &ctx, 5)
            })
            .collect();
        assert_linear_complexity(&format!("MD057 {name}"), &times, 3.0);
    }
}

/// Literal directives in many code ranges must not make every live directive
/// search the entire range list in parsing, observation or warning filtering.
#[test]
fn test_directive_code_ranges_linear_complexity() {
    use rumdl_lib::inline_config::{InlineConfig, validate_inline_enables_against_active_rules};
    use rumdl_lib::rule::SuppressionReport;

    for stage in ["config", "observer", "validation"] {
        let directive = if stage == "validation" {
            "enable"
        } else {
            "disable-line"
        };
        for fenced in [false, true] {
            let unit = if fenced {
                format!("```md\n<!-- rumdl-{directive} MD013 -->\n```\n\n<!-- rumdl-{directive} MD013 -->\n\n")
            } else {
                format!("`<!-- rumdl-{directive} MD013 -->` <!-- rumdl-{directive} MD013 --> ")
            };
            let times: Vec<_> = [1024, 2048, 4096]
                .into_iter()
                .map(|count| {
                    let content = unit.repeat(count);
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    let ranges = rumdl_lib::lint_context::code_ranges(&content, MarkdownFlavor::Standard);
                    let mut report = SuppressionReport::default();
                    report.judged_rules.insert("MD013".into());
                    let rule = MD087UnusedDisableComment::new();
                    let active = std::collections::HashSet::new();
                    match stage {
                        "config" => {
                            let config =
                                InlineConfig::from_content_with_code_blocks(&content, &ranges.blocks, &ranges.spans);
                            assert!(config.is_rule_disabled("MD013", if fenced { 5 } else { 1 }));
                            if fenced {
                                assert!(!config.is_rule_disabled("MD013", 2));
                            }
                        }
                        "observer" => assert_eq!(rule.check_suppressions(&ctx, &report).unwrap().len(), count),
                        _ => assert_eq!(
                            validate_inline_enables_against_active_rules(
                                &content,
                                MarkdownFlavor::Standard,
                                &active,
                                &active
                            )
                            .len(),
                            count
                        ),
                    }
                    let run = || match stage {
                        "config" => {
                            std::hint::black_box(InlineConfig::from_content_with_code_blocks(
                                std::hint::black_box(&content),
                                &ranges.blocks,
                                &ranges.spans,
                            ));
                        }
                        "observer" => {
                            std::hint::black_box(rule.check_suppressions(std::hint::black_box(&ctx), &report).unwrap());
                        }
                        _ => {
                            std::hint::black_box(validate_inline_enables_against_active_rules(
                                std::hint::black_box(&content),
                                MarkdownFlavor::Standard,
                                &active,
                                &active,
                            ));
                        }
                    };
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            run();
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("directive ranges {stage}, fenced={fenced}"), &times, 3.0);
        }
    }
}

/// Suppression observers run after rule checks, so measure their actual entry
/// point with a report rather than MD087's deliberately empty `check`.
#[test]
fn test_md087_suppressions_linear_complexity() {
    use rumdl_lib::inline_config::DisableLayer;
    use rumdl_lib::rule::{SuppressedWarning, SuppressionReport};

    let rule = MD087UnusedDisableComment::new();
    for (name, unit, layer, line_step) in [
        ("unused-line", "é日本🙂 <!-- rumdl-disable-line MD013 --> ", None, 1),
        (
            "used-lines",
            "text <!-- rumdl-disable-line MD013 -->\n",
            Some(DisableLayer::Line),
            1,
        ),
        (
            "used-blocks",
            "<!-- rumdl-disable MD013 -->\ntext\n",
            Some(DisableLayer::Block),
            2,
        ),
        (
            "config-line",
            "é日本🙂 <!-- rumdl-configure-file {\"MD013\":false} --> ",
            None,
            1,
        ),
        (
            "config-lines",
            "<!-- rumdl-configure-file {\"MD013\":false} -->\n",
            None,
            1,
        ),
    ] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = unit.repeat(count);
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                let mut report = SuppressionReport::default();
                report.judged_rules.insert("MD013".into());
                if let Some(layer) = layer {
                    report.suppressed = (1..=count)
                        .map(|line| SuppressedWarning {
                            rule_name: "MD013".into(),
                            line: line * line_step,
                            layer,
                        })
                        .collect();
                }
                assert!(!rule.should_skip(&ctx));
                assert_eq!(
                    rule.check_suppressions(&ctx, &report).unwrap().len(),
                    if layer.is_some() { 0 } else { count }
                );
                let mut samples: Vec<_> = (0..7)
                    .map(|_| {
                        let start = MeasurementClock::now();
                        std::hint::black_box(
                            rule.check_suppressions(std::hint::black_box(&ctx), std::hint::black_box(&report))
                                .unwrap(),
                        );
                        start.elapsed()
                    })
                    .collect();
                samples.sort();
                samples[3]
            })
            .collect();
        assert_linear_complexity(&format!("MD087 {name}"), &times, 3.0);
    }
}

#[test]
fn test_configure_file_validation_linear_complexity() {
    use rumdl_lib::inline_config::validate_inline_config_rules;

    for separator in [" ", "\n"] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("<!-- rumdl-configure-file {{\"MD013\":false}} -->{separator}").repeat(count);
                assert!(validate_inline_config_rules(&content, MarkdownFlavor::Standard).is_empty());
                let mut samples: Vec<_> = (0..7)
                    .map(|_| {
                        let start = MeasurementClock::now();
                        std::hint::black_box(validate_inline_config_rules(
                            std::hint::black_box(&content),
                            MarkdownFlavor::Standard,
                        ));
                        start.elapsed()
                    })
                    .collect();
                samples.sort();
                samples[3]
            })
            .collect();
        assert_linear_complexity(&format!("configure-file validation {separator:?}"), &times, 3.0);
    }
}

/// Fence candidates in list continuations must share block lookups, including
/// both delimiters and compound-fix endpoints.
#[test]
fn test_md077_fence_lookup_linear_complexity() {
    let rule = MD077ListContinuationIndent::default();
    for (flavor, unit) in [
        (MarkdownFlavor::Standard, "- Item\n\n ```\n code\n ```\n\n"),
        (MarkdownFlavor::MkDocs, "1. Item\n\n   ```toml\n   key = 1\n   ```\n\n"),
    ] {
        let times: Vec<_> = [2048, 4096, 8192]
            .into_iter()
            .map(|count| {
                let content = unit.repeat(count);
                let ctx = LintContext::new(&content, flavor, None);
                assert!(!rule.should_skip(&ctx));
                let warnings = rule.check(&ctx).unwrap();
                assert_eq!(warnings.len(), count);
                assert!(warnings.iter().all(|warning| warning.fix.is_some()));
                measure_rule_time_with_context(&rule, &ctx, 7)
            })
            .collect();
        assert_linear_complexity(&format!("MD077 {flavor:?} fences"), &times, 3.0);
    }
}

/// Unrecognized numeric text must not rescan the same indented run per line.
#[test]
fn test_md030_indented_fallback_linear_complexity() {
    let rule = MD030ListMarkerSpace::default();
    for (name, unit) in [("spaces", "    2.bad\n"), ("tabs", "\t2.bad\n")] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("- Root\n{}", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(rule.check(&ctx).unwrap().is_empty());
                let mut samples: Vec<_> = (0..7)
                    .map(|_| {
                        let start = MeasurementClock::now();
                        std::hint::black_box(rule.check(&ctx).unwrap());
                        start.elapsed()
                    })
                    .collect();
                samples.sort();
                samples[3]
            })
            .collect();
        assert_linear_complexity(&format!("MD030 indented fallback {name}"), &times, 3.0);
    }
}

/// HTML tags in headings must not each search every opaque code/comment range.
#[test]
fn test_heading_anchor_lookup_linear_complexity() {
    use rumdl_lib::utils::header_id_utils::{extract_heading_text, extract_html_anchor_ids};

    for (name, unit, has_anchors) in [
        ("html-code", "<span>word</span> `code` ", false),
        ("anchors", "<a id=\"live\"></a>`<a id=\"literal\"></a>` ", true),
    ] {
        for stage in ["context", "text", "ids"] {
            let times: Vec<_> = [2048, 4096, 8192]
                .into_iter()
                .map(|count| {
                    let content = format!("# {}\n", unit.repeat(count));
                    let ids = extract_html_anchor_ids(&content);
                    assert_eq!(ids.len(), if has_anchors { count } else { 0 });
                    assert!(ids.iter().all(|id| id == "live"));
                    let run = || match stage {
                        "context" => {
                            std::hint::black_box(LintContext::new(&content, MarkdownFlavor::Standard, None));
                        }
                        "text" => {
                            std::hint::black_box(extract_heading_text(&content));
                        }
                        _ => {
                            std::hint::black_box(extract_html_anchor_ids(&content));
                        }
                    };
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            run();
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("heading anchors {name}/{stage}"), &times, 3.0);
        }
    }
}

/// Raw code/link fallbacks must not rescan a non-Markdown line for every name.
#[test]
fn test_md044_fallback_ranges_linear_complexity() {
    for (name, prefix, unit, suffix) in [
        ("comment", "<!--\n", "javascript text ", "\n-->"),
        ("html", "<div>\n", "javascript text ", "\n</div>"),
        ("frontmatter", "---\ntitle: ", "javascript text ", "\n---\n"),
        ("comment_links", "<!--\n", "[label](target) javascript ", "\n-->"),
        ("comment_code", "<!--\n", "`literal` javascript ", "\n-->"),
        ("html_code", "<div>\n", "`literal` javascript ", "\n</div>"),
        ("malformed_tail", "<!--\n[label](javascript) ", "[", "\n-->"),
        ("malformed_labels", "<!--\n", "[javascript ", "\n-->"),
        ("malformed_destinations", "<!--\n", "[label](javascript ", "\n-->"),
        ("malformed_references", "<!--\n", "[label][javascript ", "\n-->"),
    ] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("{prefix}{}{suffix}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                let expected = if name == "malformed_tail" { 0 } else { count };
                let body = &ctx.lines[1];
                assert!(body.in_html_comment || body.in_html_block || body.in_front_matter);
                assert_eq!(
                    MD044ProperNames::new(vec!["JavaScript".into()], false)
                        .check(&ctx)
                        .unwrap()
                        .len(),
                    expected
                );
                let mut samples: Vec<_> = (0..7)
                    .map(|_| {
                        let rule = MD044ProperNames::new(vec!["JavaScript".into()], false);
                        let start = MeasurementClock::now();
                        let warnings = std::hint::black_box(rule.check(&ctx).unwrap());
                        let elapsed = start.elapsed();
                        assert_eq!(warnings.len(), expected);
                        elapsed
                    })
                    .collect();
                samples.sort();
                samples[3]
            })
            .collect();
        assert_linear_complexity(&format!("MD044 fallback ranges {name}"), &times, 3.0);
    }
}

/// WikiLink fallback destinations must be scanned once across all name matches.
#[test]
fn test_md044_link_destinations_linear_complexity() {
    for (name, unit) in [
        ("regular", "[label](target) javascript "),
        ("wiki", "[[target]](path) javascript "),
        ("nested", "[[target]](a(b)c) javascript "),
        ("unclosed", "[[target]](path javascript "),
        ("overlap", "[[target]](a"),
    ] {
        let times: Vec<_> = [2048, 4096, 8192]
            .into_iter()
            .map(|count| {
                let content = if name == "overlap" {
                    format!("{}javascript{} javascript\n", unit.repeat(count), ")".repeat(count))
                } else {
                    format!("{}\n", unit.repeat(count))
                };
                let ctx = LintContext::new(&content, MarkdownFlavor::Obsidian, None);
                assert!(ctx.links().len() >= count);
                let expected = if name == "overlap" { 1 } else { count };
                assert_eq!(
                    MD044ProperNames::new(vec!["JavaScript".into()], false)
                        .check(&ctx)
                        .unwrap()
                        .len(),
                    expected
                );
                let mut samples: Vec<_> = (0..7)
                    .map(|_| {
                        let rule = MD044ProperNames::new(vec!["JavaScript".into()], false);
                        let start = MeasurementClock::now();
                        let warnings = std::hint::black_box(rule.check(&ctx).unwrap());
                        let elapsed = start.elapsed();
                        assert_eq!(warnings.len(), expected);
                        elapsed
                    })
                    .collect();
                samples.sort();
                samples[3]
            })
            .collect();
        assert_linear_complexity(&format!("MD044 link destinations {name}"), &times, 3.0);
    }
}

/// Name matches must not repeatedly scan a plain line for URLs or character columns.
#[test]
fn test_md044_long_lines_linear_complexity() {
    for (name, unit) in [("ascii", "javascript text "), ("unicode", "日本🙂 javascript text ")] {
        for stage in ["fresh", "cached"] {
            let times: Vec<_> = [2048, 4096, 8192]
                .into_iter()
                .map(|count| {
                    let content = format!("{}\n", unit.repeat(count));
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    let cached = MD044ProperNames::new(vec!["JavaScript".into()], false);
                    assert_eq!(cached.check(&ctx).unwrap().len(), count);
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            // Clones share the content cache; construct a new rule for cold scans.
                            let fresh = MD044ProperNames::new(vec!["JavaScript".into()], false);
                            let rule = if stage == "fresh" { &fresh } else { &cached };
                            let start = MeasurementClock::now();
                            let warnings = std::hint::black_box(rule.check(&ctx).unwrap());
                            let elapsed = start.elapsed();
                            assert_eq!(warnings.len(), count);
                            elapsed
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("MD044 long lines {name}/{stage}"), &times, 3.0);
        }
    }
}

/// Plain attribute lists between anchors must not force a fresh anchor scan.
#[test]
fn test_heading_anchor_attributes_linear_complexity() {
    let rule = MD024NoDuplicateHeading::default();
    for name in ["id", "attributes"] {
        for stage in ["range", "MD024"] {
            let times: Vec<_> = [512, 1024, 2048]
                .into_iter()
                .map(|count| {
                    let unit = if name == "id" {
                        "<a></a> {#id} "
                    } else {
                        "<a id=x></a> {: #id .class data='value'} "
                    };
                    let content = format!("# Word {}\n\n", unit.repeat(count)).repeat(2);
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
                    for heading in ctx.headings() {
                        assert_eq!(&content[heading.text_byte_range(&content)], "Word");
                    }
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            if stage == "range" {
                                for heading in ctx.headings() {
                                    std::hint::black_box(heading.text_byte_range(&content));
                                }
                            } else {
                                std::hint::black_box(rule.check(&ctx).unwrap());
                            }
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("heading anchor attributes {name}/{stage}"), &times, 3.0);
        }
    }
}

/// Distinct unmatched backtick lengths must not repeatedly scan the suffix.
#[test]
fn test_heading_backtick_lookup_linear_complexity() {
    use rumdl_lib::utils::header_id_utils::extract_heading_text;

    for name in ["unmatched", "paired", "recovered", "indexed-pairs"] {
        for stage in ["text", "context"] {
            let measurements: Vec<_> = [128, 256, 512]
                .into_iter()
                .map(|count| {
                    let unmatched = || {
                        (1..=count)
                            .map(|len| format!("{}x ", "`".repeat(len)))
                            .collect::<String>()
                    };
                    let paired = || {
                        (1..=count)
                            .map(|len| {
                                let ticks = "`".repeat(len);
                                format!("{ticks}x{ticks} ")
                            })
                            .collect::<String>()
                    };
                    let text = match name {
                        "unmatched" => unmatched(),
                        "paired" => paired(),
                        "recovered" => format!("{}x {}", "`".repeat(count + 1), paired()),
                        _ => {
                            let ticks = "`".repeat(count + 1);
                            format!("{}{}", unmatched(), format!("{ticks}x{ticks} ").repeat(count))
                        }
                    };
                    let content = format!("# {text}<span>text</span>\n");
                    assert_eq!(extract_heading_text(&content).text, content.trim());
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            if stage == "text" {
                                std::hint::black_box(extract_heading_text(&content));
                            } else {
                                std::hint::black_box(LintContext::new(&content, MarkdownFlavor::Standard, None));
                            }
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    (content.len(), samples[3])
                })
                .collect();
            // Doubling the number of distinct lengths roughly quadruples the
            // input bytes. Compare timing growth to bytes, not the run count.
            for pair in measurements.windows(2) {
                let byte_growth = pair[1].0 as f64 / pair[0].0 as f64;
                let time_growth = pair[1].1.as_secs_f64() / pair[0].1.as_secs_f64();
                eprintln!("backticks {name}/{stage}: {byte_growth:.2}x bytes, {time_growth:.2}x time");
                assert!(
                    time_growth < byte_growth * 1.5,
                    "backticks {name}/{stage}: timing grew {time_growth:.2}x for {byte_growth:.2}x bytes"
                );
            }
        }
    }
}

/// Malformed images must not search the same bracket/parenthesis ranges repeatedly.
#[test]
fn test_heading_image_delimiters_linear_complexity() {
    use rumdl_lib::utils::header_id_utils::extract_heading_text;

    for name in [
        "description",
        "destination",
        "reference",
        "closed",
        "valid",
        "recovered",
    ] {
        for stage in ["text", "context"] {
            let times: Vec<_> = [2048, 4096, 8192]
                .into_iter()
                .map(|count| {
                    let text = match name {
                        "description" => "![ ".repeat(count),
                        "destination" => "![a]( ".repeat(count),
                        "reference" => "![a][ ".repeat(count),
                        "closed" => format!("{}{}", "![ ".repeat(count), "]".repeat(count)),
                        "recovered" => format!("![ {}", "![alt](image) ".repeat(count)),
                        _ => "![alt](image) ".repeat(count),
                    };
                    let content = format!("# {text}<span>text</span>\n");
                    assert_eq!(extract_heading_text(&content).text, content.trim());
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            if stage == "text" {
                                std::hint::black_box(extract_heading_text(&content));
                            } else {
                                std::hint::black_box(LintContext::new(&content, MarkdownFlavor::Standard, None));
                            }
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("heading image delimiters {name}/{stage}"), &times, 3.0);
        }
    }
}

/// Removing blanks at anchor seams must not repeatedly shift the heading suffix.
#[test]
fn test_heading_anchor_spacing_linear_complexity() {
    use rumdl_lib::utils::header_id_utils::extract_heading_text;

    for (name, unit, visible) in [
        ("single", "word <a id=x></a> ", "word "),
        ("adjacent", "é <a></a><a></a>  🙂 ", "é 🙂 "),
    ] {
        for stage in ["text", "context"] {
            let times: Vec<_> = [8192, 16384, 32768]
                .into_iter()
                .map(|count| {
                    let content = format!("# {}\n", unit.repeat(count));
                    assert_eq!(
                        extract_heading_text(&content).text,
                        format!("# {}", visible.repeat(count).trim_end())
                    );
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            if stage == "text" {
                                std::hint::black_box(extract_heading_text(&content));
                            } else {
                                std::hint::black_box(LintContext::new(&content, MarkdownFlavor::Standard, None));
                            }
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("anchor spacing {name}/{stage}"), &times, 3.0);
        }
    }
}

/// Removing trailing anchors must not rescan the heading for each element.
#[test]
fn test_heading_trailing_anchors_linear_complexity() {
    let rule = MD024NoDuplicateHeading::default();
    for (name, suffix) in [("plain", ""), ("custom-id", "{#custom}")] {
        for stage in ["range", "MD024"] {
            let times: Vec<_> = [512, 1024, 2048]
                .into_iter()
                .map(|count| {
                    let content = format!("# Word {}{suffix}\n\n", "<a id=x></a> ".repeat(count)).repeat(2);
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
                    for heading in ctx.headings() {
                        assert_eq!(&content[heading.text_byte_range(&content)], "Word");
                    }
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            if stage == "range" {
                                for heading in ctx.headings() {
                                    std::hint::black_box(heading.text_byte_range(&content));
                                }
                            } else {
                                std::hint::black_box(rule.check(&ctx).unwrap());
                            }
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("trailing anchors {name}/{stage}"), &times, 3.0);
        }
    }
}

/// Constructing nested JSX must not walk from the AST root for every node.
#[test]
fn test_mdx_nested_context_linear_complexity() {
    for name in ["nested", "unmatched", "paired"] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let text = match name {
                    "nested" => format!("{}text{}", "<b>".repeat(count), "</b>".repeat(count)),
                    "unmatched" => format!("{}{}", "<b>".repeat(count), "</i>".repeat(count)),
                    _ => "<b>text</b> ".repeat(count),
                };
                let content = format!("# {text}\n");
                let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
                assert_eq!(ctx.headings().count(), 1);
                let mut samples: Vec<_> = (0..7)
                    .map(|_| {
                        let start = MeasurementClock::now();
                        std::hint::black_box(LintContext::new(&content, MarkdownFlavor::MDX, None));
                        start.elapsed()
                    })
                    .collect();
                samples.sort();
                samples[3]
            })
            .collect();
        assert_linear_complexity(&format!("MDX nested context {name}"), &times, 3.0);
    }
}

/// Missing closing-tag names must not repeatedly search the open-element stack.
#[test]
fn test_md063_tag_pairing_linear_complexity() {
    let rule = MD063HeadingCapitalization::default();
    for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX] {
        for name in ["unmatched", "distinct", "paired", "nested", "recovered"] {
            let times: Vec<_> = [2048, 4096, 8192]
                .into_iter()
                .map(|count| {
                    let text = match name {
                        "paired" => "<b>text</b> ".repeat(count),
                        "recovered" => format!("<b></missing></b> {}", "<b>text</b> ".repeat(count)),
                        "nested" => format!("{}text{}", "<b>".repeat(count), "</b>".repeat(count)),
                        "distinct" => format!(
                            "{}{}",
                            "<b>".repeat(count),
                            (0..count).map(|i| format!("</missing{i}> ")).collect::<String>()
                        ),
                        _ => format!("{}{}", "<b>".repeat(count), "</i>".repeat(count)),
                    };
                    let content = format!("# {text}\n");
                    let ctx = LintContext::new(&content, flavor, None);
                    assert!(rule.check(&ctx).unwrap().is_empty());
                    let mut samples: Vec<_> = (0..7)
                        .map(|_| {
                            let start = MeasurementClock::now();
                            std::hint::black_box(rule.check(&ctx).unwrap());
                            start.elapsed()
                        })
                        .collect();
                    samples.sort();
                    samples[3]
                })
                .collect();
            assert_linear_complexity(&format!("MD063 tag pairing {flavor:?}/{name}"), &times, 3.0);
        }
    }
}

/// Each heading region must not scan every earlier region or code span.
#[test]
fn test_md063_heading_regions_linear_complexity() {
    let rule = MD063HeadingCapitalization::default();
    for (name, unit, count_warnings) in [
        ("formatting", "é日本🙂 **strong** *emphasis* `code` ", 1),
        ("html-code", "<span>word</span> `code` ", 0),
        ("links", "[word](target) `code` ", 1),
    ] {
        let times: Vec<_> = [2048, 4096, 8192]
            .into_iter()
            .map(|count| {
                let content = format!("# {}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(!rule.should_skip(&ctx));
                assert_eq!(rule.check(&ctx).unwrap().len(), count_warnings);
                measure_rule_time_with_context(&rule, &ctx, 7)
            })
            .collect();
        assert_linear_complexity(&format!("MD063 heading regions {name}"), &times, 3.0);
    }
}

/// Dense code blocks inside one list item must not rescan all earlier lines
/// when determining the indentation needed to continue that list.
#[test]
fn test_list_code_context_linear_complexity() {
    for blank in [false, true] {
        let unit = if blank {
            "   ```\n   code\n   ```\n\n"
        } else {
            "   ```\n   code\n   ```\n"
        };
        let times: Vec<_> = [512, 1024, 2048]
            .into_iter()
            .map(|count| {
                let content = format!("- Item\n{}", unit.repeat(count));
                let mut samples: Vec<_> = (0..7)
                    .map(|_| {
                        let start = MeasurementClock::now();
                        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                        let elapsed = start.elapsed();
                        assert_eq!(ctx.code_block_details.len(), count);
                        assert_eq!(ctx.list_blocks.len(), 1);
                        std::hint::black_box(ctx);
                        elapsed
                    })
                    .collect();
                samples.sort();
                samples[3]
            })
            .collect();
        assert_linear_complexity(&format!("list code context, blank={blank}"), &times, 3.0);
    }
}

/// Opting lists out of MD031 must not rescan their preceding content per fence.
#[test]
fn test_md031_list_membership_linear_complexity() {
    let rule = MD031BlanksAroundFences::new(false);
    for (name, flavor, unit) in [
        ("fences", MarkdownFlavor::Standard, "   ```\n   code\n   ```\n"),
        ("admonitions", MarkdownFlavor::MkDocs, "   !!! note\n       contents\n"),
    ] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("- Item\n{}", unit.repeat(count));
                let ctx = LintContext::new(&content, flavor, None);
                assert!(!rule.should_skip(&ctx));
                assert!(rule.check(&ctx).unwrap().is_empty());
                assert_eq!(rule.fix(&ctx).unwrap(), content);
                measure_rule_time_with_context(&rule, &ctx, 7)
            })
            .collect();
        assert_linear_complexity(&format!("MD031 list membership {name}"), &times, 3.0);
    }
}

/// Spacing checks must not search every fence for each source line or list gap.
#[test]
fn test_spacing_fence_lookup_linear_complexity() {
    let rules: [&dyn Rule; 2] = [&MD031BlanksAroundFences::default(), &MD076ListItemSpacing::default()];
    for rule in rules {
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs] {
            let times: Vec<_> = [2048, 4096, 8192]
                .into_iter()
                .map(|count| {
                    let content = "1. Item\n\n   ```toml\n   key = 1\n   ```\n\n".repeat(count);
                    let ctx = LintContext::new(&content, flavor, None);
                    assert!(!rule.should_skip(&ctx));
                    assert_eq!(ctx.code_block_details.len(), count);
                    assert!(rule.check(&ctx).unwrap().is_empty());
                    assert_eq!(rule.fix(&ctx).unwrap(), content);
                    measure_rule_time_with_context(rule, &ctx, 7)
                })
                .collect();
            assert_linear_complexity(&format!("{} {flavor:?} fence spacing", rule.name()), &times, 3.0);
        }
    }
}

/// Opening fences must not each search the document's entire block list.
#[test]
fn test_md070_fence_lookup_linear_complexity() {
    let rule = MD070NestedCodeFence::new();
    for (name, unit) in [
        ("closed", "```rust\nlet x = 1;\n```\n\n"),
        ("container-ended", "- Item:\n\n  ```yaml\n  a: b\n\nEnd.\n\n"),
    ] {
        let times: Vec<_> = [2048, 4096, 8192]
            .into_iter()
            .map(|count| {
                let content = unit.repeat(count);
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(!rule.should_skip(&ctx));
                assert_eq!(ctx.code_block_details.len(), count);
                assert!(rule.check(&ctx).unwrap().is_empty());
                measure_rule_time_with_context(&rule, &ctx, 7)
            })
            .collect();
        assert_linear_complexity(&format!("MD070 {name}"), &times, 3.0);
    }
}

/// A block's search for its intended closer must not walk the quotes or list
/// items nested inside its own container. Every level holds a colliding block,
/// so the input grows with the depth squared: four times the depth is sixteen
/// times the input. A search bounded by the nested container costs about 16x;
/// one that scans every deeper level from every block costs about 64x.
#[test]
fn test_md070_nested_container_search_linear_complexity() {
    let rule = MD070NestedCodeFence::new();
    let quote = |k: usize| {
        let p = "> ".repeat(k + 1);
        format!("{p}```markdown\n{p}```python\n{p}x\n{p}```\n")
    };
    let list = |k: usize| {
        let p = "  ".repeat(k);
        format!("{p}- item\n\n{p}  ```markdown\n{p}  ```python\n{p}  x\n{p}  ```\n\n")
    };
    for (name, level) in [("quote", &quote as &dyn Fn(usize) -> String), ("list", &list)] {
        let measure = |depth: usize| {
            let content: String = (0..depth).map(level).collect();
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), depth, "MD070 nested {name}");
            measure_rule_time_with_context(&rule, &ctx, 7)
        };
        let ratio = measure(256).as_secs_f64() / measure(64).as_secs_f64();
        assert!(
            ratio < 32.0,
            "MD070 nested {name}: 16x the input took {ratio:.1}x the time"
        );
    }
}

/// Warning positions must not rescan every preceding construct on a heading.
#[test]
fn test_md093_warning_columns_linear_complexity() {
    let rule = MD093NoFormattingInHeadings::default();
    for (name, prefix, suffix) in [
        ("atx", "# ", "\n"),
        ("setext", "", "\n===\n"),
        ("quoted", "> # ", "\r\n"),
    ] {
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!(
                    "{prefix}{}{suffix}",
                    "é日本🙂 **strong** *emphasis* `code` ".repeat(count)
                );
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(!rule.should_skip(&ctx));
                assert_eq!(rule.check(&ctx).unwrap().len(), count * 3);
                measure_rule_time(&rule, &content, 5)
            })
            .collect();
        assert_linear_complexity(&format!("MD093 {name}"), &times, 3.0);
    }
}

/// Building fixes must not rescan a Unicode line for every warning.
#[test]
fn test_md084_fix_ranges_linear_complexity() {
    for strict in [false, true] {
        let mut config = rumdl_lib::config::Config::default();
        config
            .rules
            .entry("MD084".to_string())
            .or_default()
            .values
            .insert("strict".to_string(), strict.into());
        let rule = MD084InvisibleCharacters::from_config(&config);
        for (name, unit, expected) in [
            ("invisible", "é日本🙂 x\u{200b} y ", 1),
            ("tone-mark", "é日本🙂 x\u{0340} y ", 1),
            ("clusters", "é日本🙂 x\u{200b}\u{200b}y ", if strict { 2 } else { 1 }),
            ("nonfixable", "é日本🙂 x\u{fffc}y ", 1),
            ("sparse", "é日本🙂 text ", 0),
        ] {
            let times: Vec<_> = [1024, 2048, 4096]
                .into_iter()
                .map(|count| {
                    let suffix = if name == "sparse" { " \u{200b}" } else { "" };
                    let content = format!("{}{suffix}\n", unit.repeat(count));
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    assert!(!rule.should_skip(&ctx));
                    let warnings = rule.check(&ctx).unwrap();
                    assert_eq!(warnings.len(), expected * count + usize::from(name == "sparse"));
                    assert!(
                        warnings
                            .iter()
                            .all(|warning| warning.fix.is_some() == (name != "nonfixable"))
                    );
                    measure_rule_time(rule.as_ref(), &content, 5)
                })
                .collect();
            assert_linear_complexity(&format!("MD084 {name} strict={strict}"), &times, 3.0);
        }
    }
}

/// Every heading candidate must not rescan every link in the document.
#[test]
fn test_heading_link_membership_linear_complexity() {
    for (name, unit, flavor) in [
        (
            "atx",
            "## Repeated heading\n\nparagraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::Standard,
        ),
        (
            "list",
            "- ## Repeated heading\n\n  paragraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::Standard,
        ),
        (
            "quoted-list",
            "> - ## Repeated heading\n>\n>   paragraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::Standard,
        ),
        (
            "mdx",
            "## Repeated heading\n\nparagraph [jump](#repeated-heading).\n\n",
            MarkdownFlavor::MDX,
        ),
    ] {
        let times: Vec<_> = [4096, 8192, 16384]
            .into_iter()
            .map(|count| {
                let content = unit.repeat(count);
                let ctx = LintContext::new(&content, flavor, None);
                assert_eq!(ctx.headings().count(), count);
                let mut samples: Vec<_> = (0..5)
                    .map(|_| {
                        let start = MeasurementClock::now();
                        std::hint::black_box(LintContext::new(std::hint::black_box(&content), flavor, None));
                        start.elapsed()
                    })
                    .collect();
                samples.sort();
                samples[2]
            })
            .collect();
        assert_linear_complexity(&format!("heading link membership {name}"), &times, 3.0);
    }
}

/// Image membership and warning columns must scale on long Unicode lines.
#[test]
fn test_md089_indexes_linear_complexity() {
    for (name, unit, expected) in [
        (
            "links",
            "é日本🙂 [label](https://example.com) ![图](https://example.com/image.png) ",
            0,
        ),
        ("badges", "中文[![图](img.png)](target)中文 ", 0),
        ("mixed", "中文[label](target)中文 ![图](img.png) ", 2),
        ("warnings", "é日本🙂 中文english ", 1),
    ] {
        let rule = MD089CjkSpacing::default();
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("{}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(!rule.should_skip(&ctx));
                assert_eq!(rule.check(&ctx).unwrap().len(), count * expected);
                measure_rule_time(&rule, &content, 5)
            })
            .collect();
        assert_linear_complexity(&format!("MD089 indexes {name}"), &times, 3.0);
    }
}

/// Source positions must scale with input size on long Unicode lines.
#[test]
fn test_md095_source_positions_linear_complexity() {
    for (unit, expected) in [
        ("é日本🙂 &amp; <span>text</span> ", 0),
        ("é日本🙂 [x](https://еxample.com) ", 1),
        ("é日本🙂 <a href=\"https://&#x435;xample.com\">x</a> ", 1),
        ("é日本🙂 https://еxample.com ", 1),
        ("<div> é日本🙂 [x](https://еxample.com) </div> ", 0),
    ] {
        let rule = MD095LinkConfusables;
        let times: Vec<_> = [1024, 2048, 4096]
            .into_iter()
            .map(|count| {
                let content = format!("{}\n", unit.repeat(count));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert!(!rule.should_skip(&ctx));
                assert_eq!(rule.check(&ctx).unwrap().len(), count * expected);
                measure_rule_time(&rule, &content, 5)
            })
            .collect();
        assert_linear_complexity(&format!("MD095 positions {unit:?}"), &times, 3.0);
    }
}

/// Assert linear complexity by checking growth ratios
fn assert_linear_complexity(name: &str, durations: &[Duration], threshold: f64) {
    assert!(durations.len() >= 3, "Need at least 3 measurements for ratio check");

    let ratio_1 = durations[1].as_secs_f64() / durations[0].as_secs_f64();
    let ratio_2 = durations[2].as_secs_f64() / durations[1].as_secs_f64();

    // Every caller doubles the input size per step, so the ratios are the cost
    // of one doubling and the sizes themselves need not be named here.
    println!("{name} scaling per doubling: {ratio_1:.2}x then {ratio_2:.2}x");

    assert!(
        ratio_1 < threshold,
        "{name} shows non-linear scaling: first doubling took {ratio_1:.2}x (threshold: {threshold}x)"
    );
    assert!(
        ratio_2 < threshold,
        "{name} shows non-linear scaling: second doubling took {ratio_2:.2}x (threshold: {threshold}x)"
    );
}

// =============================================================================
// Content Generators
// =============================================================================

/// Generate document with nested lists (tests MD004, MD005, MD007, MD030, MD032)
fn generate_list_document(num_entries: usize) -> String {
    let mut content = String::with_capacity(num_entries * 100);
    content.push_str("# List Document\n\n");

    for i in 0..num_entries {
        // Mix of ordered and unordered lists with varying nesting
        if i % 3 == 0 {
            content.push_str(&format!("- Item {i}\n"));
            content.push_str(&format!("  - Nested item {i}.1\n"));
            content.push_str(&format!("    - Deeply nested {i}.1.1\n"));
        } else if i % 3 == 1 {
            content.push_str(&format!("1. Ordered item {i}\n"));
            content.push_str(&format!("   1. Nested ordered {i}.1\n"));
        } else {
            content.push_str(&format!("* Unordered {i}\n"));
            content.push_str(&format!("  * Sub-item {i}.1\n"));
        }
    }

    content.push_str("\nEnd of lists.\n");
    content
}

/// Generate document with many headings (tests MD001, MD003, MD018-MD023, MD025, MD026)
fn generate_heading_document(num_headings: usize) -> String {
    let mut content = String::with_capacity(num_headings * 60);
    content.push_str("# Main Title\n\n");

    for i in 1..num_headings {
        let level = (i % 5) + 1; // Cycle through h1-h5
        let hashes = "#".repeat(level);

        // Mix of ATX styles
        if i % 4 == 0 {
            content.push_str(&format!("{hashes} Heading {i} {hashes}\n\n"));
        } else if i % 4 == 1 {
            content.push_str(&format!("{hashes} Heading {i}\n\n"));
        } else if i % 4 == 2 {
            // Closed ATX with potential issues
            content.push_str(&format!("{hashes}  Heading {i}  {hashes}\n\n"));
        } else {
            content.push_str(&format!("{hashes} Heading {i}:\n\n"));
        }

        content.push_str("Some paragraph content.\n\n");
    }

    content
}

/// Generate document with paragraphs and emphasis (tests MD013, MD036, MD037, MD049, MD050, MD064)
fn generate_paragraph_document(num_paragraphs: usize) -> String {
    let mut content = String::with_capacity(num_paragraphs * 200);
    content.push_str("# Text Document\n\n");

    for i in 0..num_paragraphs {
        // Vary emphasis styles
        if i % 4 == 0 {
            content.push_str(&format!(
                "This is paragraph {i} with *emphasis* and **strong** text.\n\n"
            ));
        } else if i % 4 == 1 {
            content.push_str(&format!(
                "Paragraph {i} uses _underscores_ for _emphasis_ and __strong__.\n\n"
            ));
        } else if i % 4 == 2 {
            // Potential MD036 trigger (emphasis-only line)
            content.push_str(&format!("**Bold heading style {i}**\n\n"));
            content.push_str("Normal content follows.\n\n");
        } else {
            // Long line for MD013
            content.push_str(&format!(
                "This is a longer paragraph number {i} that contains multiple words and might trigger line length checks if it exceeds the configured maximum line length threshold.\n\n"
            ));
        }
    }

    content
}

/// Generate document with links (tests MD034, MD039, MD042, MD051-MD054, MD059, MD062)
fn generate_link_document(num_links: usize) -> String {
    let mut content = String::with_capacity(num_links * 100);
    content.push_str("# Links Document\n\n");

    for i in 0..num_links {
        if i % 5 == 0 {
            // Standard link
            content.push_str(&format!("- [Link {i}](https://example.com/{i})\n"));
        } else if i % 5 == 1 {
            // Reference link
            content.push_str(&format!("- [Link {i}][ref{i}]\n"));
        } else if i % 5 == 2 {
            // Image
            content.push_str(&format!("- ![Image {i}](https://example.com/img{i}.png)\n"));
        } else if i % 5 == 3 {
            // Bare URL (MD034)
            content.push_str(&format!("- https://example.com/bare{i}\n"));
        } else {
            // Fragment link
            content.push_str(&format!("- [Section {i}](#section-{i})\n"));
        }
    }

    // Add reference definitions
    content.push('\n');
    for i in 0..num_links {
        if i % 5 == 1 {
            content.push_str(&format!("[ref{i}]: https://example.com/ref{i}\n"));
        }
    }

    // Add target sections for fragment links
    content.push('\n');
    for i in 0..num_links {
        if i % 5 == 4 {
            content.push_str(&format!("## Section {i}\n\nContent for section {i}.\n\n"));
        }
    }

    content
}

/// Generate document with code blocks (tests MD014, MD031, MD038, MD040, MD046, MD048)
fn generate_code_document(num_blocks: usize) -> String {
    let mut content = String::with_capacity(num_blocks * 150);
    content.push_str("# Code Document\n\n");

    for i in 0..num_blocks {
        content.push_str(&format!("## Code Example {i}\n\n"));

        if i % 3 == 0 {
            // Fenced code with language
            content.push_str("```rust\n");
            content.push_str(&format!("fn example_{i}() {{\n"));
            content.push_str(&format!("    println!(\"Hello {i}\");\n"));
            content.push_str("}\n");
            content.push_str("```\n\n");
        } else if i % 3 == 1 {
            // Fenced code without language (MD040)
            content.push_str("```\n");
            content.push_str(&format!("$ echo \"Command {i}\"\n"));
            content.push_str(&format!("Output {i}\n"));
            content.push_str("```\n\n");
        } else {
            // Inline code (MD038)
            content.push_str(&format!(
                "Use the `function_{i}()` method with ` spaced ` arguments.\n\n"
            ));
        }
    }

    content
}

/// Generate a document whose code spans all share one line (tests MD038)
///
/// `generate_code_document` gives every code span a line of its own, which
/// never exercises the same-line neighbour scan.
fn generate_single_line_code_spans(num_spans: usize) -> String {
    let mut content = String::with_capacity(num_spans * 5 + 32);
    content.push_str("# Inline Code\n\n");

    for _ in 0..num_spans {
        content.push_str("`a ` ");
    }
    content.push('\n');

    content
}

/// Generate mixed document for structural rules (tests MD012, MD041, MD047)
fn generate_mixed_document(num_sections: usize) -> String {
    let mut content = String::with_capacity(num_sections * 300);
    // Note: No leading heading to test MD041

    for i in 0..num_sections {
        content.push_str(&format!("## Section {i}\n\n"));

        // Content with varying blank lines (MD012)
        content.push_str(&format!("Paragraph one in section {i}.\n"));
        if i % 3 == 0 {
            content.push_str("\n\n\n"); // Multiple blanks
        } else {
            content.push('\n');
        }
        content.push_str(&format!("Paragraph two in section {i}.\n\n"));

        // List
        content.push_str(&format!("- Item {i}.1\n"));
        content.push_str(&format!("- Item {i}.2\n\n"));

        // Code
        content.push_str("```\n");
        content.push_str(&format!("code {i}\n"));
        content.push_str("```\n\n");
    }

    // MD047: Ensure trailing newline
    content.push_str("Final content.\n");
    content
}

/// Generate the EXACT document structure that caused issue #148
/// This is a "regression anchor" - if this test fails, issue #148 has regressed
fn generate_issue_148_document(num_entries: usize) -> String {
    let mut content = String::with_capacity(num_entries * 150);
    content.push_str("# Work Log\n\n");

    for i in 0..num_entries {
        content.push_str(&format!("- day-{i}: 2025-06-{:02}\n", (i % 28) + 1));
        content.push_str("  - task: 09:00-10:00\n");
        content.push_str(">  Extra space after marker\n"); // Triggers MD027
        content.push_str("    - fix: add field\n");
        content.push_str(&format!("    - fix: \"json_tag\": \"[{i}]\"\n"));
        content.push_str("    - fix: \"local_field\": [\"record_id\"]\n");
    }

    content
}

// =============================================================================
// MD026 Tests
// =============================================================================

/// The setext structure guard must look at the heading's own block, not at the
/// whole document, so the cost of a check grows with the number of headings.
#[test]
fn test_md026_setext_structure_guard_linear_complexity() {
    let rule = MD026NoTrailingPunctuation::default();
    // The last two put every heading in one list item or one nested container
    for (head, unit) in [
        ("", "- .\n  ===\n\n"),
        ("", "> <span>a</span>.\n> ===\n>\n"),
        ("", "<span>a</span>.\n===\n\n"),
        ("- item\n\n", "  #hashtag.\n  ==========\n\n"),
        ("> - item\n>\n", ">   <span>a</span>.\n>   ===\n>\n"),
        ("", "-\t<span>a</span>.\n\t===\n\n"),
    ] {
        let durations: Vec<_> = [250, 500, 1000]
            .iter()
            .map(|&size| measure_rule_time(&rule, &format!("{head}{}", unit.repeat(size)), 3))
            .collect();
        assert_linear_complexity(&format!("MD026 {head:?} {unit:?}"), &durations, 3.0);
    }
}

/// MD027 asks per quoted line whether the quote above it held a list; the answer
/// must come from the running state of the quote, not a scan back through it.
#[test]
fn test_md027_list_continuation_lookup_linear_complexity() {
    let rule = MD027MultipleSpacesBlockquote::default();
    for unit in [
        "> quote line text\n",
        "> - item\n>   continued\n",
        ">  spaced quote line\n",
    ] {
        let durations: Vec<_> = [1000, 2000, 4000]
            .iter()
            .map(|&size| measure_rule_time(&rule, &unit.repeat(size), 3))
            .collect();
        assert_linear_complexity(&format!("MD027 {unit:?}"), &durations, 3.0);
    }
}

/// MD023 asks per quoted heading whether an item may be open at its quote depth; the
/// answer must come from the running state of the quotes above it, not a scan back.
#[test]
fn test_md023_quoted_list_flags_linear_complexity() {
    let rule = MD023HeadingStartLeft;
    for unit in [
        "> > - item\n>\n>    ## H\n>\n",
        "> - item\n>\n>   >    ## H\n>\n",
        "> > - a\n> - b\n>   >    ## H\n>\n",
        "> - > - item\n>\n>   >    ## H\n>\n",
    ] {
        let durations: Vec<_> = [500, 1000, 2000]
            .iter()
            .map(|&size| measure_rule_time(&rule, &unit.repeat(size), 3))
            .collect();
        assert_linear_complexity(&format!("MD023 {unit:?}"), &durations, 3.0);
    }
}

#[test]
fn test_md033_long_html_line_linear_complexity() {
    let rule = MD033NoInlineHtml::default();
    for unit in ["text <span>x</span> ", "é日本🙂 <span>x</span> "] {
        let durations: Vec<_> = [8192, 16384, 32768]
            .into_iter()
            .map(|size| measure_rule_time(&rule, &unit.repeat(size), 5))
            .collect();
        assert_linear_complexity(&format!("MD033 {unit:?}"), &durations, 3.0);
    }
}

#[test]
fn test_md033_allowed_inside_linear_complexity() {
    let config = toml::from_str("allowed-inside = ['details', 'section']").unwrap();
    let rule = MD033NoInlineHtml::from_config_struct(config);
    for unit in [
        "<details><b>inside</b></details><i>outside</i>\n",
        "<details><section><b>inside</b></section><i>inside</i></details><b>outside</b>\n",
    ] {
        let durations: Vec<_> = [4096, 8192, 16384]
            .into_iter()
            .map(|size| measure_rule_time(&rule, &unit.repeat(size), 5))
            .collect();
        assert_linear_complexity("MD033 allowed-inside", &durations, 3.0);
    }
}

// =============================================================================
// LintContext Construction Tests
// =============================================================================

#[test]
fn test_lint_context_construction_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            println!("Testing LintContext with {size} sections ({} bytes)", content.len());
            measure_context_time(&content, iterations)
        })
        .collect();

    assert_linear_complexity("LintContext::new", &durations, 6.0);
}

#[test]
fn test_lint_context_link_dense_prose_linear_complexity() {
    // Every prose line carries links, so a per-line scan of all link ranges
    // grows with lines times links. Doubling the input then costs close to 4x,
    // while linear construction stays near 2x.
    let sizes = [10000, 20000, 40000];
    let iterations = 3;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content: String = (0..size)
                .map(|i| format!("Line {i} with [a link](https://example.com/{i}) and [another](#x{i}).\n"))
                .collect();
            measure_context_time(&content, iterations)
        })
        .collect();

    assert_linear_complexity("LintContext::new (link-dense prose)", &durations, 3.0);
}

#[test]
fn test_lint_context_long_link_line_linear_complexity() {
    for (name, unit) in [
        ("ascii", "[link](https://example.com) <hello@example.com> "),
        (
            "unicode",
            "é日本🙂 [![图](image.png)](https://example.com) [未解決][missing] ",
        ),
    ] {
        let durations: Vec<_> = [4096, 8192, 16384]
            .into_iter()
            .map(|size| measure_context_time(&unit.repeat(size), 3))
            .collect();
        assert_linear_complexity(&format!("LintContext::new (long {name} link line)"), &durations, 3.0);
    }
}

#[test]
fn test_inline_element_columns_linear_complexity() {
    for (name, unit) in [
        ("code", "text `code` "),
        ("math", "é日本🙂 $x + y$ "),
        ("html", "é日本🙂 <span>x</span> "),
    ] {
        let durations: Vec<_> = [8192, 16384, 32768]
            .into_iter()
            .map(|size| {
                let content = unit.repeat(size);
                let measure_first_access = || {
                    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                    // Time the cold cache only. Context construction and drop
                    // must not dilute the growth of column materialization.
                    let start = MeasurementClock::now();
                    match name {
                        "code" => {
                            std::hint::black_box(ctx.code_spans());
                        }
                        "math" => {
                            std::hint::black_box(ctx.math_spans());
                        }
                        "html" => {
                            std::hint::black_box(ctx.html_tags());
                        }
                        _ => unreachable!(),
                    }
                    start.elapsed()
                };
                measure_first_access();
                let mut samples: Vec<_> = (0..3).map(|_| measure_first_access()).collect();
                samples.sort();
                samples[1]
            })
            .collect();
        assert_linear_complexity(&format!("inline {name} columns"), &durations, 3.0);
    }
}

#[test]
fn test_lint_context_colon_definitions_linear_complexity() {
    let durations: Vec<_> = [4096, 8192, 16384]
        .into_iter()
        .map(|size| {
            let content: String = (0..size).map(|i| format!("Term {i} `code`\n::: body\n\n")).collect();
            measure_context_time(&content, 3)
        })
        .collect();

    assert_linear_complexity("LintContext::new (colon definitions)", &durations, 3.0);
}

#[test]
fn test_lint_context_div_markers_linear_complexity() {
    let durations: Vec<_> = [8192, 16384, 32768]
        .into_iter()
        .map(|size| {
            let content: String = (0..size)
                .map(|i| format!("# Term {i} `code`\n\n::: body\n\n"))
                .collect();
            measure_context_time(&content, 5)
        })
        .collect();
    assert_linear_complexity("LintContext::new (div markers)", &durations, 3.0);
}

#[test]
fn test_md013_table_body_linear_complexity() {
    let rule = MD013LineLength::from_config_struct(MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    });
    // Delimiter-like body rows belong to the first table; each must not
    // start another scan over all remaining body rows.
    let durations: Vec<_> = [1024, 2048, 4096]
        .into_iter()
        .map(|size| measure_rule_time(&rule, &"cell\n---:\n".repeat(size), 3))
        .collect();

    assert_linear_complexity("MD013 table body", &durations, 3.0);
}

#[test]
fn test_inline_config_multi_line_directives_linear_complexity() {
    use rumdl_lib::inline_config::InlineConfig;
    let durations: Vec<_> = [1024, 2048, 4096]
        .into_iter()
        .map(|size| {
            let content = "<!--\nrumdl-disable-next-line\nMD013\n-->\ntext\n\n".repeat(size);
            let _ = InlineConfig::from_content(&content);
            let mut times: Vec<Duration> = (0..3)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let _ = std::hint::black_box(InlineConfig::from_content(std::hint::black_box(&content)));
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[1]
        })
        .collect();
    assert_linear_complexity("InlineConfig::from_content (multi-line directives)", &durations, 3.0);
}

#[test]
fn test_inline_config_code_examples_with_one_closer_linear_complexity() {
    use rumdl_lib::inline_config::InlineConfig;
    // A fenced example of many unterminated openers followed by a single closer:
    // each opener is rejected as code without parsing the text up to the closer.
    let durations: Vec<_> = [1024, 2048, 4096]
        .into_iter()
        .map(|size| {
            let content = format!("```\n{}-->\n```\n", "<!-- rumdl-disable MD013\n".repeat(size));
            let _ = InlineConfig::from_content(&content);
            let mut times: Vec<Duration> = (0..3)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let _ = std::hint::black_box(InlineConfig::from_content(std::hint::black_box(&content)));
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[1]
        })
        .collect();
    assert_linear_complexity("InlineConfig::from_content (code openers, one closer)", &durations, 3.0);
}

#[test]
fn test_inline_config_markers_many_comments_on_one_line_linear_complexity() {
    use rumdl_lib::inline_config::has_inline_config_markers;
    // Ordinary comments crowded on a single line: the candidate check must not
    // look back over the growing line for each one.
    let durations: Vec<_> = [8192, 16384, 32768]
        .into_iter()
        .map(|size| {
            let content = "<!-- plain --> ".repeat(size);
            let _ = has_inline_config_markers(&content);
            let mut times: Vec<Duration> = (0..3)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let _ = std::hint::black_box(has_inline_config_markers(std::hint::black_box(&content)));
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[1]
        })
        .collect();
    assert_linear_complexity("has_inline_config_markers (one line)", &durations, 3.0);
}

#[test]
fn test_inline_config_many_comments_on_one_line_after_a_directive_linear_complexity() {
    use rumdl_lib::inline_config::InlineConfig;
    // One directive, then ordinary comments crowded on a single line with no
    // newline after them: deciding whether each spans lines must not search the
    // rest of the document again.
    let durations: Vec<_> = [8192, 16384, 32768]
        .into_iter()
        .map(|size| {
            let content = format!("<!-- rumdl-disable MD013 -->\n{}", "<!-- plain --> ".repeat(size));
            let _ = InlineConfig::from_content(&content);
            let mut times: Vec<Duration> = (0..3)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let _ = std::hint::black_box(InlineConfig::from_content(std::hint::black_box(&content)));
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[1]
        })
        .collect();
    assert_linear_complexity("InlineConfig::from_content (one line of comments)", &durations, 3.0);
}

#[test]
fn test_inline_config_crowded_openers_before_a_later_closer_linear_complexity() {
    use rumdl_lib::inline_config::InlineConfig;
    // One directive, then many openers on one line whose closer is on a later
    // line: each opener is checked for code without rescanning its line.
    let durations: Vec<_> = [8192, 16384, 32768]
        .into_iter()
        .map(|size| {
            let content = format!("<!-- rumdl-disable MD013 -->\n{}\n-->\n", "<!-- plain ".repeat(size));
            let _ = InlineConfig::from_content(&content);
            let mut times: Vec<Duration> = (0..3)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let _ = std::hint::black_box(InlineConfig::from_content(std::hint::black_box(&content)));
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[1]
        })
        .collect();
    assert_linear_complexity("InlineConfig::from_content (crowded openers)", &durations, 3.0);
}

#[test]
fn test_apply_warning_fixes_linear_complexity() {
    // One insertion per line, the shape of a rule flagging every list in a
    // long document. Shifting the rest of the buffer per edit makes doubling
    // the input cost close to 4x; assembling the output once stays near 2x.
    let sizes = [20000, 40000, 80000];
    let iterations = 3;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content: String = (0..size).map(|i| format!("text {i}\n- item\n")).collect();
            let mut offset = 0;
            let warnings: Vec<LintWarning> = content
                .split_inclusive('\n')
                .filter_map(|line| {
                    let start = offset;
                    offset += line.len();
                    line.starts_with("- ").then(|| LintWarning {
                        message: "List should be preceded by blank line".to_string(),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        severity: Severity::Warning,
                        fix: Some(Fix::new(start..start, "\n".to_string())),
                        rule_name: Some("MD032".to_string()),
                    })
                })
                .collect();
            let expected_len = content.len() + warnings.len();
            assert_eq!(apply_warning_fixes(&content, &warnings).unwrap().len(), expected_len);

            let mut times: Vec<Duration> = (0..iterations)
                .map(|_| {
                    let start = MeasurementClock::now();
                    let _ = std::hint::black_box(apply_warning_fixes(&content, &warnings));
                    start.elapsed()
                })
                .collect();
            times.sort();
            times[iterations / 2]
        })
        .collect();

    assert_linear_complexity("apply_warning_fixes", &durations, 3.0);
}

// =============================================================================
// Issue #148 Regression Anchor
// =============================================================================

#[test]
fn test_issue_148_exact_pattern_linear_complexity() {
    // This test uses the EXACT document structure from issue #148
    // If this fails, we've reintroduced the O(n²) regression

    let sizes = [300, 600, 1200]; // Matches the ~890 row original report
    let iterations = 5;
    let rule = MD007ULIndent::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_issue_148_document(size);
            println!(
                "Testing issue #148 pattern with {size} entries ({} bytes)",
                content.len()
            );
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    // Extra strict threshold for this specific regression
    assert_linear_complexity("MD007 (issue #148 pattern)", &durations, 4.0);
}

// =============================================================================
// List Rules (MD004, MD005, MD007, MD030, MD032)
// =============================================================================

#[test]
fn test_md004_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD004UnorderedListStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD004", &durations, 6.0);
}

#[test]
fn test_md005_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD005ListIndent::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD005", &durations, 6.0);
}

#[test]
fn test_md007_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD007ULIndent::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD007", &durations, 6.0);
}

#[test]
fn test_md030_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD030ListMarkerSpace::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD030", &durations, 6.0);
}

#[test]
fn test_md032_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD032BlanksAroundLists::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_list_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD032", &durations, 6.0);
}

#[test]
fn test_md032_many_lists_linear_complexity() {
    // Many short lists between paragraphs. Checking every line against every
    // list block grows with lines times lists: doubling the input then costs
    // close to 4x, while a lookup per line stays near 2x.
    let sizes = [2000, 4000, 8000];
    let iterations = 3;
    let rule = MD032BlanksAroundLists::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content: String = (0..size)
                .map(|i| format!("text {i}\n- a\n- b\n  - c\ntext\n\n"))
                .collect();
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD032 (many lists)", &durations, 3.0);
}

// =============================================================================
// Heading Rules (MD001, MD003, MD018-MD023, MD025, MD026)
// =============================================================================

#[test]
fn test_md001_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD001HeadingIncrement::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD001", &durations, 6.0);
}

#[test]
fn test_md003_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD003HeadingStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD003", &durations, 6.0);
}

#[test]
fn test_md018_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD018NoMissingSpaceAtx::new();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD018", &durations, 6.0);
}

#[test]
fn test_md019_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD019NoMultipleSpaceAtx;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD019", &durations, 6.0);
}

#[test]
fn test_md020_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD020NoMissingSpaceClosedAtx;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD020", &durations, 6.0);
}

#[test]
fn test_md022_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD022BlanksAroundHeadings::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD022", &durations, 6.0);
}

#[test]
fn test_md025_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD025SingleTitle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD025", &durations, 6.0);
}

#[test]
fn test_md026_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD026NoTrailingPunctuation::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_heading_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD026", &durations, 6.0);
}

// =============================================================================
// Text Rules (MD013, MD036, MD037, MD049, MD050, MD064)
// =============================================================================

#[test]
fn test_md013_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD013LineLength::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD013", &durations, 6.0);
}

/// `num_items` quoted list items, each long enough to be rewrapped, under an
/// intro holding a code span and a display math pair so the scan for code
/// spans crossing a line boundary runs for the document.
fn generate_quoted_list_under_code_span(num_items: usize) -> String {
    let item = "> - This quoted list item runs on far past the configured width so that the reflow has to rewrap it into shorter lines.\n";
    let mut content = String::with_capacity(48 + num_items * item.len());
    content.push_str("Intro with `code` and a math pair.\n\n$$ x $$\n\n");
    for _ in 0..num_items {
        content.push_str(item);
    }
    content
}

/// Whether a `$$...$$` line inside a quoted list item is a display block is
/// read off one pass over the document that every item shares. A pass per
/// item would read the whole document again for each item, so a quoted list
/// grows about linearly only while the flags are computed once.
#[test]
fn test_md013_quoted_list_code_span_flags_linear_complexity() {
    let sizes = [100, 200, 400];
    let iterations = 3;
    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::new(40),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    });

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_quoted_list_under_code_span(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    // A pass per item costs four times as much per doubling; one shared pass
    // costs about twice, and three separates the two with room for noise.
    assert_linear_complexity("MD013 quoted list code span flags", &durations, 3.0);
}

/// One line of `num_sentences` strong sentences, the shape a sentence per line
/// reflow cuts once per sentence with a delimiter run against every cut.
fn generate_strong_sentence_line(num_sentences: usize) -> String {
    let mut content = String::with_capacity(num_sentences * 20);
    for _ in 0..num_sentences {
        content.push_str("**Sentence ends.** ");
    }
    content.push('\n');
    content
}

/// The sentence per line reflow reads a whole paragraph once and decides each
/// cut from that reading. A cut touching a delimiter run asks whether the break
/// changes what the run does, and a paragraph of strong sentences puts a run
/// against every cut, so the check grows about linearly only when that question
/// is answered without reading the paragraph again per cut.
#[test]
fn test_md013_sentence_per_line_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::new(0),
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    });

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_strong_sentence_line(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    // A reading of the paragraph per cut costs four times as much per
    // doubling, which the shared threshold lets through; the linear path
    // costs about twice, and three separates the two with room for noise.
    assert_linear_complexity("MD013 sentence per line", &durations, 3.0);
}

/// One line of `num_sentences` strong CJK sentences written with nothing between
/// them, the shape a sentence per line reflow cuts once per sentence where a
/// delimiter run touches every cut and a fullwidth bracket sits in front of it,
/// which the flanking rules alone cannot settle.
fn generate_strong_cjk_sentence_line(num_sentences: usize) -> String {
    let mut content = String::with_capacity(num_sentences * 20);
    for _ in 0..num_sentences {
        content.push_str("**（完成。）**");
    }
    content.push('\n');
    content
}

/// A cut the flanking rules cannot settle is confirmed by a parse of the text
/// carrying the break. A paragraph of strong CJK sentences puts such a cut
/// after every sentence, so the check grows about in step with the input only
/// when those cuts are confirmed together rather than by a parse each.
#[test]
fn test_md013_sentence_per_line_cjk_strong_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::new(0),
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    });

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_strong_cjk_sentence_line(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    // A parse per cut costs four times as much per doubling; one parse for all
    // of them costs about twice, and three separates the two with room for
    // noise.
    assert_linear_complexity("MD013 sentence per line, strong CJK", &durations, 3.0);
}

/// One paragraph of `num_lines` lines of about a hundred bytes each ending in
/// one trailing space, the soft break a renderer shows as one space. The join
/// that replaces each break trims that space, so the paragraph puts a trim in
/// front of every join. The second line opens a definition, so the check joins
/// the lines and then leaves the paragraph alone: the join is where the check
/// spends its time, where the reflow of an ordinary paragraph costs about a
/// microsecond a byte in a test build and buries the join's growth at any size
/// a test can afford.
fn generate_soft_break_definition_paragraph(num_lines: usize) -> String {
    let line = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore \n";
    let mut content = String::with_capacity(num_lines * (line.len() + 2));
    for idx in 0..num_lines {
        if idx == 1 {
            content.push_str(": ");
        }
        content.push_str(line);
    }
    content
}

/// The join that replaces a paragraph's soft breaks trims the spaces before
/// each break outside a code span. A paragraph whose every line ends in one
/// space puts a trim in front of every join, so the check grows about linearly
/// only when the trimmed text is written in one pass rather than shifted once
/// per join.
#[test]
fn test_md013_soft_break_join_linear_complexity() {
    let sizes = [10000, 20000, 40000];
    let iterations = 5;
    let rule = MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::new(0),
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    });

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_soft_break_definition_paragraph(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    // A shift of the text per join costs four times as much per doubling; one
    // pass over the text costs about twice, and three separates the two with
    // room for noise.
    assert_linear_complexity("MD013 soft break join", &durations, 3.0);
}

#[test]
fn test_md036_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD036NoEmphasisAsHeading::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD036", &durations, 6.0);
}

#[test]
fn test_md037_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD037NoSpaceInEmphasis;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD037", &durations, 6.0);
}

#[test]
fn test_md049_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD049EmphasisStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD049", &durations, 6.0);
}

#[test]
fn test_md050_long_line_linear_complexity() {
    let rule = MD050StrongStyle::default();
    for (unit, flavor) in [
        ("é日本🙂 **good** __other__ ", MarkdownFlavor::Standard),
        ("`**code**` :smile: $x$ **good** __other__ ", MarkdownFlavor::Standard),
        (
            "++ctrl+alt++ ^^insert^^ ~sub~ ==mark== :smile: `**code**` **good** __other__ ",
            MarkdownFlavor::MkDocs,
        ),
    ] {
        let durations: Vec<_> = [2048, 4096, 8192]
            .into_iter()
            .map(|size| measure_rule_time_for_flavor(&rule, &format!("{}\n", unit.repeat(size)), flavor, 5))
            .collect();
        assert_linear_complexity("MD050 long line", &durations, 3.0);
    }
}

#[test]
fn test_md050_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD050StrongStyle::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_paragraph_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD050", &durations, 6.0);
}

// =============================================================================
// Link Rules (MD034, MD039, MD042, MD051-MD054, MD059)
// =============================================================================

#[test]
fn test_md054_long_line_linear_complexity() {
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);
    for unit in [
        "[label](target) ![alt](image.png) ",
        "é日本🙂 [标签](target) ![图](image.png) ",
    ] {
        let durations: Vec<_> = [4096, 8192, 16384]
            .into_iter()
            .map(|size| measure_rule_time(&rule, &format!("{}\n", unit.repeat(size)), 5))
            .collect();
        assert_linear_complexity("MD054 long line", &durations, 3.0);
    }
}

#[test]
fn test_md054_fix_planning_linear_complexity() {
    let rule = MD054LinkImageStyle::new(true, true, true, false, true, true);
    for unit in [
        "[label](https://example.com)\n",
        "[![alt](image.png)](https://example.com) [plain](https://example.com)\n",
    ] {
        let durations: Vec<_> = [4096, 8192, 16384]
            .into_iter()
            .map(|size| measure_rule_time(&rule, &unit.repeat(size), 5))
            .collect();
        assert_linear_complexity("MD054 fix planning", &durations, 3.0);
    }
}

#[test]
fn test_md034_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD034NoBareUrls;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD034", &durations, 6.0);
}

#[test]
fn test_md034_interleaved_code_links_and_bare_urls_linear_complexity() {
    let rule = MD034NoBareUrls;
    let durations: Vec<_> = [2000, 4000, 8000]
        .into_iter()
        .map(|size| {
            let mut content = String::from("# URLs\n\n");
            for i in 0..size {
                content.push_str(&format!(
                    "`sample_{i}` [linked](https://example.com/linked/{i}) https://example.com/bare/{i}\n\n"
                ));
            }
            let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), size);
            measure_rule_time(&rule, &content, 5)
        })
        .collect();

    // Each bare URL must query the code spans and links without scanning the
    // whole document's records. A repeated full scan approaches 4x per doubling.
    assert_linear_complexity("MD034 interleaved code and links", &durations, 3.0);
}

#[test]
fn test_md034_long_line_linear_complexity() {
    let rule = MD034NoBareUrls;
    for (name, unit, warnings_per_unit) in [
        ("formatted", "[link](https://example.com) <hello@example.com> ", 0),
        ("bare Unicode", "é🦀 https://example.com hello@example.com ", 2),
    ] {
        let durations: Vec<_> = [4096, 8192, 16384]
            .into_iter()
            .map(|size| {
                let content = format!("{}\n", unit.repeat(size));
                let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
                assert_eq!(rule.check(&ctx).unwrap().len(), size * warnings_per_unit);
                measure_rule_time(&rule, &content, 5)
            })
            .collect();
        // Doubling a paragraph must not quadruple construct lookup or column
        // counting work. Cover the two costs separately to catch either one.
        assert_linear_complexity(&format!("MD034 long line ({name})"), &durations, 3.0);
    }
}

#[test]
fn test_md039_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD039NoSpaceInLinks;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD039", &durations, 6.0);
}

#[test]
fn test_md042_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD042NoEmptyLinks::new();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD042", &durations, 6.0);
}

#[test]
fn test_md051_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD051LinkFragments::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD051", &durations, 6.0);
}

#[test]
fn test_md053_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD053LinkImageReferenceDefinitions::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_link_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD053", &durations, 6.0);
}

// =============================================================================
// Code Rules (MD014, MD031, MD038, MD040, MD046, MD048)
// =============================================================================

#[test]
fn test_md014_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD014CommandsShowOutput::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD014", &durations, 6.0);
}

#[test]
fn test_md031_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD031BlanksAroundFences::new(true);

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD031", &durations, 6.0);
}

#[test]
fn test_md038_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD038NoSpaceInCode::new();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD038", &durations, 6.0);
}

#[test]
fn test_md038_same_line_spans_linear_complexity() {
    let sizes = [250, 500, 1000];
    let iterations = 3;
    let rule = MD038NoSpaceInCode::new();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_single_line_code_spans(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD038 (same-line spans)", &durations, 6.0);
}

#[test]
fn test_md040_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD040FencedCodeLanguage::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD040", &durations, 6.0);
}

#[test]
fn test_md046_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD046CodeBlockStyle::new(CodeBlockStyle::Consistent);

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD046", &durations, 6.0);
}

#[test]
fn test_md048_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD048CodeFenceStyle::new(CodeFenceStyle::Consistent);

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_code_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD048", &durations, 6.0);
}

// =============================================================================
// Structural Rules (MD012, MD041, MD047)
// =============================================================================

#[test]
fn test_md012_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD012NoMultipleBlanks::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD012", &durations, 6.0);
}

#[test]
fn test_md041_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD041FirstLineHeading::default();

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD041", &durations, 6.0);
}

#[test]
fn test_md047_linear_complexity() {
    let sizes = [500, 1000, 2000];
    let iterations = 5;
    let rule = MD047SingleTrailingNewline;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let content = generate_mixed_document(size);
            measure_rule_time(&rule, &content, iterations)
        })
        .collect();

    assert_linear_complexity("MD047", &durations, 6.0);
}

/// Generate a monotonically deepening list: every line is a list item indented
/// one level more than the last. This is the adversarial shape for MD077, whose
/// per-item continuation walk once re-scanned the entire tail for every item
/// (O(n^2) in depth; ~13s at depth 2000 before the fix).
fn generate_deeply_nested_list(depth: usize) -> String {
    let mut content = String::with_capacity(depth * depth / 8 + 16);
    content.push_str("# Deep\n\n");
    for i in 0..depth {
        for _ in 0..i {
            content.push_str("  ");
        }
        content.push_str("- item\n");
    }
    content
}

#[test]
fn test_md077_deeply_nested_linear_complexity() {
    // Depth doubles each step; a linear rule's time also roughly doubles. The
    // pre-fix O(n^2) walk showed ~4x+ per doubling. Test both continuation
    // styles - "aligned" additionally runs the latent-structure scan, which the
    // same guard must also short-circuit.
    let sizes = [500, 1000, 2000];
    let iterations = 3;

    for style in [ContinuationStyle::Any, ContinuationStyle::Aligned] {
        let rule = MD077ListContinuationIndent::new(style);
        let durations: Vec<_> = sizes
            .iter()
            .map(|&size| {
                let content = generate_deeply_nested_list(size);
                measure_rule_time(&rule, &content, iterations)
            })
            .collect();
        assert_linear_complexity(&format!("MD077 ({style:?})"), &durations, 6.0);
    }
}

/// Generate a single inline link whose text and destination each hold one long
/// run of backslashes. This is the adversarial shape for any escape check that
/// counts the preceding backslashes per character, since every position in the
/// run re-walks the whole run.
fn generate_backslash_run_link(run_length: usize) -> String {
    let mut content = String::with_capacity(run_length * 2 + 64);
    content.push_str("# Escapes\n\n[");
    // An even run leaves the following character unescaped, so the link and its
    // destination still close.
    for _ in 0..run_length / 2 {
        content.push_str("\\\\");
    }
    content.push_str("](");
    for _ in 0..run_length / 2 {
        content.push_str("\\\\");
    }
    content.push_str("x)\n");
    content
}

#[test]
fn test_backslash_run_link_linear_complexity() {
    // The run length doubles each step, so a linear scan roughly doubles in
    // time while a per-character backwards walk quadruples. The runs stay
    // modest because a regression here makes every measurement quadratic, and
    // the guard has to report that in reasonable time.
    let sizes = [20_000, 40_000, 80_000];
    let iterations = 3;
    let rule = MD062LinkDestinationWhitespace::new();

    let context_durations: Vec<_> = sizes
        .iter()
        .map(|&size| measure_context_time(&generate_backslash_run_link(size), iterations))
        .collect();
    assert_linear_complexity("LintContext::new (backslash run)", &context_durations, 3.0);

    let rule_durations: Vec<_> = sizes
        .iter()
        .map(|&size| measure_rule_time(&rule, &generate_backslash_run_link(size), iterations))
        .collect();
    assert_linear_complexity("MD062 (backslash run)", &rule_durations, 3.0);
}

/// Fill `directory` with `count` markdown documents, each linking to two
/// existing siblings, and return their paths. This is the adversarial shape for
/// MD057's directory listings: every document's links land in the one directory
/// that holds all of them, so a listing read per document is a read of `count`
/// entries per document.
fn write_flat_link_directory(directory: &Path, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|i| {
            let path = directory.join(format!("doc{i}.md"));
            let first = (i + 1) % count;
            let second = (i + 2) % count;
            std::fs::write(
                &path,
                format!("# Doc {i}\n\nSee [first](doc{first}.md) and [second](doc{second}.md).\n"),
            )
            .unwrap();
            path
        })
        .collect()
}

/// Time one pass of MD057 over every document in a flat directory, taking the
/// median of `iterations` passes after a discarded warm-up.
fn measure_flat_directory_time(count: usize, iterations: usize) -> Duration {
    let directory = tempfile::tempdir().unwrap();
    let paths = write_flat_link_directory(directory.path(), count);
    let contents: Vec<String> = paths
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect();
    let contexts: Vec<LintContext> = contents
        .iter()
        .zip(&paths)
        .map(|(content, path)| LintContext::new(content, MarkdownFlavor::Standard, Some(path.clone())))
        .collect();
    let rule = MD057ExistingRelativeLinks::new().with_path(directory.path());

    let run = || {
        for ctx in &contexts {
            let _ = rule.check(ctx);
        }
    };

    run();

    let mut times: Vec<Duration> = (0..iterations)
        .map(|_| {
            let start = MeasurementClock::now();
            run();
            start.elapsed()
        })
        .collect();

    times.sort();
    times[iterations / 2]
}

#[test]
fn test_md057_flat_directory_linear_complexity() {
    // The document count doubles each step. Reading the directory once per
    // document costs one listing of N entries N times, which is quadratic and
    // shows about 4x per doubling; a listing that outlives the document costs
    // one stat per document and roughly doubles. The threshold is tighter than
    // the 6x most rules here use, because 4x sits under 6x and this guard has
    // to separate the two.
    let sizes = [500, 1000, 2000];
    let iterations = 3;

    let durations: Vec<_> = sizes
        .iter()
        .map(|&size| measure_flat_directory_time(size, iterations))
        .collect();

    for (size, duration) in sizes.iter().zip(&durations) {
        println!("MD057 flat directory of {size} documents: {duration:?}");
    }

    assert_linear_complexity("MD057 (flat directory)", &durations, 3.0);
}

/// One parent with many children: every child is checked for being continuation
/// content of the parent, which must not rescan the parent's earlier children.
#[test]
fn test_md005_wide_sibling_list_linear_complexity() {
    let rule = MD005ListIndent::default();
    let durations: Vec<_> = [1000, 2000, 4000]
        .iter()
        .map(|&size| {
            let mut content = String::from("- parent\n");
            for i in 0..size {
                content.push_str(&format!("  - child {i}\n"));
            }
            measure_rule_time(&rule, &content, 5)
        })
        .collect();

    assert_linear_complexity("MD005 wide sibling list", &durations, 3.0);
}

/// Opening tags with no closing tag: pairing each one with its closing tag must
/// not search the rest of the document per tag, with or without fixes enabled.
#[test]
fn test_md033_unclosed_tags_linear_complexity() {
    for rule in [MD033NoInlineHtml::default(), MD033NoInlineHtml::with_fix(true)] {
        let durations: Vec<_> = [2000, 4000, 8000]
            .iter()
            .map(|&size| measure_rule_time(&rule, &"text <span>x\n".repeat(size), 5))
            .collect();

        assert_linear_complexity("MD033 unclosed tags", &durations, 3.0);
    }
}

#[test]
fn table_confirmation_linear_complexity() {
    // Lists interleaved with tables: confirming each table must not walk every
    // earlier list item.
    let build = |count: usize| "- x\n\n|a|b|\n|-|-|\n\n".repeat(count);
    let measure = |count: usize| {
        let content = build(count);
        let start = std::time::Instant::now();
        let ctx =
            rumdl_lib::lint_context::LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(ctx.table_blocks.len(), count);
        start.elapsed().as_secs_f64()
    };
    let small = measure(1000);
    let large = measure(8000);
    // Linear growth is 8x; quadratic is 64x.
    assert!(
        large / small < 24.0,
        "confirming 8000 tables took {:.1}x the time of 1000",
        large / small
    );
}

#[test]
fn carved_table_confirmation_linear_complexity() {
    // Many tables inside one admonition: confirming each must not rescan the
    // container from its opener.
    let measure = |count: usize| {
        let content = format!("!!! note\n\n{}", "    |a|b|\n    |-|-|\n\n".repeat(count));
        let start = std::time::Instant::now();
        let ctx = rumdl_lib::lint_context::LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
        assert_eq!(ctx.table_blocks.len(), count);
        start.elapsed().as_secs_f64()
    };
    let small = measure(1000);
    let large = measure(8000);
    assert!(
        large / small < 24.0,
        "confirming 8000 tables took {:.1}x the time of 1000",
        large / small
    );
}

#[test]
fn flavor_masked_header_confirmation_linear_complexity() {
    // A MkDocs header holding a code span full of pipes: the pipes the flavor
    // masks must be looked up without scanning all the others.
    let measure = |pipes: usize| {
        let content = format!("a | `{}`\n-|-\n", "x|".repeat(pipes));
        let start = std::time::Instant::now();
        let ctx = rumdl_lib::lint_context::LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
        assert_eq!(ctx.table_blocks.len(), 1);
        start.elapsed().as_secs_f64()
    };
    let small = measure(4000);
    let large = measure(32000);
    assert!(
        large / small < 24.0,
        "confirming 32000 masked pipes took {:.1}x the time of 4000",
        large / small
    );
}

#[test]
fn rejected_definition_table_confirmation_linear_complexity() {
    // Many rejected colon definitions beside many tables: each lookup must not
    // scan every definition.
    let measure = |count: usize| {
        let content = "term\n:warning:\n\n|a|b|\n|-|-|\n\n".repeat(count);
        let start = std::time::Instant::now();
        let ctx =
            rumdl_lib::lint_context::LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(ctx.table_blocks.len(), count);
        start.elapsed().as_secs_f64()
    };
    let small = measure(4000);
    let large = measure(32000);
    assert!(
        large / small < 16.0,
        "confirming 32000 tables took {:.1}x the time of 4000",
        large / small
    );
}

#[test]
fn rejected_definition_run_confirmation_linear_complexity() {
    // One rejected definition, then many tables in a single unbroken run of
    // non-blank lines: each table must not rescan the run above it.
    let measure = |count: usize| {
        let content = format!("term\n:warning:\n\n{}", "# s\n|a|b|\n|-|-|\n".repeat(count));
        let start = std::time::Instant::now();
        let ctx =
            rumdl_lib::lint_context::LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(ctx.table_blocks.len(), count);
        start.elapsed().as_secs_f64()
    };
    let small = measure(1000);
    let large = measure(8000);
    assert!(
        large / small < 16.0,
        "confirming 8000 tables took {:.1}x the time of 1000",
        large / small
    );
}

#[test]
fn nested_list_table_confirmation_linear_complexity() {
    // An outer item with many closed child items, then many tables at its own
    // indentation: finding the outer item must not walk the closed children.
    let measure = |count: usize| {
        let content = format!(
            "- outer\n{}{}",
            "  - c\n".repeat(count),
            "\n  |a|b|\n  |-|-|\n".repeat(count)
        );
        let start = std::time::Instant::now();
        let ctx =
            rumdl_lib::lint_context::LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(!ctx.table_blocks.is_empty());
        start.elapsed().as_secs_f64()
    };
    let small = measure(2000);
    let large = measure(16000);
    assert!(
        large / small < 16.0,
        "confirming 16000 tables took {:.1}x the time of 2000",
        large / small
    );
}

/// Native MDX facts must bypass redundant primary-parser image label scans.
#[test]
fn test_mdx_nested_image_labels_linear_complexity() {
    let times: Vec<_> = [1024, 2048, 4096]
        .into_iter()
        .map(|count| {
            let content = format!("{}x{}", "![".repeat(count), "](image.png)".repeat(count));
            let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
            assert_eq!(ctx.images().len(), 1);
            let mut samples: Vec<_> = (0..7)
                .map(|_| {
                    let start = MeasurementClock::now();
                    std::hint::black_box(LintContext::new(&content, MarkdownFlavor::MDX, None));
                    start.elapsed()
                })
                .collect();
            samples.sort();
            samples[3]
        })
        .collect();
    assert_linear_complexity("MDX nested image labels", &times, 3.0);
}
