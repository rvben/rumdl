use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Indentation size (1-8 spaces)
///
/// Enforces reasonable indentation bounds. While Markdown technically allows any
/// indentation, values outside 1-8 are either mistakes or impractical. Common values
/// are 2 (default) and 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndentSize(u8);

impl IndentSize {
    /// Minimum indentation (1 space)
    pub const MIN: u8 = 1;
    /// Maximum indentation (8 spaces)
    pub const MAX: u8 = 8;

    /// Create a new IndentSize, validating it's in range 1-8.
    ///
    /// # Errors
    /// Returns `IndentSizeError` if the value is not between 1 and 8 inclusive.
    pub fn new(value: u8) -> Result<Self, IndentSizeError> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(IndentSizeError(value))
        }
    }

    /// Get the underlying value (guaranteed to be 1-8).
    pub fn get(self) -> u8 {
        self.0
    }

    /// Convert to usize for use in calculations.
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }

    /// Convert from a default value (for use in config defaults).
    ///
    /// # Panics
    /// Panics if the value is not in range 1-8. This is intended for const defaults only.
    pub const fn from_const(value: u8) -> Self {
        assert!(
            value >= Self::MIN && value <= Self::MAX,
            "IndentSize must be between 1 and 8"
        );
        Self(value)
    }
}

impl Default for IndentSize {
    fn default() -> Self {
        Self(2) // Safe: 2 is a common default
    }
}

/// Error type for invalid IndentSize values.
#[derive(Debug, Clone, Copy)]
pub struct IndentSizeError(u8);

impl fmt::Display for IndentSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Indentation must be between {} and {} spaces, got {}. \
             Common values are 2 (default) or 4. Values outside this range are likely errors.",
            IndentSize::MIN,
            IndentSize::MAX,
            self.0
        )
    }
}

impl std::error::Error for IndentSizeError {}

impl<'de> Deserialize<'de> for IndentSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        IndentSize::new(value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for IndentSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl From<IndentSize> for usize {
    fn from(val: IndentSize) -> Self {
        val.0 as usize
    }
}

impl From<IndentSize> for u8 {
    fn from(val: IndentSize) -> Self {
        val.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_values() {
        for value in 1..=8 {
            let indent = IndentSize::new(value).unwrap();
            assert_eq!(indent.get(), value);
            assert_eq!(indent.as_usize(), value as usize);
            assert_eq!(u8::from(indent), value);
            assert_eq!(usize::from(indent), value as usize);
        }
    }

    #[test]
    fn test_invalid_values() {
        for value in [0, 9, 10, 50, 255] {
            assert!(IndentSize::new(value).is_err());
        }
    }

    #[test]
    fn test_default() {
        assert_eq!(IndentSize::default().get(), 2);
    }

    #[test]
    fn test_from_const() {
        const DEFAULT: IndentSize = IndentSize::from_const(2);
        assert_eq!(DEFAULT.get(), 2);
    }

    #[test]
    fn test_constants() {
        assert_eq!(IndentSize::MIN, 1);
        assert_eq!(IndentSize::MAX, 8);
    }

    #[test]
    fn test_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct TestConfig {
            indent: IndentSize,
        }

        let config = TestConfig {
            indent: IndentSize::new(4).unwrap(),
        };
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.indent.get(), 4);
    }

    #[test]
    fn test_validation_error() {
        #[derive(Debug, serde::Deserialize)]
        struct TestConfig {
            indent: IndentSize,
        }

        // Test value below minimum
        let toml_str = "indent = 0";
        let result: Result<TestConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("between 1 and 8") || err.contains("got 0"));

        // Test value above maximum
        let toml_str = "indent = 10";
        let result: Result<TestConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());

        // Test valid value works
        let valid_toml = "indent = 2";
        let config: TestConfig = toml::from_str(valid_toml).unwrap();
        assert_eq!(config.indent.get(), 2);
    }
}
