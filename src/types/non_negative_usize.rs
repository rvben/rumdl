use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A non-negative usize (≥0)
///
/// Many configuration values allow explicitly specifying `0` to disable a behavior (e.g., no blank
/// lines required) while still requiring validation for negative inputs during deserialization.
/// `NonNegativeUsize` captures that intent and guarantees the stored value is never negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NonNegativeUsize(usize);

impl NonNegativeUsize {
    /// Create a new NonNegativeUsize from an unsigned value.
    #[inline]
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    /// Attempt to create from a signed integer, validating it's ≥0.
    pub fn try_from_i64(value: i64) -> Result<Self, NonNegativeUsizeError> {
        if value >= 0 {
            Ok(Self(value as usize))
        } else {
            Err(NonNegativeUsizeError(value))
        }
    }

    /// Get the underlying value.
    #[inline]
    pub fn get(self) -> usize {
        self.0
    }

    /// Convert from a const (for use in defaults).
    pub const fn from_const(value: usize) -> Self {
        Self(value)
    }
}

/// Error type for invalid NonNegativeUsize values (i.e., negative integers).
#[derive(Debug, Clone, Copy)]
pub struct NonNegativeUsizeError(i64);

impl fmt::Display for NonNegativeUsizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Value must be non-negative (≥0), got {}. Negative values are not allowed.",
            self.0
        )
    }
}

impl std::error::Error for NonNegativeUsizeError {}

impl<'de> Deserialize<'de> for NonNegativeUsize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;
        NonNegativeUsize::try_from_i64(value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for NonNegativeUsize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl From<NonNegativeUsize> for usize {
    fn from(val: NonNegativeUsize) -> Self {
        val.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct SampleConfig {
        value: NonNegativeUsize,
    }

    #[test]
    fn test_zero_and_positive_values() {
        let zero = NonNegativeUsize::new(0);
        assert_eq!(zero.get(), 0);
        assert_eq!(usize::from(zero), 0);

        for value in [1, 2, 10, 42, usize::MAX] {
            let nn = NonNegativeUsize::new(value);
            assert_eq!(nn.get(), value);
            assert_eq!(usize::from(nn), value);
        }
    }

    #[test]
    fn test_try_from_i64() {
        assert!(NonNegativeUsize::try_from_i64(0).is_ok());
        assert!(NonNegativeUsize::try_from_i64(5).is_ok());
        assert!(NonNegativeUsize::try_from_i64(-1).is_err());
        assert!(NonNegativeUsize::try_from_i64(-100).is_err());
    }

    #[test]
    fn test_from_const() {
        const DEFAULT: NonNegativeUsize = NonNegativeUsize::from_const(3);
        assert_eq!(DEFAULT.get(), 3);
    }

    #[test]
    fn test_roundtrip_ser_de() {
        let config = SampleConfig {
            value: NonNegativeUsize::new(0),
        };
        let serialized = toml::to_string(&config).unwrap();
        assert_eq!(serialized.trim(), "value = 0");
        let deserialized: SampleConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.value.get(), 0);

        let config = SampleConfig {
            value: NonNegativeUsize::new(10),
        };
        let serialized = toml::to_string(&config).unwrap();
        assert_eq!(serialized.trim(), "value = 10");
        let deserialized: SampleConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.value.get(), 10);
    }

    #[test]
    fn test_deserialize_negative_error() {
        let result: Result<SampleConfig, _> = toml::from_str("value = -5");
        let err = match result {
            Ok(_) => panic!("Expected deserialization error for negative values"),
            Err(err) => err.to_string(),
        };
        assert!(
            err.contains("non-negative") || err.contains("got -5"),
            "Unexpected error message: {err}"
        );

        // Ensure valid values deserialize as expected
        let valid: SampleConfig = toml::from_str("value = 7").unwrap();
        assert_eq!(valid.value.get(), 7);
    }
}
