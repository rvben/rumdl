use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Number of trailing spaces for Markdown line breaks (≥2)
///
/// In Markdown, a line break requires at least 2 trailing spaces. Values of 0 or 1
/// don't create line breaks and would silently fail. This type enforces that constraint
/// at deserialization time, preventing broken line break configurations.
///
/// CommonMark specification requires exactly 2 spaces, but some flavors allow more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BrSpaces(usize);

impl BrSpaces {
    /// Minimum value for line breaks (CommonMark standard)
    pub const MIN: usize = 2;

    /// Create a new BrSpaces, validating it's at least 2.
    ///
    /// # Errors
    /// Returns `BrSpacesError` if the value is less than 2.
    pub fn new(value: usize) -> Result<Self, BrSpacesError> {
        if value >= Self::MIN {
            Ok(Self(value))
        } else {
            Err(BrSpacesError(value))
        }
    }

    /// Get the underlying value (guaranteed to be ≥2).
    pub fn get(self) -> usize {
        self.0
    }

    /// Convert from a default value (for use in config defaults).
    ///
    /// # Panics
    /// Panics if the value is less than 2. This is intended for const defaults only.
    pub const fn from_const(value: usize) -> Self {
        assert!(value >= Self::MIN, "BrSpaces must be at least 2 (CommonMark standard)");
        Self(value)
    }
}

impl Default for BrSpaces {
    fn default() -> Self {
        Self(2) // Safe: 2 is the CommonMark standard
    }
}

/// Error type for invalid BrSpaces values.
#[derive(Debug, Clone, Copy)]
pub struct BrSpacesError(usize);

impl fmt::Display for BrSpacesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Line break spaces must be at least 2, got {}. \
             Markdown requires at least 2 trailing spaces to create a line break \
             (CommonMark specification). Values of 0 or 1 do not create line breaks.",
            self.0
        )
    }
}

impl std::error::Error for BrSpacesError {}

impl<'de> Deserialize<'de> for BrSpaces {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = usize::deserialize(deserializer)?;
        BrSpaces::new(value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for BrSpaces {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl From<BrSpaces> for usize {
    fn from(val: BrSpaces) -> Self {
        val.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_values() {
        for value in [2, 3, 4, 10, 100] {
            let br_spaces = BrSpaces::new(value).unwrap();
            assert_eq!(br_spaces.get(), value);
            assert_eq!(usize::from(br_spaces), value);
        }
    }

    #[test]
    fn test_invalid_values() {
        for value in [0, 1] {
            assert!(BrSpaces::new(value).is_err());
        }
    }

    #[test]
    fn test_default() {
        assert_eq!(BrSpaces::default().get(), 2);
    }

    #[test]
    fn test_from_const() {
        const DEFAULT: BrSpaces = BrSpaces::from_const(2);
        assert_eq!(DEFAULT.get(), 2);
    }

    #[test]
    fn test_min_constant() {
        assert_eq!(BrSpaces::MIN, 2);
    }

    #[test]
    fn test_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct TestConfig {
            spaces: BrSpaces,
        }

        let config = TestConfig {
            spaces: BrSpaces::new(3).unwrap(),
        };
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.spaces.get(), 3);
    }

    #[test]
    fn test_validation_error() {
        #[derive(Debug, serde::Deserialize)]
        struct TestConfig {
            spaces: BrSpaces,
        }

        // Test value below minimum
        let toml_str = "spaces = 1";
        let result: Result<TestConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must be at least 2") || err.contains("got 1"));

        // Test zero
        let toml_str = "spaces = 0";
        let result: Result<TestConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());

        // Test valid value works
        let valid_toml = "spaces = 2";
        let config: TestConfig = toml::from_str(valid_toml).unwrap();
        assert_eq!(config.spaces.get(), 2);
    }
}
