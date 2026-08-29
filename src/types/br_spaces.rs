use serde::{Deserialize, Serialize};

/// Number of trailing spaces that MD009 accepts as a hard line break.
///
/// CommonMark renders a hard line break for two or more trailing spaces, so a
/// value below 2 cannot describe one. Such a value turns the exception off
/// instead: every trailing space is then reported. This mirrors markdownlint,
/// where `br_spaces` of 0 or 1 "disallows any trailing spaces".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BrSpaces(usize);

impl BrSpaces {
    /// Fewest trailing spaces that render a hard line break (CommonMark).
    pub const MIN: usize = 2;

    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// The configured value as written.
    pub fn get(self) -> usize {
        self.0
    }

    /// The trailing-space count kept as a hard line break, or `None` when the
    /// exception is off and every trailing space is reported.
    pub fn line_break(self) -> Option<usize> {
        (self.0 >= Self::MIN).then_some(self.0)
    }
}

impl Default for BrSpaces {
    fn default() -> Self {
        Self(Self::MIN)
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
    fn test_line_break_values() {
        for value in [2, 3, 4, 10, 100] {
            let br_spaces = BrSpaces::new(value);
            assert_eq!(br_spaces.get(), value);
            assert_eq!(usize::from(br_spaces), value);
            assert_eq!(br_spaces.line_break(), Some(value));
        }
    }

    #[test]
    fn test_values_below_two_turn_the_exception_off() {
        for value in [0, 1] {
            let br_spaces = BrSpaces::new(value);
            assert_eq!(br_spaces.get(), value);
            assert_eq!(br_spaces.line_break(), None);
        }
    }

    #[test]
    fn test_default() {
        assert_eq!(BrSpaces::default().get(), 2);
        assert_eq!(BrSpaces::default().line_break(), Some(2));
    }

    #[test]
    fn test_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct TestConfig {
            spaces: BrSpaces,
        }

        for value in [0, 1, 2, 3] {
            let config = TestConfig {
                spaces: BrSpaces::new(value),
            };
            let serialized = toml::to_string(&config).unwrap();
            assert_eq!(serialized, format!("spaces = {value}\n"));
            let deserialized: TestConfig = toml::from_str(&serialized).unwrap();
            assert_eq!(deserialized.spaces.get(), value);
        }
    }
}
