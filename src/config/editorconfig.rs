//! `.editorconfig` support, opt-in via `[global] editorconfig = true`.
//!
//! Properties are resolved per file, so section globs and nested
//! `.editorconfig` files apply exactly as written, and are layered in at
//! [`ConfigSource::EditorConfig`]: they fill in settings no rumdl config
//! mentions and lose to any that it does.
//!
//! Only properties with an unambiguous rumdl equivalent are mapped. The rest
//! are read solely to report where rumdl's behavior contradicts what the
//! `.editorconfig` asks for; every such warning names the rule responsible, so
//! the caller can drop it when that rule is not enabled.

use std::path::Path;

use ec4rs::Properties;
use ec4rs::property::{
    FinalNewline, IndentSize as EcIndentSize, IndentStyle as EcIndentStyle, MaxLineLen, TabWidth, TrimTrailingWs,
};
use ec4rs::rawvalue::RawValue;

use super::source_tracking::{ConfigSource, SourcedConfig, SourcedValue};
use crate::types::{IndentSize, LineLength};

/// Decides whether hard tabs are allowed.
const HARD_TABS_RULE: &str = "MD010";
/// Decides whether trailing whitespace is allowed.
const TRAILING_SPACES_RULE: &str = "MD009";
/// Requires a single trailing newline.
const FINAL_NEWLINE_RULE: &str = "MD047";
/// Owns the `indent` option that `indent_size` maps onto.
const UL_INDENT_RULE: &str = "MD007";
/// Enforces the line length, and is the only rule the global one is read for.
const LINE_LENGTH_RULE: &str = "MD013";

/// The `.editorconfig` properties rumdl maps onto its own settings.
///
/// Ordered so it can key a map of file groups: two files resolving to the same
/// settings share one effective config.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct EditorConfigSettings {
    /// `max_line_length`, mapped onto `[global] line-length`. `off` becomes
    /// [`LineLength::new(0)`], which rumdl reads as no limit.
    pub line_length: Option<LineLength>,
    /// `indent_size` in spaces, mapped onto MD007's `indent`.
    pub indent: Option<IndentSize>,
}

impl EditorConfigSettings {
    /// Whether any property mapped onto a rumdl setting.
    pub fn is_empty(&self) -> bool {
        self.line_length.is_none() && self.indent.is_none()
    }
}

/// A `.editorconfig` property rumdl read but does not act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorConfigWarning {
    /// The rule whose behavior contradicts the property, when the divergence is
    /// rule-specific. Such a message is only true while that rule is enabled.
    /// `None` means the property itself is unusable, which is worth reporting
    /// whatever rules are active.
    pub rule: Option<&'static str>,
    pub message: String,
}

impl EditorConfigWarning {
    fn unusable(message: String) -> Self {
        Self { rule: None, message }
    }

    fn diverges(rule: &'static str, message: String) -> Self {
        Self {
            rule: Some(rule),
            message,
        }
    }
}

/// What the `.editorconfig` files applying to one file resolve to.
#[derive(Debug, Clone, Default)]
pub struct EditorConfigResolution {
    pub settings: EditorConfigSettings,
    pub warnings: Vec<EditorConfigWarning>,
    /// The `.editorconfig` file that supplied a mapped value.
    pub origin: Option<String>,
}

/// Resolve the `.editorconfig` properties that apply to `file_path`.
///
/// The path is anchored to the working directory first: lookup walks the file's
/// ancestors, and a relative path has none. It is not canonicalized, so a file
/// that does not exist yet (an unsaved editor buffer named on `--stdin-filename`)
/// still resolves against the directory it belongs to.
///
/// Never fails: a `.editorconfig` that cannot be parsed yields no settings and
/// one warning, so a broken file downgrades to "no editorconfig" rather than
/// aborting the lint.
pub fn resolve(file_path: &Path) -> EditorConfigResolution {
    let mut resolution = EditorConfigResolution::default();

    let file_path = std::path::absolute(file_path).unwrap_or_else(|_| file_path.to_path_buf());

    let props = match ec4rs::properties_of(&file_path) {
        Ok(props) => props,
        Err(e) => {
            resolution.warnings.push(EditorConfigWarning::unusable(format!(
                "Ignoring .editorconfig for {}: {e}",
                file_path.display()
            )));
            return resolution;
        }
    };

    read_max_line_length(&props, &mut resolution);
    read_indent_size(&props, &mut resolution);
    report_divergences(&props, &mut resolution);

    resolution
}

