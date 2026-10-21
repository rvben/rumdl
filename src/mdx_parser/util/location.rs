//! Deal with positions in a file.
//!
//! * Convert between byte indices and unist points.
//! * Convert between byte indices into a string which is built up of several
//!   slices in a whole document, and byte indices into that whole document.

use crate::mdx_parser::unist::Point;
use alloc::{vec, vec::Vec};

/// Each stop represents a new slice, which contains the byte index into the
/// corresponding string where the slice starts (`0`), and the byte index into
/// the whole document where that slice starts (`1`).
pub type Stop = (usize, usize);

#[derive(Debug)]
pub struct Location {
    /// List, where each index is a line number (0-based), and each value is
    /// the byte index *after* where the line ends.
    indices: Vec<usize>,
}

impl Location {
    /// Get an index for the given `bytes`.
    ///
    /// Port of <https://github.com/vfile/vfile-location/blob/main/index.js>
    #[must_use]
    pub fn new(bytes: &[u8]) -> Self {
        let mut index = 0;
        let mut location_index = Self { indices: vec![] };

        while index < bytes.len() {
            if bytes[index] == b'\r' {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    location_index.indices.push(index + 2);
                    index += 1;
                } else {
                    location_index.indices.push(index + 1);
                }
            } else if bytes[index] == b'\n' {
                location_index.indices.push(index + 1);
            }

            index += 1;
        }

        location_index.indices.push(index + 1);
        location_index
    }

    /// Get the line and column-based `point` for `offset` in the bound indices.
    ///
    /// Returns `None` when given out of bounds input.
    ///
    /// Port of <https://github.com/vfile/vfile-location/blob/main/index.js>
    #[must_use]
    pub fn to_point(&self, offset: usize) -> Option<Point> {
        let end = self.indices.last()?;

        if offset >= *end {
            return None;
        }

        let index = self.indices.partition_point(|start| *start <= offset);
        let previous = if index > 0 { self.indices[index - 1] } else { 0 };
        Some(Point::new(index + 1, offset + 1 - previous, offset))
    }

    /// Like `to_point`, but takes a relative offset from a certain string
    /// instead of an absolute offset into the whole document.
    ///
    /// The relative offset is made absolute based on `stops`, which represent
    /// where that certain string is in the whole document.
    #[must_use]
    pub fn relative_to_point(&self, stops: &[Stop], relative: usize) -> Option<Point> {
        Location::relative_to_absolute(stops, relative).and_then(|absolute| self.to_point(absolute))
    }

    /// Turn a relative offset into an absolute offset.
    #[must_use]
    pub fn relative_to_absolute(stops: &[Stop], relative: usize) -> Option<usize> {
        let mut index = 0;

        while index < stops.len() && stops[index].0 <= relative {
            index += 1;
        }

        // There are no points: that only occurs if there was an empty string.
        if index == 0 {
            None
        } else {
            let (stop_relative, stop_absolute) = &stops[index - 1];
            Some(stop_absolute + (relative - stop_relative))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_lf() {
        let location = Location::new("ab\nc".as_bytes());
        assert_eq!(
            location.to_point(0), // `a`
            Some(Point::new(1, 1, 0)),
            "should support some points (1)"
        );
        assert_eq!(
            location.to_point(1), // `b`
            Some(Point::new(1, 2, 1)),
            "should support some points (2)"
        );
        assert_eq!(
            location.to_point(2), // `\n`
            Some(Point::new(1, 3, 2)),
            "should support some points (3)"
        );
        assert_eq!(
            location.to_point(3), // `c`
            Some(Point::new(2, 1, 3)),
            "should support some points (4)"
        );
        assert_eq!(
            location.to_point(4), // EOF
            // Still gets a point, so that we can represent positions of things
            // that end at the last character, the `c` end at `2:2`.
            Some(Point::new(2, 2, 4)),
            "should support some points (5)"
        );
        assert_eq!(
            location.to_point(5), // Out of bounds
            None,
            "should support some points (6)"
        );
    }

    #[test]
    fn test_location_cr() {
        let location = Location::new("a\rb".as_bytes());
        assert_eq!(
            location.to_point(0), // `a`
            Some(Point::new(1, 1, 0)),
            "should support some points (1)"
        );
        assert_eq!(
            location.to_point(1), // `\r`
            Some(Point::new(1, 2, 1)),
            "should support some points (2)"
        );
        assert_eq!(
            location.to_point(2), // `b`
            Some(Point::new(2, 1, 2)),
            "should support some points (3)"
        );
    }

    #[test]
    fn test_location_cr_lf() {
        let location = Location::new("a\r\nb".as_bytes());
        assert_eq!(
            location.to_point(0), // `a`
            Some(Point::new(1, 1, 0)),
            "should support some points (1)"
        );
        assert_eq!(
            location.to_point(1), // `\r`
            Some(Point::new(1, 2, 1)),
            "should support some points (2)"
        );
        assert_eq!(
            location.to_point(2), // `\n`
            Some(Point::new(1, 3, 2)),
            "should support some points (3)"
        );
        assert_eq!(
            location.to_point(3), // `b`
            Some(Point::new(2, 1, 3)),
            "should support some points (4)"
        );
    }
    #[test]
    fn test_empty() {
        let location = Location::new("".as_bytes());
        assert_eq!(location.to_point(0), Some(Point::new(1, 1, 0)), "to_point");
        assert_eq!(location.relative_to_point(&[], 0), None, "relative_to_point");
        assert_eq!(Location::relative_to_absolute(&[], 0), None, "relative_to_absolute");
    }
}
