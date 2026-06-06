use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Target text column for MD030's `ol-align-column` setting: `0` (off) or `3..=6`.
///
/// Ordered list text is aligned to this column, measured from the start of the
/// marker. The range is not a style preference but a CommonMark constraint: at most
/// 4 spaces may follow a list marker (5 or more start an indented code block), and
/// the narrowest marker (`1.`) is 2 columns wide, so text can only land between
/// column 3 (`1.` + 1 space) and column 6 (`1.` + 4 spaces). `0` disables alignment.
///
/// Validated at construction and during config deserialization, so an out-of-range
/// value is rejected with a clear error rather than silently degrading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct OlAlignColumn(usize);

impl OlAlignColumn {
    /// Create a target column, validating it is `0` (off) or within `3..=6`.
    ///
    /// # Errors
    /// Returns `OlAlignColumnError` for any other value.
    pub fn new(column: usize) -> Result<Self, OlAlignColumnError> {
        if column == 0 || (3..=6).contains(&column) {
            Ok(Self(column))
        } else {
            Err(OlAlignColumnError(column))
        }
    }

    /// The raw column value (`0` when alignment is off).
    pub fn get(self) -> usize {
        self.0
    }

    /// The target column when alignment is enabled, or `None` when off (`0`).
    pub fn enabled(self) -> Option<usize> {
        (self.0 > 0).then_some(self.0)
    }
}

/// Error type for an out-of-range `ol-align-column` value.
#[derive(Debug, Clone, Copy)]
pub struct OlAlignColumnError(usize);

impl fmt::Display for OlAlignColumnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ol-align-column must be 0 (off) or between 3 and 6, got {}. \
             CommonMark allows at most 4 spaces after an ordered list marker \
             (5 or more start an indented code block), so text can only align to \
             columns 3-6 measured from the marker start.",
            self.0
        )
    }
}

impl std::error::Error for OlAlignColumnError {}

impl<'de> Deserialize<'de> for OlAlignColumn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let column = usize::deserialize(deserializer)?;
        OlAlignColumn::new(column).map_err(serde::de::Error::custom)
    }
}

impl Serialize for OlAlignColumn {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_off() {
        assert_eq!(OlAlignColumn::default().get(), 0);
        assert_eq!(OlAlignColumn::default().enabled(), None);
    }

    #[test]
    fn test_valid_values() {
        assert_eq!(OlAlignColumn::new(0).unwrap().enabled(), None);
        for column in 3..=6 {
            let c = OlAlignColumn::new(column).unwrap();
            assert_eq!(c.get(), column);
            assert_eq!(c.enabled(), Some(column));
        }
    }

    #[test]
    fn test_out_of_range_values_rejected() {
        for column in [1, 2, 7, 8, 40, usize::MAX] {
            assert!(OlAlignColumn::new(column).is_err(), "{column} should be rejected");
        }
    }

    #[test]
    fn test_deserialize_rejects_out_of_range() {
        #[derive(Debug, Deserialize)]
        struct TestConfig {
            column: OlAlignColumn,
        }

        let err = toml::from_str::<TestConfig>("column = 7").unwrap_err().to_string();
        assert!(
            err.contains("must be 0 (off) or between 3 and 6") || err.contains("got 7"),
            "unexpected error: {err}"
        );

        // Valid values deserialize cleanly.
        assert_eq!(toml::from_str::<TestConfig>("column = 4").unwrap().column.get(), 4);
        assert_eq!(toml::from_str::<TestConfig>("column = 0").unwrap().column.get(), 0);
    }
}