/// Layer resolved settings into a config at [`ConfigSource::EditorConfig`].
///
/// Precedence does the work: a value still at [`ConfigSource::Default`] takes
/// the `.editorconfig` value, and anything a rumdl config or the CLI set
/// outranks it.
pub fn apply<S>(sourced: &mut SourcedConfig<S>, settings: &EditorConfigSettings, origin: Option<&str>) {
    let origin = || origin.map(str::to_string);

    // The global line length is only ever read as MD013's limit, and only while
    // MD013 has no `line-length` of its own. Filling it in would therefore
    // replace a limit the rumdl config set on the rule. Precedence cannot catch
    // that: the two settings are reconciled once the sources are gone.
    if let Some(line_length) = settings.line_length
        && !rule_sets_its_own_line_length(sourced)
    {
        sourced
            .global
            .line_length
            .merge_override(line_length, ConfigSource::EditorConfig, origin());
    }

    if let Some(indent) = settings.indent {
        // Rule keys are canonical uppercase once merged into a `SourcedConfig`.
        let rule = sourced.rules.entry(UL_INDENT_RULE.to_string()).or_default();
        let value = toml::Value::Integer(i64::from(indent.get()));
        rule.values
            .entry("indent".to_string())
            .or_insert_with(|| SourcedValue::new(value.clone(), ConfigSource::Default))
            .merge_override(value, ConfigSource::EditorConfig, origin());
    }
}

/// Whether a rumdl config or the CLI gave MD013 a line length of its own, which
/// the global setting merely stands in for.
fn rule_sets_its_own_line_length<S>(sourced: &SourcedConfig<S>) -> bool {
    sourced
        .rules
        .get(LINE_LENGTH_RULE)
        .and_then(|rule| rule.values.get("line-length"))
        .is_some_and(|value| !matches!(value.source, ConfigSource::Default | ConfigSource::EditorConfig))
}

fn read_max_line_length(props: &Properties, resolution: &mut EditorConfigResolution) {
    let raw = props.get_raw::<MaxLineLen>();
    match props.get::<MaxLineLen>() {
        Ok(MaxLineLen::Off) => {
            resolution.settings.line_length = Some(LineLength::new(0));
            resolution.record_origin(raw);
        }
        // A limit of zero is not a limit, and reading it as "unlimited" would
        // turn a meaningless value into a confident setting.
        Ok(MaxLineLen::Value(0)) => resolution.warnings.push(EditorConfigWarning::unusable(format!(
            "{}: `max_line_length = 0` is not a usable limit; ignoring it. Use `off` for no limit.",
            source_label(raw)
        ))),
        Ok(MaxLineLen::Value(limit)) => {
            resolution.settings.line_length = Some(LineLength::new(limit));
            resolution.record_origin(raw);
        }
        Err(raw) => report_unusable(raw, "max_line_length", resolution),
    }
}

fn read_indent_size(props: &Properties, resolution: &mut EditorConfigResolution) {
    let raw = props.get_raw::<EcIndentSize>();
    let spaces = match props.get::<EcIndentSize>() {
        Ok(EcIndentSize::Value(spaces)) => spaces,
        // `indent_size = tab` defers to `tab_width`. Guessing a width when none
        // is given would invent an indent the project never stated.
        Ok(EcIndentSize::UseTabWidth) => match props.get::<TabWidth>() {
            Ok(TabWidth::Value(width)) => width,
            Err(_) => {
                resolution.warnings.push(EditorConfigWarning::unusable(format!(
                    "{}: `indent_size = tab` needs a `tab_width` to resolve to a number of spaces; ignoring it.",
                    source_label(raw)
                )));
                return;
            }
        },
        Err(raw) => {
            report_unusable(raw, "indent_size", resolution);
            return;
        }
    };

    match u8::try_from(spaces).ok().and_then(|s| IndentSize::new(s).ok()) {
        Some(indent) => {
            resolution.settings.indent = Some(indent);
            resolution.record_origin(raw);
        }
        None => resolution.warnings.push(EditorConfigWarning::unusable(format!(
            "{}: `indent_size = {spaces}` is outside the {}-{} spaces {UL_INDENT_RULE} accepts; ignoring it.",
            source_label(raw),
            IndentSize::MIN,
            IndentSize::MAX
        ))),
    }
}

