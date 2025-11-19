use crate::rule_config_serde::RuleConfig;
use crate::types::NonNegativeUsize;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Represents the blank-line requirement for a heading level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingBlankRequirement {
    /// Require an exact minimum count of blank lines (0 or greater).
    /// Stored as `NonNegativeUsize` to forbid negative values while allowing `0`.
    Limited(NonNegativeUsize),
    /// `-1` in markdownlint config – accept any number of blank lines (including none).
    Unlimited,
}

impl HeadingBlankRequirement {
    pub fn limited(value: usize) -> Self {
        HeadingBlankRequirement::Limited(NonNegativeUsize::new(value))
    }

    pub const fn unlimited() -> Self {
        HeadingBlankRequirement::Unlimited
    }

    pub fn is_unlimited(&self) -> bool {
        matches!(self, HeadingBlankRequirement::Unlimited)
    }

    pub fn required_count(&self) -> Option<usize> {
        match self {
            HeadingBlankRequirement::Limited(value) => Some(value.get()),
            HeadingBlankRequirement::Unlimited => None,
        }
    }
}

impl Default for HeadingBlankRequirement {
    fn default() -> Self {
        HeadingBlankRequirement::limited(1)
    }
}

/// Configuration for blank lines that can be specified per heading level or globally
#[derive(Debug, Clone, PartialEq)]
pub enum HeadingLevelConfig {
    /// Same value for all heading levels
    Scalar(HeadingBlankRequirement),
    /// Per-level values for heading levels 1-6
    PerLevel([HeadingBlankRequirement; 6]),
}

impl HeadingLevelConfig {
    /// Get the configured value for a specific heading level (1-6)
    pub fn get_for_level(&self, level: usize) -> &HeadingBlankRequirement {
        match self {
            HeadingLevelConfig::Scalar(value) => value,
            HeadingLevelConfig::PerLevel(values) => {
                if (1..=6).contains(&level) {
                    &values[level - 1]
                } else {
                    const DEFAULT: HeadingBlankRequirement =
                        HeadingBlankRequirement::Limited(NonNegativeUsize::from_const(1));
                    &DEFAULT
                }
            }
        }
    }

    /// Create a scalar configuration with the same value for all levels
    pub fn scalar(value: usize) -> Self {
        HeadingLevelConfig::Scalar(HeadingBlankRequirement::limited(value))
    }

    /// Create a scalar configuration from an explicit requirement (Limited or Unlimited)
    pub fn scalar_requirement(value: HeadingBlankRequirement) -> Self {
        HeadingLevelConfig::Scalar(value)
    }

    /// Create a per-level configuration (all limited values)
    pub fn per_level(values: [usize; 6]) -> Self {
        HeadingLevelConfig::PerLevel(values.map(HeadingBlankRequirement::limited))
    }

    /// Create a per-level configuration from explicit requirements
    pub fn per_level_requirements(values: [HeadingBlankRequirement; 6]) -> Self {
        HeadingLevelConfig::PerLevel(values)
    }
}

impl Default for HeadingLevelConfig {
    fn default() -> Self {
        HeadingLevelConfig::Scalar(HeadingBlankRequirement::default())
    }
}

impl<'de> Deserialize<'de> for HeadingBlankRequirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;
        if value == -1 {
            Ok(HeadingBlankRequirement::unlimited())
        } else if value >= 0 {
            Ok(HeadingBlankRequirement::limited(value as usize))
        } else {
            Err(serde::de::Error::custom(format!(
                "Invalid blank line requirement {value}. Use -1 for unlimited or a non-negative integer."
            )))
        }
    }
}

impl Serialize for HeadingBlankRequirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            HeadingBlankRequirement::Unlimited => (-1).serialize(serializer),
            HeadingBlankRequirement::Limited(value) => value.get().serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for HeadingLevelConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Scalar(HeadingBlankRequirement),
            Array(Vec<HeadingBlankRequirement>),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Scalar(value) => Ok(HeadingLevelConfig::Scalar(value)),
            Helper::Array(values) => {
                if values.len() != 6 {
                    return Err(D::Error::custom(format!(
                        "Heading level array must have exactly 6 values (for levels 1-6), got {}",
                        values.len()
                    )));
                }
                let mut array = [HeadingBlankRequirement::default(); 6];
                array.copy_from_slice(&values);
                Ok(HeadingLevelConfig::PerLevel(array))
            }
        }
    }
}

impl Serialize for HeadingLevelConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            HeadingLevelConfig::Scalar(value) => value.serialize(serializer),
            HeadingLevelConfig::PerLevel(values) => values[..].serialize(serializer),
        }
    }
}

