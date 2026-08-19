//! Where the config loader gets file contents from.
//!
//! The loader follows an `extends` chain by reading one file after another. On
//! native builds that is the filesystem. The browser wasm build has no
//! filesystem, so the embedder fetches contents itself (Obsidian, for example,
//! reads its vault through an async adapter) and hands them over up front; the
//! loader then tells it which file the chain needs next. Both paths go through
//! [`ConfigFileSource`] so `extends` resolution and merge order are written once.

use std::path::{Path, PathBuf};

#[cfg(any(feature = "wasm", test))]
pub(crate) use in_memory::InMemoryConfigFiles;

/// A place the config loader reads config files and the values that locate
/// them (environment variables, the home directory) from.
pub(crate) trait ConfigFileSource {
    /// Whether a file exists at `path`.
    fn exists(&self, path: &Path) -> bool;

    /// The contents of the file at `path`.
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// One identity per file, so a file reached by two spellings is recognised
    /// as the same file when detecting `extends` cycles.
    fn canonicalize(&self, path: &Path) -> PathBuf;

    /// The value of environment variable `name`, for `$VAR` in `extends`.
    fn env_var(&self, name: &str) -> Option<String>;

    /// The home directory, for `~/` in `extends`. `None` leaves `~/` literal.
    fn home_dir(&self) -> Option<PathBuf>;
}

/// The process's own filesystem and environment.
pub(crate) struct FsConfigFiles;

impl ConfigFileSource for FsConfigFiles {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        #[cfg(feature = "native")]
        {
            use etcetera::{BaseStrategy, choose_base_strategy};
            choose_base_strategy().ok().map(|s| s.home_dir().to_path_buf())
        }
        #[cfg(not(feature = "native"))]
        {
            None
        }
    }
}

/// Only the wasm build has an embedder to supply contents; native builds read
/// the filesystem through [`FsConfigFiles`].
#[cfg(any(feature = "wasm", test))]
mod in_memory {
    use super::ConfigFileSource;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::{Component, Path, PathBuf};

    /// Contents an embedder supplied up front, keyed by the path the loader asks for.
    ///
    /// A path mapped to `None` is a file the embedder looked for and did not find,
    /// so the loader reports it missing exactly as it would on disk. A path absent
    /// from the map is one the embedder was never asked for: the first such path is
    /// recorded as [`needed`](Self::needed) and the load fails, so the caller can
    /// fetch that file, add it, and load again. Each round trip discovers the next
    /// link of the chain, which is what an embedder that can only read files
    /// asynchronously needs.
    pub(crate) struct InMemoryConfigFiles {
        files: HashMap<PathBuf, Option<String>>,
        env: HashMap<String, String>,
        home: Option<PathBuf>,
        needed: RefCell<Option<PathBuf>>,
    }

    impl InMemoryConfigFiles {
        pub(crate) fn new(
            files: impl IntoIterator<Item = (String, Option<String>)>,
            env: HashMap<String, String>,
            home: Option<PathBuf>,
        ) -> Self {
            Self {
                files: files
                    .into_iter()
                    .map(|(path, content)| (lexically_normalized(Path::new(&path)), content))
                    .collect(),
                env,
                home,
                needed: RefCell::new(None),
            }
        }

        /// The first path the loader asked for that the embedder has not supplied,
        /// if the load stopped because of one. The loader's own error for that
        /// attempt is then just "not found" and should be disregarded. The path is
        /// returned normalized (`docs/../base.toml` as `base.toml`), which is also
        /// the key it is looked up under, so the embedder can store it as given.
        pub(crate) fn needed(&self) -> Option<PathBuf> {
            self.needed.borrow().clone()
        }

        fn lookup(&self, path: &Path) -> Option<&Option<String>> {
            let key = lexically_normalized(path);
            let found = self.files.get(&key);
            if found.is_none() {
                let mut needed = self.needed.borrow_mut();
                if needed.is_none() {
                    *needed = Some(key);
                }
            }
            found
        }
    }

    impl ConfigFileSource for InMemoryConfigFiles {
        fn exists(&self, path: &Path) -> bool {
            matches!(self.lookup(path), Some(Some(_)))
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            match self.lookup(path) {
                Some(Some(content)) => Ok(content.clone()),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{} was not provided", path.display()),
                )),
            }
        }

        fn canonicalize(&self, path: &Path) -> PathBuf {
            lexically_normalized(path)
        }

        fn env_var(&self, name: &str) -> Option<String> {
            self.env.get(name).cloned()
        }

        fn home_dir(&self) -> Option<PathBuf> {
            self.home.clone()
        }
    }

    /// Collapse `.` and `..` segments without touching the filesystem, so
    /// `docs/../base.toml` and `base.toml` name the same entry. A `..` with nothing
    /// to climb out of is kept, so two distinct paths above the root stay distinct.
    fn lexically_normalized(path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    let climbable = matches!(out.components().next_back(), Some(Component::Normal(_)));
                    if climbable {
                        out.pop();
                    } else {
                        out.push(component);
                    }
                }
                other => out.push(other),
            }
        }
        if out.as_os_str().is_empty() {
            out.push(".");
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn normalizes_dot_segments_lexically() {
            assert_eq!(
                lexically_normalized(Path::new("docs/../base.toml")),
                PathBuf::from("base.toml")
            );
            assert_eq!(
                lexically_normalized(Path::new("./a/./b.toml")),
                PathBuf::from("a/b.toml")
            );
            assert_eq!(
                lexically_normalized(Path::new("a/b/../../c.toml")),
                PathBuf::from("c.toml")
            );
            assert_eq!(
                lexically_normalized(Path::new("/x/../y.toml")),
                PathBuf::from("/y.toml")
            );
        }

        #[test]
        fn keeps_unclimbable_parent_segments() {
            assert_eq!(
                lexically_normalized(Path::new("../base.toml")),
                PathBuf::from("../base.toml")
            );
            assert_eq!(
                lexically_normalized(Path::new("../../base.toml")),
                PathBuf::from("../../base.toml")
            );
            assert_eq!(
                lexically_normalized(Path::new("a/../../base.toml")),
                PathBuf::from("../base.toml")
            );
            assert_eq!(lexically_normalized(Path::new(".")), PathBuf::from("."));
        }

        #[test]
        fn in_memory_reports_the_first_unsupplied_path() {
            let files =
                InMemoryConfigFiles::new([(".rumdl.toml".to_string(), Some(String::new()))], HashMap::new(), None);
            assert!(files.exists(Path::new("./.rumdl.toml")));
            assert_eq!(files.needed(), None);

            assert!(!files.exists(Path::new("docs/../base/.rumdl.toml")));
            assert!(files.read_to_string(Path::new("other.toml")).is_err());
            assert_eq!(
                files.needed(),
                Some(PathBuf::from("base/.rumdl.toml")),
                "the first unsupplied path is reported, in its normalized form"
            );
        }

        #[test]
        fn in_memory_distinguishes_missing_from_unsupplied() {
            let files = InMemoryConfigFiles::new([("gone.toml".to_string(), None)], HashMap::new(), None);
            assert!(!files.exists(Path::new("gone.toml")));
            assert!(files.read_to_string(Path::new("gone.toml")).is_err());
            assert_eq!(
                files.needed(),
                None,
                "a file the embedder reported missing is not a request for it"
            );
        }
    }
}
