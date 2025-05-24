use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;

/// String interner for reducing memory allocations of common strings
#[derive(Debug)]
pub struct StringInterner {
    strings: HashMap<String, Arc<str>>,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            strings: HashMap::new(),
        }
    }

    /// Intern a string, returning an Arc<str> that can be shared
    pub fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(interned) = self.strings.get(s) {
            interned.clone()
        } else {
            let arc_str: Arc<str> = Arc::from(s);
            self.strings.insert(s.to_string(), arc_str.clone());
            arc_str
        }
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

lazy_static! {
    /// Global string interner for common patterns
    static ref GLOBAL_INTERNER: Arc<Mutex<StringInterner>> = Arc::new(Mutex::new(StringInterner::new()));
}

/// Intern a string globally
pub fn intern_string(s: &str) -> Arc<str> {
    let mut interner = GLOBAL_INTERNER.lock().unwrap();
    interner.intern(s)
}

/// Common interned strings for performance
pub mod common {
    use super::*;
    use lazy_static::lazy_static;

    lazy_static! {
        // Rule names
        pub static ref MD001: Arc<str> = intern_string("MD001");
        pub static ref MD002: Arc<str> = intern_string("MD002");
        pub static ref MD003: Arc<str> = intern_string("MD003");
        pub static ref MD004: Arc<str> = intern_string("MD004");
        pub static ref MD005: Arc<str> = intern_string("MD005");
        pub static ref MD006: Arc<str> = intern_string("MD006");
        pub static ref MD007: Arc<str> = intern_string("MD007");
        pub static ref MD009: Arc<str> = intern_string("MD009");
        pub static ref MD010: Arc<str> = intern_string("MD010");
        pub static ref MD013: Arc<str> = intern_string("MD013");
        pub static ref MD034: Arc<str> = intern_string("MD034");

        // Common messages
        pub static ref TRAILING_SPACES: Arc<str> = intern_string("Trailing spaces found");
        pub static ref HARD_TABS: Arc<str> = intern_string("Hard tabs found");
        pub static ref LINE_TOO_LONG: Arc<str> = intern_string("Line length exceeds limit");
        pub static ref BARE_URL: Arc<str> = intern_string("Bare URL found");

        // Common patterns
        pub static ref EMPTY_STRING: Arc<str> = intern_string("");
        pub static ref SPACE: Arc<str> = intern_string(" ");
        pub static ref NEWLINE: Arc<str> = intern_string("\n");
        pub static ref HASH: Arc<str> = intern_string("#");
        pub static ref ASTERISK: Arc<str> = intern_string("*");
        pub static ref DASH: Arc<str> = intern_string("-");
        pub static ref PLUS: Arc<str> = intern_string("+");
    }
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