/// Report the properties rumdl reads but will not follow.
///
/// Only the values that actually contradict rumdl are reported: asking for
/// spaces, trimmed trailing whitespace or a final newline is what rumdl already
/// enforces, so those stay silent.
fn report_divergences(props: &Properties, resolution: &mut EditorConfigResolution) {
    if let Ok(EcIndentStyle::Tabs) = props.get::<EcIndentStyle>() {
        resolution.warnings.push(EditorConfigWarning::diverges(
            HARD_TABS_RULE,
            format!(
                "{}: `indent_style = tab` is not applied; {HARD_TABS_RULE} flags hard tabs. \
                 Disable {HARD_TABS_RULE} in your rumdl config to allow them.",
                source_label(props.get_raw::<EcIndentStyle>())
            ),
        ));
    }

    if let Ok(TrimTrailingWs::Value(false)) = props.get::<TrimTrailingWs>() {
        resolution.warnings.push(EditorConfigWarning::diverges(
            TRAILING_SPACES_RULE,
            format!(
                "{}: `trim_trailing_whitespace = false` is not applied; {TRAILING_SPACES_RULE} flags trailing \
                 whitespace. Disable {TRAILING_SPACES_RULE} in your rumdl config to allow it.",
                source_label(props.get_raw::<TrimTrailingWs>())
            ),
        ));
    }

    if let Ok(FinalNewline::Value(false)) = props.get::<FinalNewline>() {
        resolution.warnings.push(EditorConfigWarning::diverges(
            FINAL_NEWLINE_RULE,
            format!(
                "{}: `insert_final_newline = false` is not applied; {FINAL_NEWLINE_RULE} requires a final newline. \
                 Disable {FINAL_NEWLINE_RULE} in your rumdl config to allow files without one.",
                source_label(props.get_raw::<FinalNewline>())
            ),
        ));
    }
}

/// Report a value that was set but could not be parsed.
///
/// A property that was never set, and one written as the literal `unset`, both
/// mean "no value to map" rather than a mistake, so neither is reported.
fn report_unusable(raw: &RawValue, key: &str, resolution: &mut EditorConfigResolution) {
    if let Ok(value) = raw.into_result() {
        resolution.warnings.push(EditorConfigWarning::unusable(format!(
            "{}: `{key} = {value}` is not a value rumdl can use; ignoring it.",
            source_label(raw)
        )));
    }
}

impl EditorConfigResolution {
    /// Record which `.editorconfig` supplied a mapped value. The first one wins,
    /// which for a single mapped value is the file that set it.
    fn record_origin(&mut self, raw: &RawValue) {
        if self.origin.is_none() {
            self.origin = raw.source().map(|(path, _)| path.display().to_string());
        }
    }
}

