use crate::rule_config_serde::RuleConfig;
use crate::types::{OlAlignColumn, PositiveUsize};
use serde::{Deserialize, Serialize};

/// Configuration for MD030 (Spaces after list markers).
///
/// Public so other rules (notably MD013's reflow) can load it through the shared
/// [`crate::rule_config_serde::load_rule_config`] path and reuse
/// [`MD030Config::expected_spaces`], rather than re-deriving the spacing rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD030Config {
    /// Spaces for single-line unordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ul_single")]
    pub ul_single: PositiveUsize,

    /// Spaces for multi-line unordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ul_multi")]
    pub ul_multi: PositiveUsize,

    /// Spaces for single-line ordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ol_single")]
    pub ol_single: PositiveUsize,

    /// Spaces for multi-line ordered list items (default: 1)
    #[serde(default = "default_spaces", alias = "ol_multi")]
    pub ol_multi: PositiveUsize,

    /// Align ordered list text to this column, measured from the start of the
    /// marker (default: 0 = off; valid values are 3-6, with 4 the usual choice).
    /// Narrower markers are padded up to the column; markers too wide for it
    /// overflow with one space rather than pushing the rest of the list over.
    /// See docs/md030.md.
    #[serde(default, alias = "ol_align_column")]
    pub ol_align_column: OlAlignColumn,
}

fn default_spaces() -> PositiveUsize {
    PositiveUsize::from_const(1)
}

impl Default for MD030Config {
    fn default() -> Self {
        Self {
            ul_single: default_spaces(),
            ul_multi: default_spaces(),
            ol_single: default_spaces(),
            ol_multi: default_spaces(),
            ol_align_column: OlAlignColumn::default(),
        }
    }
}

impl MD030Config {
    /// Number of spaces that should follow a list marker, given whether the list
    /// is ordered, whether the item spans multiple lines, and the marker width
    /// (e.g. 1 for `-`, 2 for `1.`).
    ///
    /// The `ol-align-column` override takes precedence for ordered lists: it
    /// aligns text to the target column measured from the marker start, padding a
    /// narrow marker and letting one too wide overflow with a single space, capped
    /// at 4 spaces (5+ would start a CommonMark indented code block). Otherwise the
    /// fixed `ul-single`/`ul-multi`/`ol-single`/`ol-multi` value applies.
    pub fn expected_spaces(&self, is_ordered: bool, is_multi: bool, marker_len: usize) -> usize {
        if is_ordered && let Some(target_column) = self.ol_align_column.enabled() {
            return target_column.saturating_sub(marker_len).clamp(1, 4);
        }
        match (is_ordered, is_multi) {
            (false, false) => self.ul_single.get(),
            (false, true) => self.ul_multi.get(),
            (true, false) => self.ol_single.get(),
            (true, true) => self.ol_multi.get(),
        }
    }
}

impl RuleConfig for MD030Config {
    const RULE_NAME: &'static str = "MD030";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            ul_single = 2
            ol_single = 3
            ul_multi = 4
            ol_multi = 5
        "#;
        let config: MD030Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ul_single.get(), 2);
        assert_eq!(config.ol_single.get(), 3);
        assert_eq!(config.ul_multi.get(), 4);
        assert_eq!(config.ol_multi.get(), 5);
    }

    #[test]
    fn test_kebab_case_canonical_format() {
        let toml_str = r#"
            ul-single = 2
            ol-single = 3
            ul-multi = 4
            ol-multi = 5
        "#;
        let config: MD030Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ul_single.get(), 2);
        assert_eq!(config.ol_single.get(), 3);
        assert_eq!(config.ul_multi.get(), 4);
        assert_eq!(config.ol_multi.get(), 5);
    }

    #[test]
    fn test_ol_align_column_defaults_to_zero() {
        let config = MD030Config::default();
        assert_eq!(
            config.ol_align_column.get(),
            0,
            "ol-align-column should default to 0 (off)"
        );
    }

    #[test]
    fn test_ol_align_column_kebab_and_snake_case() {
        let kebab: MD030Config = toml::from_str("ol-align-column = 4").unwrap();
        assert_eq!(kebab.ol_align_column.get(), 4);

        let snake: MD030Config = toml::from_str("ol_align_column = 4").unwrap();
        assert_eq!(snake.ol_align_column.get(), 4);
    }

    #[test]
    fn test_ol_align_column_accepts_off_and_valid_range() {
        for column in [0, 3, 4, 5, 6] {
            let config: MD030Config = toml::from_str(&format!("ol-align-column = {column}")).unwrap();
            assert_eq!(config.ol_align_column.get(), column);
        }
    }

    #[test]
    fn test_ol_align_column_rejects_out_of_range() {
        // 1 and 2 are below the narrowest marker's reach; 7+ would need 5+ spaces,
        // which CommonMark parses as an indented code block. All are rejected.
        for column in [1, 2, 7, 8, 100] {
            let result: Result<MD030Config, _> = toml::from_str(&format!("ol-align-column = {column}"));
            assert!(result.is_err(), "ol-align-column = {column} should be rejected");
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("must be 0 (off) or between 3 and 6") || err.contains(&format!("got {column}")),
                "unexpected error for {column}: {err}"
            );
        }
    }
}
