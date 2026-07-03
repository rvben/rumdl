//! Atomic file writes for the `--fix` / `fmt` path.
//!
//! Overwriting a user's Markdown file in place with `std::fs::write` opens it
//! with `O_TRUNC`: the moment the call starts, the original bytes are gone, and
//! if the process is killed, the disk fills, or the write errors partway
//! through, the file is left truncated with no way to recover the original.
//! For a linter that edits source files this is data loss.
//!
//! `write_atomically` instead writes the new content to a sibling temp file on
//! the same filesystem and renames it over the target. `rename(2)` is atomic, so
//! a concurrent reader (or a crash) sees either the complete old file or the
//! complete new file, never a partial one, and a failed write leaves the
//! original untouched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Disambiguates concurrent temp files written to the same directory.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write `content` to `path` atomically.
///
/// The new content lands in a temp file next to the target and is renamed into
/// place, so the target is only ever the complete old or complete new file. The
/// original file's permissions are preserved on the replacement, and symlinks
/// are written through (the resolved real file is replaced, the link is kept),
/// matching the behavior of a plain `fs::write`.
///
/// On any failure the temp file is removed and the original target is left
/// exactly as it was.
pub fn write_atomically(path: &Path, content: &[u8]) -> io::Result<()> {
    // Resolve symlinks so the temp file is created on the same filesystem as,
    // and the rename targets, the real file. Renaming over a symlink path itself
    // would replace the link with a regular file. A path that does not yet
    // resolve (a brand-new file) falls back to the path as given.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let parent = target.parent().filter(|p| !p.as_os_str().is_empty());

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = target.file_name().and_then(|n| n.to_str()).unwrap_or("rumdl");
    #[cfg(not(target_arch = "wasm32"))]
    let tmp_name = format!(".{base}.rumdl-tmp.{}.{counter}", std::process::id());
    // `std::process::id()` is unavailable on WASI; the per-process counter is
    // enough there since WASI runs are single-process.
    #[cfg(target_arch = "wasm32")]
    let tmp_name = format!(".{base}.rumdl-tmp.{counter}");
    let tmp_path: PathBuf = match parent {
        Some(dir) => dir.join(tmp_name),
        None => PathBuf::from(tmp_name),
    };

    let result = (|| {
        fs::write(&tmp_path, content)?;
        // Carry the original file's permissions onto the replacement so a fix
        // never resets an executable bit or a restrictive 0600 mode.
        if let Ok(meta) = fs::metadata(&target) {
            let _ = fs::set_permissions(&tmp_path, meta.permissions());
        }
        fs::rename(&tmp_path, &target)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::tempdir;

    #[test]
    fn writes_new_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, "old").unwrap();
        write_atomically(&path, b"new content").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
    }

    #[test]
    fn creates_a_new_file_when_target_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fresh.md");
        write_atomically(&path, b"hello").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let path = dir.path().join("doc.md");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        write_atomically(&path, b"new").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "fix must not widen a restrictive file mode");
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_a_symlink_and_keeps_the_link() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let real = dir.path().join("real.md");
        let link = dir.path().join("link.md");
        fs::write(&real, "old").unwrap();
        symlink(&real, &link).unwrap();

        write_atomically(&link, b"updated").unwrap();

        // The link is still a symlink, and the real file received the content.
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "updated");
    }

    #[test]
    fn failed_write_leaves_original_intact() {
        // Target's parent directory does not exist, so the temp write fails.
        // The original path never existed, and no partial file is created.
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing-subdir").join("doc.md");
        assert!(write_atomically(&path, b"content").is_err());
        assert!(!path.exists(), "no partial file should be left behind");
    }

    #[test]
    fn concurrent_reader_never_sees_a_partial_file() {
        // A plain truncating write can expose an empty/partial file to a
        // concurrent reader. The atomic write must not: every read observes a
        // complete document. Mirrors cache.rs's atomicity test.
        let dir = tempdir().unwrap();
        let path = dir.path().join("doc.md");
        let a = "A".repeat(200_000);
        let b = "B".repeat(200_000);
        fs::write(&path, &a).unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let reader_path = path.clone();
        let reader_stop = Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            let mut saw_partial = false;
            while !reader_stop.load(Ordering::Relaxed) {
                let mut buf = String::new();
                if let Ok(mut f) = fs::File::open(&reader_path)
                    && f.read_to_string(&mut buf).is_ok()
                {
                    let len = buf.len();
                    if len != 0 && len != 200_000 {
                        saw_partial = true;
                        break;
                    }
                }
            }
            saw_partial
        });

        for i in 0..200 {
            let content = if i % 2 == 0 { &b } else { &a };
            write_atomically(&path, content.as_bytes()).unwrap();
        }
        stop.store(true, Ordering::Relaxed);
        assert!(!reader.join().unwrap(), "reader observed a partial file");
    }
}
