use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

/// String interner for reducing memory allocations of common strings
#[derive(Debug)]
pub struct StringInterner {
    strings: HashMap<String, Arc<str>>,
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            strings: HashMap::new(),
        }
    }

    /// Intern a string, returning an `Arc<str>` that can be shared
    pub fn intern(&mut self, s: &str) -> Arc<str> {
        Arc::clone(self.strings.entry(s.to_string()).or_insert_with(|| Arc::from(s)))
    }

    /// Get the number of interned strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if the interner is empty
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

/// Global string interner for common patterns
static GLOBAL_INTERNER: LazyLock<Arc<Mutex<StringInterner>>> =
    LazyLock::new(|| Arc::new(Mutex::new(StringInterner::new())));

/// Intern a string globally
///
/// If the mutex is poisoned, returns a fresh Arc<str> without interning.
/// This ensures the library never panics due to mutex poisoning.
pub fn intern_string(s: &str) -> Arc<str> {
    match GLOBAL_INTERNER.lock() {
        Ok(mut interner) => interner.intern(s),
        Err(_) => Arc::from(s),
    }
}

/// Common interned strings for performance
pub mod common {
    use super::*;
    use std::sync::LazyLock;

    // Rule names
    pub static MD001: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD001"));
    pub static MD002: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD002"));
    pub static MD003: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD003"));
    pub static MD004: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD004"));
    pub static MD005: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD005"));
    pub static MD006: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD006"));
    pub static MD007: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD007"));
    pub static MD009: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD009"));
    pub static MD010: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD010"));
    pub static MD013: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD013"));
    pub static MD034: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("MD034"));

    // Common messages
    pub static TRAILING_SPACES: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("Trailing spaces found"));
    pub static HARD_TABS: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("Hard tabs found"));
    pub static LINE_TOO_LONG: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("Line length exceeds limit"));
    pub static BARE_URL: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("Bare URL found"));

    // Common patterns
    pub static EMPTY_STRING: LazyLock<Arc<str>> = LazyLock::new(|| intern_string(""));
    pub static SPACE: LazyLock<Arc<str>> = LazyLock::new(|| intern_string(" "));
    pub static NEWLINE: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("\n"));
    pub static HASH: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("#"));
    pub static ASTERISK: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("*"));
    pub static DASH: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("-"));
    pub static PLUS: LazyLock<Arc<str>> = LazyLock::new(|| intern_string("+"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_interner() {
        let mut interner = StringInterner::new();

        let s1 = interner.intern("hello");
        let s2 = interner.intern("hello");
        let s3 = interner.intern("world");

        // Same string should return the same Arc
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(!Arc::ptr_eq(&s1, &s3));

        assert_eq!(interner.len(), 2);
        assert!(!interner.is_empty());
    }

    #[test]
    fn test_global_interner() {
        let s1 = intern_string("test");
        let s2 = intern_string("test");

        assert!(Arc::ptr_eq(&s1, &s2));
    }
}