/// Configuration for MD022 (Headings should be surrounded by blank lines)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD022Config {
    /// Number of blank lines required above headings (default: 1 for all levels)
    /// Can be a single integer (applies to all levels) or an array of 6 integers (one per level 1-6)
    #[serde(default = "default_lines_above", alias = "lines_above")]
    pub lines_above: HeadingLevelConfig,

    /// Number of blank lines required below headings (default: 1 for all levels)
    /// Can be a single integer (applies to all levels) or an array of 6 integers (one per level 1-6)
    #[serde(default = "default_lines_below", alias = "lines_below")]
    pub lines_below: HeadingLevelConfig,

    /// Whether the first heading can be at the start of the document (default: true)
    #[serde(default = "default_allowed_at_start", alias = "allowed_at_start")]
    pub allowed_at_start: bool,
}

fn default_lines_above() -> HeadingLevelConfig {
    HeadingLevelConfig::default()
}

fn default_lines_below() -> HeadingLevelConfig {
    HeadingLevelConfig::default()
}

fn default_allowed_at_start() -> bool {
    true
}

impl Default for MD022Config {
    fn default() -> Self {
        Self {
            lines_above: default_lines_above(),
            lines_below: default_lines_below(),
            allowed_at_start: default_allowed_at_start(),
        }
    }
}

impl RuleConfig for MD022Config {
    const RULE_NAME: &'static str = "MD022";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_backwards_compatibility() {
        let toml_str = r#"
            lines_above = 2
            lines_below = 3
            allowed_at_start = false
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lines_above.get_for_level(1).required_count(), Some(2));
        assert_eq!(config.lines_below.get_for_level(1).required_count(), Some(3));
        assert!(!config.allowed_at_start);
    }

    #[test]
    fn test_kebab_case_canonical_format() {
        let toml_str = r#"
            lines-above = 2
            lines-below = 3
            allowed-at-start = false
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lines_above.get_for_level(1).required_count(), Some(2));
        assert_eq!(config.lines_below.get_for_level(1).required_count(), Some(3));
        assert!(!config.allowed_at_start);
    }

    #[test]
    fn test_per_level_array_configuration() {
        let toml_str = r#"
            lines-above = [0, 1, 1, 2, 2, 2]
            lines-below = [1, 1, 1, 1, 1, 1]
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();

        // Test lines_above for each level
        assert_eq!(config.lines_above.get_for_level(1).required_count(), Some(0)); // H1: no blank above
        assert_eq!(config.lines_above.get_for_level(2).required_count(), Some(1)); // H2: 1 blank above
        assert_eq!(config.lines_above.get_for_level(3).required_count(), Some(1)); // H3: 1 blank above
        assert_eq!(config.lines_above.get_for_level(4).required_count(), Some(2)); // H4: 2 blanks above
        assert_eq!(config.lines_above.get_for_level(5).required_count(), Some(2)); // H5: 2 blanks above
        assert_eq!(config.lines_above.get_for_level(6).required_count(), Some(2)); // H6: 2 blanks above

        // Test lines_below (all 1)
        for level in 1..=6 {
            assert_eq!(config.lines_below.get_for_level(level).required_count(), Some(1));
        }
    }

    #[test]
    fn test_per_level_wrong_length() {
        let toml_str = r#"
            lines-above = [1, 2, 3]
        "#;
        let result: Result<MD022Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exactly 6 values"));
    }

    #[test]
    fn test_scalar_applies_to_all_levels() {
        let toml_str = r#"
            lines-above = 2
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();

        // All levels should get the same value
        for level in 1..=6 {
            assert_eq!(config.lines_above.get_for_level(level).required_count(), Some(2));
        }
    }

    #[test]
    fn test_serialization_roundtrip_scalar() {
        let config = MD022Config {
            lines_above: HeadingLevelConfig::scalar(2),
            lines_below: HeadingLevelConfig::scalar(1),
            allowed_at_start: false,
        };

        let serialized = toml::to_string(&config).unwrap();
        let deserialized: MD022Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_serialization_roundtrip_array() {
        let config = MD022Config {
            lines_above: HeadingLevelConfig::per_level([0, 1, 1, 2, 2, 2]),
            lines_below: HeadingLevelConfig::per_level([1, 1, 1, 1, 1, 1]),
            allowed_at_start: true,
        };

        let serialized = toml::to_string(&config).unwrap();
        let deserialized: MD022Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_scalar_unlimited_configuration() {
        let toml_str = r#"
            lines-above = -1
            lines-below = 0
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();
        assert!(config.lines_above.get_for_level(1).is_unlimited());
        assert_eq!(config.lines_below.get_for_level(1).required_count(), Some(0));
    }

    #[test]
    fn test_per_level_with_unlimited_entries() {
        let toml_str = r#"
            lines-above = [-1, 0, 1, 2, 2, 2]
            lines-below = [1, -1, 1, 1, 1, 1]
        "#;
        let config: MD022Config = toml::from_str(toml_str).unwrap();
        assert!(config.lines_above.get_for_level(1).is_unlimited());
        assert_eq!(config.lines_above.get_for_level(2).required_count(), Some(0));
        assert!(config.lines_below.get_for_level(2).is_unlimited());
    }
}
