use crate::rule_config_serde::RuleConfig;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One target style for the auto-fix conversion.
///
/// `Auto` is a meta-value that expands to a source-aware default ordering. The
/// six concrete variants are the six MD054 link/image styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PreferredStyle {
    /// Use the source-aware default candidate ordering. As a list entry, this
    /// acts as a wildcard fallback so explicit values can be tried first.
    #[default]
    Auto,
    Full,
    Collapsed,
    Shortcut,
    Inline,
    Autolink,
    #[serde(alias = "url_inline")]
    UrlInline,
}

/// Ordered list of preferred targets for the MD054 auto-fix.
///
/// Accepts either a scalar (`preferred-style: autolink`) or a list
/// (`preferred-style: [autolink, full]`) at the config layer; both forms
/// normalize to a non-empty `Vec<PreferredStyle>` internally. The first entry
/// that is allowed *and* reachable from the source style wins; `Auto` expands
/// inline to the source-aware default ordering, so `[autolink, auto]` means
/// "prefer autolink, else fall back to the normal Auto behaviour".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferredStyles(Vec<PreferredStyle>);

impl PreferredStyles {
    /// Read access for the conversion planner.
    pub fn as_slice(&self) -> &[PreferredStyle] {
        &self.0
    }

    /// Construct from a single style. Used by call sites that pin one target.
    pub fn single(style: PreferredStyle) -> Self {
        Self(vec![style])
    }
}

impl FromIterator<PreferredStyle> for PreferredStyles {
    /// Build a multi-entry preference list. Empty iterators are rejected at
    /// construction time so callers fail fast rather than at fix time.
    fn from_iter<I: IntoIterator<Item = PreferredStyle>>(iter: I) -> Self {
        let v: Vec<_> = iter.into_iter().collect();
        assert!(!v.is_empty(), "PreferredStyles must contain at least one entry");
        Self(v)
    }
}

impl Default for PreferredStyles {
    fn default() -> Self {
        Self(vec![PreferredStyle::Auto])
    }
}

impl<'de> Deserialize<'de> for PreferredStyles {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Single(PreferredStyle),
            Many(Vec<PreferredStyle>),
        }
        let parsed = match Either::deserialize(d)? {
            Either::Single(style) => vec![style],
            Either::Many(list) => list,
        };
        if parsed.is_empty() {
            return Err(serde::de::Error::custom(
                "preferred-style list must contain at least one entry",
            ));
        }
        Ok(Self(parsed))
    }
}

impl Serialize for PreferredStyles {
    /// Round-trips a single-element list as a scalar so configuration files
    /// don't grow brackets the user didn't write.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if let [only] = self.0.as_slice() {
            only.serialize(s)
        } else {
            self.0.serialize(s)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MD054Config {
    #[serde(default = "default_true")]
    pub autolink: bool,
    #[serde(default = "default_true")]
    pub collapsed: bool,
    #[serde(default = "default_true")]
    pub full: bool,
    #[serde(default = "default_true")]
    pub inline: bool,
    #[serde(default = "default_true")]
    pub shortcut: bool,
    #[serde(default = "default_true", rename = "url-inline", alias = "url_inline")]
    pub url_inline: bool,
    /// Ordered preference for the auto-fix target style. Accepts a scalar (one
    /// style) or a list (priority order). The default `auto` selects the
    /// best-fitting style for the source. See `PreferredStyles` docs for the
    /// list semantics, including `auto` as a wildcard fallback entry.
    #[serde(default, rename = "preferred-style", alias = "preferred_style")]
    pub preferred_style: PreferredStyles,
}

impl Default for MD054Config {
    fn default() -> Self {
        Self {
            autolink: true,
            collapsed: true,
            full: true,
            inline: true,
            shortcut: true,
            url_inline: true,
            preferred_style: PreferredStyles::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

impl RuleConfig for MD054Config {
    const RULE_NAME: &'static str = "MD054";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_toml(s: &str) -> MD054Config {
        toml::from_str(s).expect("config should parse")
    }

    #[test]
    fn scalar_preferred_style_parses() {
        let cfg = parse_toml(r#"preferred-style = "autolink""#);
        assert_eq!(cfg.preferred_style.as_slice(), &[PreferredStyle::Autolink]);
    }

    #[test]
    fn list_preferred_style_parses_in_order() {
        let cfg = parse_toml(r#"preferred-style = ["autolink", "full"]"#);
        assert_eq!(
            cfg.preferred_style.as_slice(),
            &[PreferredStyle::Autolink, PreferredStyle::Full]
        );
    }

    #[test]
    fn list_with_auto_fallback_parses() {
        let cfg = parse_toml(r#"preferred-style = ["autolink", "auto"]"#);
        assert_eq!(
            cfg.preferred_style.as_slice(),
            &[PreferredStyle::Autolink, PreferredStyle::Auto]
        );
    }

    #[test]
    fn empty_list_rejected() {
        let err = toml::from_str::<MD054Config>(r#"preferred-style = []"#).unwrap_err();
        assert!(
            err.to_string().contains("at least one entry"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn missing_field_defaults_to_auto() {
        let cfg = parse_toml("");
        assert_eq!(cfg.preferred_style.as_slice(), &[PreferredStyle::Auto]);
    }

    #[test]
    fn snake_case_alias_still_works() {
        let cfg = parse_toml(r#"preferred_style = "full""#);
        assert_eq!(cfg.preferred_style.as_slice(), &[PreferredStyle::Full]);
    }

    #[test]
    fn url_inline_kebab_and_snake_case_both_parse() {
        let cfg = parse_toml(r#"preferred-style = "url-inline""#);
        assert_eq!(cfg.preferred_style.as_slice(), &[PreferredStyle::UrlInline]);
        let cfg = parse_toml(r#"preferred-style = "url_inline""#);
        assert_eq!(cfg.preferred_style.as_slice(), &[PreferredStyle::UrlInline]);
    }

    #[test]
    fn single_element_list_round_trips_as_scalar() {
        let cfg = MD054Config {
            preferred_style: PreferredStyles::single(PreferredStyle::Autolink),
            ..MD054Config::default()
        };
        let serialized = toml::to_string(&cfg).unwrap();
        assert!(
            serialized.contains(r#"preferred-style = "autolink""#),
            "single element should serialize as scalar, got:\n{serialized}"
        );
    }

    #[test]
    fn multi_element_list_serializes_as_list() {
        let cfg = MD054Config {
            preferred_style: PreferredStyles(vec![PreferredStyle::Autolink, PreferredStyle::Full]),
            ..MD054Config::default()
        };
        let serialized = toml::to_string(&cfg).unwrap();
        assert!(
            serialized.contains("preferred-style = [\"autolink\", \"full\"]"),
            "multi-element should serialize as list, got:\n{serialized}"
        );
    }
}
