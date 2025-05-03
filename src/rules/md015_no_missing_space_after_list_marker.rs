/// Rule MD015: No missing space after list marker
///
/// See [docs/md015.md](../../docs/md015.md) for full documentation, configuration, and examples.
///
/// NOTE: The AST is not used for detection/fixing in this rule because the CommonMark parser only recognizes list items that already have a space after the marker. Lines missing the space are not parsed as lists in the AST, so regex/line-based logic is required to detect and fix these violations.
use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;
use toml;
use crate::rules::code_block_utils::CodeBlockUtils;

lazy_static! {
    // Horizontal rule pattern
    static ref HR_PATTERN: Regex = Regex::new(r"^\s*[-*_]{3,}\s*$").unwrap();
    // List item pattern: matches lines that start with optional indentation, a list marker, and no space after
    static ref LIST_ITEM_RE: Regex = Regex::new(r"^(\s*)([-*+]|\d+[.)])(\S.*)").unwrap();
    // Fix pattern: matches lines that start with optional indentation, a list marker, optional spaces, and the rest
    static ref FIX_LIST_ITEM_RE: Regex = Regex::new(r"^(\s*)([-*+]|\d+[.)])(\s*)(.*)$").unwrap();
}

#[derive(Debug, Clone)]
pub struct MD015NoMissingSpaceAfterListMarker {
    pub require_space: bool,
}

impl Default for MD015NoMissingSpaceAfterListMarker {
    fn default() -> Self {
        Self {
            require_space: true,
        }
    }
}

impl MD015NoMissingSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_require_space(require_space: bool) -> Self {
        Self { require_space }
    }

    /// Check if a line is a horizontal rule
    #[inline(always)]
    fn is_horizontal_rule(line: &str) -> bool {
        HR_PATTERN.is_match(line)
    }

    /// Check if a line is a list item missing a space after the marker
    #[inline(always)]
    fn is_list_item_without_space(line: &str) -> bool {
        if line.is_empty() || line.trim().is_empty() {
            return false;
        }
        LIST_ITEM_RE.captures(line).is_some()
    }

    /// Fix a single list item line by ensuring a single space after the marker
    fn fix_list_item(line: &str) -> String {
        if let Some(caps) = FIX_LIST_ITEM_RE.captures(line) {
            let indent = &caps[1];
            let marker = &caps[2];
            let content = &caps[4];
            format!("{}{} {}", indent, marker, content)
        } else {
            line.to_string()
        }
    }

    /// Main check logic: flag all lines that look like list items but are missing a space after the marker
    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if !self.require_space {
            return Ok(vec![]);
        }
        let mut warnings = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();
        let code_block_lines = CodeBlockUtils::identify_code_block_lines(ctx.content);
        for (i, line) in lines.iter().enumerate() {
            if code_block_lines.get(i).copied().unwrap_or(false) {
                continue; // skip code blocks
            }
            if Self::is_horizontal_rule(line) {
                continue; // skip horizontal rules
            }
            if let Some(caps) = LIST_ITEM_RE.captures(line) {
                let marker = &caps[2];
                let message = if marker == "*" || marker == "-" || marker == "+" {
                    "Missing space after unordered list marker"
                } else {
                    "Missing space after ordered list marker"
                };
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    severity: Severity::Warning,
                    line: i + 1,
                    column: 1,
                    message: message.to_string(),
                    fix: None,
                });
            }
        }
        Ok(warnings)
    }

    /// Main fix logic: insert a single space after the marker for all lines that look like list items but are missing it
    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if !self.require_space {
            return Ok(ctx.content.to_string());
        }
        let mut lines: Vec<String> = ctx.content.lines().map(|s| s.to_string()).collect();
        let code_block_lines = CodeBlockUtils::identify_code_block_lines(ctx.content);
        for (i, line_str) in lines.iter_mut().enumerate() {
            if code_block_lines.get(i).copied().unwrap_or(false) {
                continue; // skip code blocks
            }
            if Self::is_horizontal_rule(line_str) {
                continue; // skip horizontal rules
            }
            if LIST_ITEM_RE.captures(line_str).is_some() {
                *line_str = Self::fix_list_item(line_str);
            }
        }
        Ok(lines.join("\n"))
    }
}

impl Rule for MD015NoMissingSpaceAfterListMarker {
    fn name(&self) -> &'static str {
        "MD015"
    }

    fn description(&self) -> &'static str {
        "List markers must be followed by a space"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        self.check(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        self.fix(ctx)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("require_space".to_string(), toml::Value::Boolean(self.require_space));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let require_space = crate::config::get_rule_config_value::<bool>(config, "MD015", "require_space").unwrap_or(true);
        Box::new(MD015NoMissingSpaceAfterListMarker { require_space })
    }
}
