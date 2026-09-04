//! Every part of a run measures against the same line length.
//!
//! `line-length` is spelled twice, once in `[global]` and once as an MD013
//! option, and MD013 measures against the global setting when its own option is
//! unset. Four places load MD013's limit and three of them used to skip that
//! fallback, so a global `line-length = 120` wrapped paragraphs at 120 while
//! MD060 and MD075 rebuilt tables to fit 80 and `rumdl config` answered 80.
//!
//! MD060 auto-compacts a table whose aligned form is wider than the limit, so
//! the formatted table width reports which limit the rule used. These tests read
//! it back through the CLI rather than through the rule, because the defect was
//! that each caller loaded the configuration its own way.

use std::process::Command;

/// A ragged table whose aligned form is 94 columns wide: wider than MD013's own
/// default of 80, narrower than the 120 the fixtures below set globally. The
/// last row is separated by a blank line, which is the orphan MD075 merges back.
const TABLE: &str = "\
# T

| Alpha column heading here 12 | Beta column heading here 123 | Gamma column heading here 12 |
|---|---|---|
| a | b | c |

| d | e | f |
";

/// The width of the table's aligned form, which is what makes it exceed 80.
const ALIGNED_WIDTH: usize = 94;

/// Format `TABLE` under `config` with only `rule` enabled, and report the width
/// of the delimiter row.
///
/// The delimiter row, rather than the widest or narrowest line, because neither
/// of those answers the question: the header row is already `ALIGNED_WIDTH`
/// columns in the source and keeps that width under either branch, and the
/// orphaned row is not a table of its own, so a run with only MD060 enabled
/// leaves it short whatever the limit was. The delimiter row is written `|---|`
/// in the source and reaches `ALIGNED_WIDTH` only by being aligned.
fn delimiter_row_width(config: &str, rule: &str) -> usize {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join(".rumdl.toml"), config).expect("failed to write config");
    let file = dir.path().join("t.md");
    std::fs::write(&file, TABLE).expect("failed to write document");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "--enable", rule, "t.md"])
        .current_dir(dir.path())
        .env("NO_COLOR", "1")
        .env("RUMDL_CACHE_DIR", dir.path().join(".cache"))
        .output()
        .expect("failed to run rumdl fmt");
    assert!(
        output.status.success(),
        "rumdl fmt --enable {rule} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let formatted = std::fs::read_to_string(&file).expect("failed to read back document");
    let delimiters: Vec<&str> = formatted
        .lines()
        .filter(|line| line.starts_with('|') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')))
        .collect();
    assert_eq!(
        delimiters.len(),
        1,
        "expected exactly one delimiter row to measure.\n\n--- output ---\n{formatted}"
    );
    delimiters[0].chars().count()
}

/// Two settings are in every fixture because without them the limit is
/// unobservable rather than unused:
///
/// - `[MD013] tables = true`, because MD013 leaves tables unmeasured by default
///   and MD060 reads that as "no limit at all".
/// - `[MD060] style = "aligned"`, because MD060's default style never builds the
///   aligned form whose width the limit decides. (MD075 normalizes the default to
///   `aligned` itself, so this only matters for MD060.)
fn config(global: &str, md013: &str) -> String {
    format!("[global]\n{global}\n\n[MD013]\ntables = true\n{md013}\n\n[MD060]\nstyle = \"aligned\"\n")
}

/// The reported cases: a `[global] line-length` MD013 does not override is the
/// limit tables are sized against.
#[test]
fn test_tables_are_sized_against_the_global_line_length() {
    for rule in ["MD060", "MD075"] {
        let width = delimiter_row_width(&config("line-length = 120", ""), rule);
        assert_eq!(
            width, ALIGNED_WIDTH,
            "{rule} compacted a {ALIGNED_WIDTH}-column table under a global line-length of 120, \
             so it sized the table against MD013's default of 80 rather than the configured limit."
        );
    }
}

/// The same construct under a limit the aligned table really does exceed. Without
/// this the test above passes for a rule that simply stopped compacting.
#[test]
fn test_tables_are_still_compacted_when_they_exceed_the_global_line_length() {
    for rule in ["MD060", "MD075"] {
        let width = delimiter_row_width(&config("line-length = 90", ""), rule);
        assert!(
            width < ALIGNED_WIDTH,
            "{rule} left a {ALIGNED_WIDTH}-column table aligned under a global line-length of 90; \
             the limit is inherited but no longer enforced. Delimiter row: {width}"
        );
    }
}

/// With nothing to inherit, MD013's own default still applies.
#[test]
fn test_tables_fall_back_to_md013s_own_default() {
    for rule in ["MD060", "MD075"] {
        let width = delimiter_row_width(&config("disable = []", ""), rule);
        assert!(
            width < ALIGNED_WIDTH,
            "{rule} left a {ALIGNED_WIDTH}-column table aligned with no line length configured \
             anywhere, so it stopped applying MD013's default of 80. Delimiter row: {width}"
        );
    }
}

/// MD013's own option still outranks the global one. The fallback fills an unset
/// option; it does not let the global setting override a rule that sets its own.
#[test]
fn test_md013s_own_line_length_outranks_the_global_one() {
    for rule in ["MD060", "MD075"] {
        let width = delimiter_row_width(&config("line-length = 120", "line-length = 85"), rule);
        assert!(
            width < ALIGNED_WIDTH,
            "{rule} sized a table against the global line-length of 120 while MD013 set its own \
             limit of 85. Delimiter row: {width}"
        );
    }
}

/// The path that never went through the global setting, as a positive control:
/// these rules can produce an aligned table wider than 80, so the assertions
/// above are reporting the limit rather than an alignment that never happens.
#[test]
fn test_md013s_own_line_length_reaches_the_table_rules() {
    for rule in ["MD060", "MD075"] {
        let width = delimiter_row_width(&config("disable = []", "line-length = 120"), rule);
        assert_eq!(
            width, ALIGNED_WIDTH,
            "{rule} compacted a {ALIGNED_WIDTH}-column table under MD013's own line-length of 120."
        );
    }
}

/// MD075 merges the orphaned row before formatting it, so the width assertions
/// above describe a table the rule actually rebuilt.
#[test]
fn test_md075_merges_the_orphaned_row_it_formats() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join(".rumdl.toml"), config("line-length = 120", "")).expect("failed to write config");
    let file = dir.path().join("t.md");
    std::fs::write(&file, TABLE).expect("failed to write document");

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["fmt", "--no-cache", "--enable", "MD075", "t.md"])
        .current_dir(dir.path())
        .env("NO_COLOR", "1")
        .env("RUMDL_CACHE_DIR", dir.path().join(".cache"))
        .output()
        .expect("failed to run rumdl fmt");
    assert!(output.status.success());

    let formatted = std::fs::read_to_string(&file).expect("failed to read back document");
    let rows: Vec<&str> = formatted.lines().filter(|line| line.starts_with('|')).collect();
    assert_eq!(
        rows.len(),
        4,
        "MD075 did not merge the orphaned row back into the table, so the formatted width says \
         nothing about the limit it used.\n\n--- output ---\n{formatted}"
    );
    assert!(
        rows[3].starts_with("| d"),
        "the merged row is not the orphan.\n\n--- output ---\n{formatted}"
    );
}
