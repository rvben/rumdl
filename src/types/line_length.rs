use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A line length value that can be 0 (meaning no limit) or a positive value (≥1)
///
/// Many configuration values for line length need to support both:
/// - 0: Special value meaning "no line length limit"
/// - ≥1: Actual line length limit
///
/// This type enforces those constraints at deserialization time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
#[schemars(transparent)]
pub struct LineLength(Option<usize>);

impl LineLength {
    /// Create a new LineLength, where 0 means no limit and values ≥1 are actual limits.
    pub fn new(value: usize) -> Self {
        if value == 0 { Self(None) } else { Self(Some(value)) }
    }

    /// Get the underlying value (0 for no limit, otherwise the actual limit).
    pub fn get(self) -> usize {
        self.0.unwrap_or(0)
    }

    /// Check if this represents "no limit" (value was 0).
    pub fn is_unlimited(self) -> bool {
        self.0.is_none()
    }

    /// Get the effective limit for comparisons.
    /// Returns usize::MAX for unlimited, otherwise the actual limit.
    pub fn effective_limit(self) -> usize {
        self.0.unwrap_or(usize::MAX)
    }

    /// Convert from a default value (for use in config defaults).
    ///
    /// # Panics
    /// Never panics - accepts any value including 0.
    pub const fn from_const(value: usize) -> Self {
        if value == 0 { Self(None) } else { Self(Some(value)) }
    }
}

/// We don't need a separate error type since LineLength accepts all values.
/// The validation is implicit in the conversion.
impl<'de> Deserialize<'de> for LineLength {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = usize::deserialize(deserializer)?;
        Ok(LineLength::new(value))
    }
}

impl Serialize for LineLength {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.get().serialize(serializer)
    }
}

impl From<LineLength> for usize {
    fn from(val: LineLength) -> Self {
        val.get()
    }
}

impl fmt::Display for LineLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unlimited() {
            write!(f, "unlimited")
        } else {
            write!(f, "{}", self.get())
        }
    }
}

impl Default for LineLength {
    fn default() -> Self {
        Self::from_const(80) // Default line length is 80
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_values() {
        // Test 0 (unlimited)
        let unlimited = LineLength::new(0);
        assert_eq!(unlimited.get(), 0);
        assert!(unlimited.is_unlimited());
        assert_eq!(unlimited.effective_limit(), usize::MAX);
        assert_eq!(usize::from(unlimited), 0);

        // Test positive values
        for value in [1, 2, 80, 100, 120, 1000] {
            let limited = LineLength::new(value);
            assert_eq!(limited.get(), value);
            assert!(!limited.is_unlimited());
            assert_eq!(limited.effective_limit(), value);
            assert_eq!(usize::from(limited), value);
        }
    }

    #[test]
    fn test_from_const() {
        const UNLIMITED: LineLength = LineLength::from_const(0);
        assert_eq!(UNLIMITED.get(), 0);
        assert!(UNLIMITED.is_unlimited());

        const LIMITED: LineLength = LineLength::from_const(80);
        assert_eq!(LIMITED.get(), 80);
        assert!(!LIMITED.is_unlimited());
    }

    #[test]
    fn test_display() {
        let unlimited = LineLength::new(0);
        assert_eq!(format!("{unlimited}"), "unlimited");

        let limited = LineLength::new(80);
        assert_eq!(format!("{limited}"), "80");
    }

    #[test]
    fn test_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct TestConfig {
            value: LineLength,
        }

        // Test unlimited (0)
        let config = TestConfig {
            value: LineLength::new(0),
        };
        let serialized = toml::to_string(&config).unwrap();
        assert_eq!(serialized.trim(), "value = 0");
        let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.value.get(), 0);
        assert!(deserialized.value.is_unlimited());

        // Test limited value
        let config = TestConfig {
            value: LineLength::new(100),
        };
        let serialized = toml::to_string(&config).unwrap();
        assert_eq!(serialized.trim(), "value = 100");
        let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.value.get(), 100);
        assert!(!deserialized.value.is_unlimited());
    }

    #[test]
    fn test_deserialization() {
        #[derive(Debug, serde::Deserialize)]
        struct TestConfig {
            value: LineLength,
        }

        // Test 0 (unlimited)
        let toml_str = "value = 0";
        let config: TestConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.value.get(), 0);
        assert!(config.value.is_unlimited());

        // Test positive values
        let toml_str = "value = 120";
        let config: TestConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.value.get(), 120);
        assert!(!config.value.is_unlimited());
    }
}
