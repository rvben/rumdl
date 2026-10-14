//! Source identities already resolved by native file discovery.
//!
//! A command can share successful canonicalizations with its filesystem-aware
//! rules. Only selected source files are covered; link targets still observe
//! the filesystem normally. Library and virtual callers need no scope.

use std::cell::RefCell;
use std::collections::HashSet;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

thread_local! {
    static SOURCES: RefCell<Option<Arc<HashSet<PathBuf>>>> = const { RefCell::new(None) };
}

/// Successful source-file canonicalizations made during this command's file
/// selection. Do not include fallback paths from a failed canonicalization.
pub struct CanonicalSourcePaths(Arc<HashSet<PathBuf>>);

impl CanonicalSourcePaths {
    pub fn new(paths: HashSet<PathBuf>) -> Self {
        Self(Arc::new(paths))
    }

    /// Make the selected identities available on this worker until the guard
    /// drops. Nested scopes restore their caller's set, including on unwind.
    pub fn enter(&self) -> SourcePathScope {
        SourcePathScope {
            previous: SOURCES.with_borrow_mut(|sources| sources.replace(Arc::clone(&self.0))),
            _thread: PhantomData,
        }
    }
}

/// A worker-local source identity scope. Keep it alive during file processing.
#[must_use]
pub struct SourcePathScope {
    previous: Option<Arc<HashSet<PathBuf>>>,
    // A scope must restore the same worker on which it was entered.
    _thread: PhantomData<Rc<()>>,
}

impl Drop for SourcePathScope {
    fn drop(&mut self) {
        SOURCES.with_borrow_mut(|sources| *sources = self.previous.take());
    }
}

pub(crate) fn canonicalize_source(path: &Path) -> std::io::Result<PathBuf> {
    let known = SOURCES.with_borrow(|sources| sources.as_ref().is_some_and(|sources| sources.contains(path)));
    if known {
        Ok(path.to_path_buf())
    } else {
        path.canonicalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_restore_after_nesting_and_unwind() {
        let temp = tempfile::tempdir().unwrap();
        let outer_path = temp.path().join("outer.md");
        let inner_path = temp.path().join("inner.md");
        // Missing paths let the test distinguish a selected identity from a
        // filesystem lookup, without relying on syscall timing.
        let outer = CanonicalSourcePaths::new(HashSet::from([outer_path.clone()]));
        let inner = CanonicalSourcePaths::new(HashSet::from([inner_path.clone()]));
        {
            let _outer = outer.enter();
            assert_eq!(canonicalize_source(&outer_path).unwrap(), outer_path);
            let result = std::panic::catch_unwind(|| {
                let _inner = inner.enter();
                assert_eq!(canonicalize_source(&inner_path).unwrap(), inner_path);
                assert!(canonicalize_source(&outer_path).is_err());
                panic!("unwind the inner scope");
            });
            assert!(result.is_err());
            assert_eq!(canonicalize_source(&outer_path).unwrap(), outer_path);
            assert!(canonicalize_source(&inner_path).is_err());
        }
        assert!(canonicalize_source(&outer_path).is_err());
    }

    #[test]
    fn scopes_do_not_leak_between_workers() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("selected.md");
        let sources = CanonicalSourcePaths::new(HashSet::from([path.clone()]));
        let _scope = sources.enter();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                assert!(canonicalize_source(&path).is_err());
                let _scope = sources.enter();
                assert_eq!(canonicalize_source(&path).unwrap(), path);
            });
        });
        assert_eq!(canonicalize_source(&path).unwrap(), path);
    }

    #[cfg(unix)]
    #[test]
    fn unselected_symlinks_are_resolved_fresh() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.md");
        let second = temp.path().join("second.md");
        let link = temp.path().join("source.md");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        symlink(&first, &link).unwrap();
        let first = first.canonicalize().unwrap();
        let second = second.canonicalize().unwrap();
        let sources = CanonicalSourcePaths::new(HashSet::from([first.clone()]));
        let _scope = sources.enter();
        assert_eq!(canonicalize_source(&link).unwrap(), first);
        std::fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();
        assert_eq!(canonicalize_source(&link).unwrap(), second);
    }
}