/// A `file:line` label for a property, for messages that must say where a value
/// came from. Falls back to the bare filename when source tracking has nothing.
///
/// The path is shown relative to the working directory, as config warnings from
/// rumdl's own files are; `.editorconfig` paths are absolute because the walk
/// that found them is.
fn source_label(raw: &RawValue) -> String {
    match raw.source() {
        Some((path, line)) => format!(
            "{}:{line}",
            super::validation::to_relative_display_path(&path.to_string_lossy())
        ),
        None => ".editorconfig".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::flavor::ConfigLoaded;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Write a `.editorconfig` and a `doc.md` beside it, returning the doc path.
    ///
    /// Every fixture is rooted so the walk stops inside the temp directory and
    /// no `.editorconfig` from the machine it runs on can leak in.
    fn fixture(contents: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".editorconfig"), format!("root = true\n{contents}")).unwrap();
        let doc = dir.path().join("doc.md");
        fs::write(&doc, "# Title\n").unwrap();
        (dir, doc)
    }

    fn messages(resolution: &EditorConfigResolution) -> String {
        resolution
            .warnings
            .iter()
            .map(|w| w.message.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn maps_max_line_length_and_indent_size() {
        let (_dir, doc) = fixture("[*.md]\nmax_line_length = 100\nindent_size = 4\n");
        let resolution = resolve(&doc);

        assert_eq!(resolution.settings.line_length, Some(LineLength::new(100)));
        assert_eq!(resolution.settings.indent, Some(IndentSize::new(4).unwrap()));
        assert!(resolution.warnings.is_empty(), "{}", messages(&resolution));
        assert!(
            resolution.origin.is_some_and(|o| o.ends_with(".editorconfig")),
            "the mapped value should be traced back to the file that set it"
        );
    }

    #[test]
    fn a_section_that_does_not_match_the_file_is_not_applied() {
        let (_dir, doc) = fixture("[*.py]\nmax_line_length = 100\nindent_size = 4\n");
        let resolution = resolve(&doc);

        assert!(resolution.settings.is_empty());
        assert!(resolution.warnings.is_empty(), "{}", messages(&resolution));
    }

    #[test]
    fn nearest_editorconfig_section_wins_over_a_broader_one() {
        let (_dir, doc) = fixture("[*]\nmax_line_length = 80\n\n[*.md]\nmax_line_length = 120\n");
        assert_eq!(resolve(&doc).settings.line_length, Some(LineLength::new(120)));
    }

    #[test]
    fn max_line_length_off_means_no_limit() {
        let (_dir, doc) = fixture("[*]\nmax_line_length = off\n");
        let line_length = resolve(&doc).settings.line_length.expect("off should map");
        assert!(line_length.is_unlimited());
    }

    #[test]
    fn max_line_length_zero_is_reported_not_read_as_unlimited() {
        let (_dir, doc) = fixture("[*]\nmax_line_length = 0\n");
        let resolution = resolve(&doc);

        assert_eq!(resolution.settings.line_length, None);
        assert!(messages(&resolution).contains("max_line_length = 0"));
    }

    #[test]
    fn indent_size_tab_resolves_through_tab_width() {
        let (_dir, doc) = fixture("[*]\nindent_size = tab\ntab_width = 4\n");
        let resolution = resolve(&doc);

        assert_eq!(resolution.settings.indent, Some(IndentSize::new(4).unwrap()));
        assert!(resolution.warnings.is_empty(), "{}", messages(&resolution));
    }

    #[test]
    fn indent_size_tab_without_tab_width_is_reported_not_guessed() {
        let (_dir, doc) = fixture("[*]\nindent_size = tab\n");
        let resolution = resolve(&doc);

        assert_eq!(resolution.settings.indent, None);
        assert!(messages(&resolution).contains("needs a `tab_width`"));
    }

    #[test]
    fn indent_size_outside_the_supported_range_is_reported() {
        for size in ["0", "12"] {
            let (_dir, doc) = fixture(&format!("[*]\nindent_size = {size}\n"));
            let resolution = resolve(&doc);

            assert_eq!(resolution.settings.indent, None, "indent_size = {size} must not apply");
            assert!(messages(&resolution).contains("outside the 1-8 spaces"));
        }
    }

    #[test]
    fn an_unparseable_value_is_reported_and_a_missing_one_is_not() {
        let (_dir, doc) = fixture("[*]\nmax_line_length = wide\n");
        let resolution = resolve(&doc);
        assert_eq!(resolution.settings.line_length, None);
        assert!(messages(&resolution).contains("max_line_length = wide"));

        let (_dir, doc) = fixture("[*]\ncharset = utf-8\n");
        let resolution = resolve(&doc);
        assert!(resolution.settings.is_empty());
        assert!(resolution.warnings.is_empty(), "{}", messages(&resolution));
    }

    #[test]
    fn the_unset_keyword_is_not_reported_as_a_bad_value() {
        let (_dir, doc) = fixture("[*]\nmax_line_length = unset\nindent_size = unset\n");
        let resolution = resolve(&doc);

        assert!(resolution.settings.is_empty());
        assert!(resolution.warnings.is_empty(), "{}", messages(&resolution));
    }

    #[test]
    fn only_the_values_rumdl_contradicts_are_reported() {
        let (_dir, doc) = fixture(
            "[*]\nindent_style = space\ntrim_trailing_whitespace = true\ninsert_final_newline = true\nend_of_line = lf\n",
        );
        let resolution = resolve(&doc);
        assert!(resolution.warnings.is_empty(), "{}", messages(&resolution));

        let (_dir, doc) = fixture(
            "[*]\nindent_style = tab\ntrim_trailing_whitespace = false\ninsert_final_newline = false\nend_of_line = crlf\n",
        );
        let resolution = resolve(&doc);
        let rules: Vec<_> = resolution.warnings.iter().map(|w| w.rule).collect();
        assert_eq!(
            rules,
            vec![
                Some(HARD_TABS_RULE),
                Some(TRAILING_SPACES_RULE),
                Some(FINAL_NEWLINE_RULE)
            ],
            "{}",
            messages(&resolution)
        );
    }

    #[test]
    fn a_missing_editorconfig_resolves_to_nothing() {
        let dir = TempDir::new().unwrap();
        let doc = dir.path().join("doc.md");
        fs::write(&doc, "# Title\n").unwrap();

        // A `.editorconfig` above the temp directory could still apply, so this
        // asserts only that the absence of one here is not itself a problem.
        let resolution = resolve(&doc);
        assert!(resolution.warnings.is_empty(), "{}", messages(&resolution));
    }

    #[test]
    fn apply_fills_in_defaults_but_never_overrides_a_rumdl_config() {
        let settings = EditorConfigSettings {
            line_length: Some(LineLength::new(120)),
            indent: Some(IndentSize::new(4).unwrap()),
        };

        let mut sourced = SourcedConfig::<ConfigLoaded>::default();
        apply(&mut sourced, &settings, Some(".editorconfig"));
        assert_eq!(sourced.global.line_length.value.get(), 120);
        assert_eq!(sourced.global.line_length.source, ConfigSource::EditorConfig);
        assert_eq!(
            sourced.rules[UL_INDENT_RULE].values["indent"].value,
            toml::Value::Integer(4)
        );

        let mut sourced = SourcedConfig::<ConfigLoaded>::default();
        sourced
            .global
            .line_length
            .push_override(LineLength::new(90), ConfigSource::ProjectConfig, None);
        sourced
            .rules
            .entry(UL_INDENT_RULE.to_string())
            .or_default()
            .values
            .insert(
                "indent".to_string(),
                SourcedValue::new(toml::Value::Integer(3), ConfigSource::ProjectConfig),
            );

        apply(&mut sourced, &settings, Some(".editorconfig"));
        assert_eq!(sourced.global.line_length.value.get(), 90);
        assert_eq!(
            sourced.rules[UL_INDENT_RULE].values["indent"].value,
            toml::Value::Integer(3)
        );
    }

    #[test]
    fn apply_leaves_the_global_limit_alone_when_the_rule_carries_its_own() {
        let settings = EditorConfigSettings {
            line_length: Some(LineLength::new(120)),
            indent: None,
        };

        let mut sourced = SourcedConfig::<ConfigLoaded>::default();
        sourced
            .rules
            .entry(LINE_LENGTH_RULE.to_string())
            .or_default()
            .values
            .insert(
                "line-length".to_string(),
                SourcedValue::new(toml::Value::Integer(80), ConfigSource::ProjectConfig),
            );

        apply(&mut sourced, &settings, Some(".editorconfig"));
        assert_eq!(
            sourced.global.line_length.source,
            ConfigSource::Default,
            "the global limit stands in for MD013's, so filling it in would override the rule"
        );
    }
}
