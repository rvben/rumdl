//! abstract syntax trees: [unist][].
//!
//! [unist]: https://github.com/syntax-tree/unist

use alloc::fmt;

/// One place in a source file.
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    /// 1-indexed integer representing a line in a source file.
    pub line: usize,
    /// 1-indexed integer representing a column in a source file.
    pub column: usize,
    /// 0-indexed integer representing a character in a source file.
    pub offset: usize,
}

impl Point {
    #[must_use]
    pub fn new(line: usize, column: usize, offset: usize) -> Point {
        Point { line, column, offset }
    }
}

impl fmt::Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} ({})", self.line, self.column, self.offset)
    }
}

/// Location of a node in a source file.
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(), derive(serde::Serialize, serde::Deserialize))]
pub struct Position {
    /// Represents the place of the first character of the parsed source region.
    pub start: Point,
    /// Represents the place of the first character after the parsed source
    /// region, whether it exists or not.
    pub end: Point,
}

impl Position {
    #[must_use]
    pub fn new(
        start_line: usize,
        start_column: usize,
        start_offset: usize,
        end_line: usize,
        end_column: usize,
        end_offset: usize,
    ) -> Position {
        Position {
            start: Point::new(start_line, start_column, start_offset),
            end: Point::new(end_line, end_column, end_offset),
        }
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}-{}:{} ({}-{})",
            self.start.line, self.start.column, self.end.line, self.end.column, self.start.offset, self.end.offset
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn point() {
        let point = Point::new(1, 1, 0);
        assert_eq!(
            format!("{:?}", point),
            "1:1 (0)",
            "should support `Debug` on unist points"
        );
    }

    #[test]
    fn position() {
        let position = Position::new(1, 1, 0, 1, 3, 2);
        assert_eq!(
            format!("{:?}", position),
            "1:1-1:3 (0-2)",
            "should support `Debug` on unist positions"
        );
    }
}
