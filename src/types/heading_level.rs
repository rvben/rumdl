use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Markdown heading level (1-6)
///
/// Markdown supports exactly 6 levels of headings, from # (level 1) through ###### (level 6).
/// This type enforces that constraint at both compile time (after construction) and runtime
/// (during config deserialization).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeadingLevel(u8);

impl HeadingLevel {
    /// Create a new heading level, validating it's in the range 1-6.
    ///
    /// # Errors
    /// Returns `HeadingLevelError` if the level is not between 1 and 6 inclusive.
    pub fn new(level: u8) -> Result<Self, HeadingLevelError> {
        if (1..=6).contains(&level) {
            Ok(Self(level))
        } else {
            Err(HeadingLevelError(level))
        }
    }

    /// Get the underlying heading level value (1-6).
    pub fn get(self) -> u8 {
        self.0
    }

    /// Convert to usize for compatibility with existing code.
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Error type for invalid heading levels.
#[derive(Debug, Clone, Copy)]
pub struct HeadingLevelError(u8);

impl fmt::Display for HeadingLevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Heading level must be between 1 and 6, got {}. \
             Markdown supports only 6 heading levels (# through ######).",
            self.0
        )
    }
}

impl std::error::Error for HeadingLevelError {}

impl<'de> Deserialize<'de> for HeadingLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let level = u8::deserialize(deserializer)?;
        HeadingLevel::new(level).map_err(serde::de::Error::custom)
    }
}

impl Serialize for HeadingLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl Default for HeadingLevel {
    fn default() -> Self {
        Self(1) // Safe: 1 is always valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_heading_levels() {
        for level in 1..=6 {
            let h = HeadingLevel::new(level).unwrap();
            assert_eq!(h.get(), level);
            assert_eq!(h.as_usize(), level as usize);
        }
    }

    #[test]
    fn test_invalid_heading_levels() {
        for level in [0, 7, 8, 10, 255] {
            assert!(HeadingLevel::new(level).is_err());
        }
    }

    #[test]
    fn test_default() {
        assert_eq!(HeadingLevel::default().get(), 1);
    }

    #[test]
    fn test_roundtrip() {
        // Test that HeadingLevel can be serialized and deserialized within a struct
        #[derive(serde::Serialize, serde::Deserialize)]
        struct TestConfig {
            level: HeadingLevel,
        }

        let config = TestConfig {
            level: HeadingLevel::new(3).unwrap(),
        };
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.level.get(), 3);
    }

    #[test]
    fn test_validation_error() {
        #[derive(Debug, serde::Deserialize)]
        struct TestConfig {
            level: HeadingLevel,
        }

        let toml_str = "level = 10";
        let result: Result<TestConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be between 1 and 6") || err.contains("got 10"));

        // Also test that valid config deserializes correctly
        let valid_toml = "level = 3";
        let config: TestConfig = toml::from_str(valid_toml).unwrap();
        assert_eq!(config.level.get(), 3);
    }
}
