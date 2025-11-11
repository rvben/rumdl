use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A positive non-zero usize (≥1)
///
/// Many configuration values must be at least 1 (e.g., indentation sizes, spaces per tab).
/// This type enforces that constraint at deserialization time, preventing invalid configs
/// like "0 spaces per tab" or "0 character line length".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PositiveUsize(usize);

impl PositiveUsize {
    /// Create a new PositiveUsize, validating it's at least 1.
    ///
    /// # Errors
    /// Returns `PositiveUsizeError` if the value is 0.
    pub fn new(value: usize) -> Result<Self, PositiveUsizeError> {
        if value >= 1 {
            Ok(Self(value))
        } else {
            Err(PositiveUsizeError(value))
        }
    }

    /// Get the underlying value (guaranteed to be ≥1).
    pub fn get(self) -> usize {
        self.0
    }

    /// Convert from a default value (for use in config defaults).
    ///
    /// # Panics
    /// Panics if the value is 0. This is intended for const defaults only.
    pub const fn from_const(value: usize) -> Self {
        assert!(value >= 1, "PositiveUsize must be at least 1");
        Self(value)
    }
}

/// Error type for invalid PositiveUsize values.
#[derive(Debug, Clone, Copy)]
pub struct PositiveUsizeError(usize);

impl fmt::Display for PositiveUsizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Value must be at least 1, got {}. Zero is not a valid value for this configuration.",
            self.0
        )
    }
}

impl std::error::Error for PositiveUsizeError {}

impl<'de> Deserialize<'de> for PositiveUsize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = usize::deserialize(deserializer)?;
        PositiveUsize::new(value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for PositiveUsize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl From<PositiveUsize> for usize {
    fn from(val: PositiveUsize) -> Self {
        val.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_values() {
        for value in [1, 2, 4, 10, 100, 1000, usize::MAX] {
            let positive = PositiveUsize::new(value).unwrap();
            assert_eq!(positive.get(), value);
            assert_eq!(usize::from(positive), value);
        }
    }

    #[test]
    fn test_invalid_value() {
        assert!(PositiveUsize::new(0).is_err());
    }

    #[test]
    fn test_from_const() {
        const DEFAULT: PositiveUsize = PositiveUsize::from_const(4);
        assert_eq!(DEFAULT.get(), 4);
    }

    #[test]
    fn test_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct TestConfig {
            value: PositiveUsize,
        }

        let config = TestConfig {
            value: PositiveUsize::new(42).unwrap(),
        };
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.value.get(), 42);
    }

    #[test]
    fn test_validation_error() {
        #[derive(Debug, serde::Deserialize)]
        struct TestConfig {
            value: PositiveUsize,
        }

        let toml_str = "value = 0";
        let result: Result<TestConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be at least 1") || err.contains("got 0"));

        // Test valid value works
        let valid_toml = "value = 5";
        let config: TestConfig = toml::from_str(valid_toml).unwrap();
        assert_eq!(config.value.get(), 5);
    }
}
