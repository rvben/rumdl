//! Locating a tool binary the way the process spawner would.
//!
//! Availability is answered in-process rather than by spawning `which` or
//! `where`. Neither finder is part of any platform contract (a minimal container
//! image routinely lacks `which`), and a spawn that fails because the finder is
//! missing is indistinguishable from the tool being missing: every configured
//! tool then reports "not found in PATH" while sitting on the PATH. The rules
//! here mirror what `Command::new` itself resolves, so a tool reported present is
//! one that will spawn and one reported missing is one that would not have.

use std::ffi::OsStr;
#[cfg(any(unix, windows))]
use std::path::Path;
use std::path::PathBuf;

/// Where `Command::new(program)` would find `program`, or `None` if it would not.
///
/// `search_path` is the `PATH` value to search, normally the environment's; it is
/// a parameter so the lookup can be exercised against a controlled directory.
///
/// On unix this follows `execvp`: a name containing a slash is taken as a path
/// and never searched for, anything else is looked up in each `PATH` entry in
/// order (an empty entry meaning the current directory), and a match must be a
/// regular file with an execute bit. When `PATH` is unset the libc default
/// `/usr/bin:/bin` applies.
///
/// On windows this follows the standard library's own resolution. A name with a
/// path separator is never searched for: one ending in `.exe` is used as written,
/// any other is tried with `.exe` appended to the name as written and then as
/// written. A bare name is searched in the directory of the running executable,
/// the system directory, the Windows directory and then each `PATH` entry, in
/// that order, with `.exe` appended when the name contains no `.` at all; the
/// working directory is not searched. `PATHEXT` is deliberately not consulted,
/// because `Command::new` does not consult it either: a `tool.cmd` shim is not
/// something the spawn would find under the bare name.
pub fn resolve_program(program: &OsStr, search_path: Option<&OsStr>) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    resolve_for_platform(program, search_path)
}

#[cfg(unix)]
fn resolve_for_platform(program: &OsStr, search_path: Option<&OsStr>) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;

    if program.as_bytes().contains(&b'/') {
        let path = Path::new(program);
        return is_executable_file(path).then(|| path.to_path_buf());
    }

    let default_path = OsStr::new("/usr/bin:/bin");
    let search_path = search_path.unwrap_or(default_path);
    std::env::split_paths(search_path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable_file(candidate))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn resolve_for_platform(program: &OsStr, search_path: Option<&OsStr>) -> Option<PathBuf> {
    use std::ffi::OsString;

    let bytes = program.as_encoded_bytes();
    if bytes.ends_with(b"\\") || bytes.ends_with(b"/") {
        return None;
    }
    let has_separator = bytes.contains(&b'\\') || bytes.contains(&b'/');

    if has_separator {
        let path = Path::new(program);
        let has_exe_suffix = bytes.len() >= 4 && bytes[bytes.len() - 4..].eq_ignore_ascii_case(b".exe");
        if has_exe_suffix {
            return path.is_file().then(|| path.to_path_buf());
        }
        // `.exe` is appended to the name as written (`dir\tool.cmd` is tried as
        // `dir\tool.cmd.exe`), then the name as written is used.
        let mut with_exe: OsString = program.to_os_string();
        with_exe.push(".exe");
        let with_exe = PathBuf::from(with_exe);
        if with_exe.is_file() {
            return Some(with_exe);
        }
        return path.is_file().then(|| path.to_path_buf());
    }

    // A bare name gets `.exe` only when it has no extension at all, and any `.`
    // counts as one: `tool.v2` is looked up as written.
    let has_extension = bytes.contains(&b'.');

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        dirs.push(dir);
    }
    dirs.extend(windows_system_directories());
    if let Some(search_path) = search_path {
        dirs.extend(std::env::split_paths(search_path).filter(|dir| !dir.as_os_str().is_empty()));
    }

    dirs.into_iter().find_map(|dir| {
        let mut candidate = dir.join(program);
        if !has_extension {
            candidate.set_extension("exe");
        }
        candidate.is_file().then_some(candidate)
    })
}

/// The system directory and the Windows directory, asked of the system the way
/// the spawner asks (`GetSystemDirectoryW`, then `GetWindowsDirectoryW`) rather
/// than read from `SystemRoot`, so the answer holds in a process whose
/// environment lacks or rewrites that variable.
#[cfg(windows)]
fn windows_system_directories() -> Vec<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};

    // Each call reports the length written, or the length it needs (counting
    // the terminating NUL) when the buffer is too small, or 0 on failure.
    let query = |get: unsafe extern "system" fn(*mut u16, u32) -> u32| -> Option<PathBuf> {
        let mut buf = vec![0u16; 260];
        loop {
            // SAFETY: `buf` is a live, writable buffer of `buf.len()` UTF-16
            // units and that length is what the call is told it may write.
            let len = unsafe { get(buf.as_mut_ptr(), u32::try_from(buf.len()).ok()?) } as usize;
            if len == 0 {
                return None;
            }
            if len < buf.len() {
                buf.truncate(len);
                return Some(PathBuf::from(OsString::from_wide(&buf)));
            }
            buf.resize(len, 0);
        }
    };
    [GetSystemDirectoryW, GetWindowsDirectoryW]
        .into_iter()
        .filter_map(query)
        .collect()
}

#[cfg(not(any(unix, windows)))]
fn resolve_for_platform(_program: &OsStr, _search_path: Option<&OsStr>) -> Option<PathBuf> {
    // No process spawning on this platform, so no tool can be run.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::ffi::OsString;

    #[cfg(unix)]
    fn write_tool(dir: &Path, name: &str, executable: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn empty_program_name_never_resolves() {
        assert_eq!(resolve_program(OsStr::new(""), Some(OsStr::new("/usr/bin"))), None);
    }

    #[cfg(unix)]
    #[test]
    fn tool_on_a_path_without_which_resolves() {
        // Issue #820: the search must not depend on a `which` binary. The PATH
        // handed in holds only the temp dir, so there is no `which` anywhere the
        // lookup can see, and the tool must still be found.
        let dir = tempfile::tempdir().unwrap();
        let tool = write_tool(dir.path(), "shellcheck", true);
        assert_eq!(
            resolve_program(OsStr::new("shellcheck"), Some(dir.path().as_os_str())),
            Some(tool)
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_without_execute_bit_does_not_resolve() {
        // Same name, same directory, only the mode differs: the negative control
        // for the test above.
        let dir = tempfile::tempdir().unwrap();
        write_tool(dir.path(), "shellcheck", false);
        assert_eq!(
            resolve_program(OsStr::new("shellcheck"), Some(dir.path().as_os_str())),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn absent_tool_does_not_resolve() {
        let dir = tempfile::tempdir().unwrap();
        write_tool(dir.path(), "shellcheck", true);
        assert_eq!(resolve_program(OsStr::new("shfmt"), Some(dir.path().as_os_str())), None);
    }

    #[cfg(unix)]
    #[test]
    fn directory_named_like_the_tool_does_not_resolve() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("shellcheck")).unwrap();
        assert_eq!(
            resolve_program(OsStr::new("shellcheck"), Some(dir.path().as_os_str())),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn first_path_entry_wins() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let expected = write_tool(first.path(), "ruff", true);
        write_tool(second.path(), "ruff", true);
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();
        assert_eq!(resolve_program(OsStr::new("ruff"), Some(&path)), Some(expected));
    }

    #[cfg(unix)]
    #[test]
    fn a_name_with_a_slash_is_a_path_and_is_not_searched() {
        // `Command::new("bin/tool")` spawns relative to the working directory and
        // never consults PATH; the lookup must agree, or a tool that will not
        // spawn is reported present.
        let on_path = tempfile::tempdir().unwrap();
        write_tool(on_path.path(), "tool", true);
        let elsewhere = tempfile::tempdir().unwrap();
        let absolute = write_tool(elsewhere.path(), "tool", true);

        assert_eq!(
            resolve_program(absolute.as_os_str(), Some(on_path.path().as_os_str())),
            Some(absolute.clone())
        );
        let missing: OsString = elsewhere.path().join("bin").join("tool").into();
        assert_eq!(resolve_program(&missing, Some(on_path.path().as_os_str())), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_path_to_a_non_executable_file_does_not_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let plain = write_tool(dir.path(), "tool", false);
        assert_eq!(resolve_program(plain.as_os_str(), None), None);
    }

    #[cfg(unix)]
    #[test]
    fn unset_path_falls_back_to_the_libc_default() {
        // `sh` lives in /bin on every unix; a tool that exists nowhere does not.
        assert!(resolve_program(OsStr::new("sh"), None).is_some());
        assert_eq!(resolve_program(OsStr::new("rumdl-no-such-tool-820"), None), None);
    }

    #[cfg(windows)]
    #[test]
    fn bare_name_resolves_to_exe_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("shellcheck.exe");
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(
            resolve_program(OsStr::new("shellcheck"), Some(dir.path().as_os_str())),
            Some(exe.clone())
        );
        assert_eq!(
            resolve_program(OsStr::new("shellcheck.exe"), Some(dir.path().as_os_str())),
            Some(exe)
        );
    }

    #[cfg(windows)]
    #[test]
    fn cmd_shim_is_not_found_under_the_bare_name() {
        // `Command::new("tool")` appends `.exe`, never `.cmd`, so a shim that only
        // `where` would report must not count as available.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tool.cmd"), b"").unwrap();
        assert_eq!(resolve_program(OsStr::new("tool"), Some(dir.path().as_os_str())), None);
        let shim = dir.path().join("tool.cmd");
        assert_eq!(resolve_program(shim.as_os_str(), None), Some(shim));
    }

    #[cfg(windows)]
    #[test]
    fn absent_tool_does_not_resolve() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shellcheck.exe"), b"").unwrap();
        assert_eq!(resolve_program(OsStr::new("shfmt"), Some(dir.path().as_os_str())), None);
    }

    #[cfg(windows)]
    #[test]
    fn any_dot_in_a_bare_name_is_its_extension() {
        // The spawner appends `.exe` only to a name with no `.` at all, so
        // `tool.v2` is looked up as written: it finds a file of that exact name
        // and never `tool.v2.exe`.
        let dir = tempfile::tempdir().unwrap();
        let as_written = dir.path().join("tool.v2");
        std::fs::write(&as_written, b"").unwrap();
        assert_eq!(
            resolve_program(OsStr::new("tool.v2"), Some(dir.path().as_os_str())),
            Some(as_written)
        );

        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("tool.v2.exe"), b"").unwrap();
        assert_eq!(
            resolve_program(OsStr::new("tool.v2"), Some(other.path().as_os_str())),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_sub_path_tries_exe_appended_and_then_the_name_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("tool");
        let exe = dir.path().join("tool.exe");
        std::fs::write(&bare, b"").unwrap();
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(resolve_program(bare.as_os_str(), None), Some(exe.clone()));
        std::fs::remove_file(&exe).unwrap();
        assert_eq!(resolve_program(bare.as_os_str(), None), Some(bare));

        // `.exe` is appended to the name as written, not swapped for its
        // extension: `dir\tool.cmd` is tried as `dir\tool.cmd.exe`.
        let shim = dir.path().join("tool.cmd");
        let shim_exe = dir.path().join("tool.cmd.exe");
        std::fs::write(&shim, b"").unwrap();
        std::fs::write(&shim_exe, b"").unwrap();
        assert_eq!(resolve_program(shim.as_os_str(), None), Some(shim_exe));

        // A name already ending in `.exe` is used as written and nothing else
        // is tried for it.
        let missing = dir.path().join("absent.exe");
        assert_eq!(resolve_program(missing.as_os_str(), None), None);
        let trailing = dir.path().join("tool\\");
        assert_eq!(resolve_program(trailing.as_os_str(), None), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_system_directory_is_searched_before_path() {
        // `cmd` lives in the system directory, which the spawner searches before
        // `PATH`, so a `cmd.exe` on `PATH` never shadows it and an empty `PATH`
        // still finds it.
        let system32 = windows_system_directories().into_iter().next().unwrap();
        assert!(system32.join("cmd.exe").is_file(), "{system32:?} lacks cmd.exe");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cmd.exe"), b"").unwrap();
        assert_eq!(
            resolve_program(OsStr::new("cmd"), Some(dir.path().as_os_str())),
            Some(system32.join("cmd.exe"))
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_program(OsStr::new("cmd"), Some(empty.path().as_os_str())),
            Some(system32.join("cmd.exe"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_working_directory_is_not_searched() {
        // A tool that exists only in the working directory would not spawn under
        // its bare name, so it must not be reported present. The positive
        // control puts the same directory on `PATH`.
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("rumdl-lookup-probe.exe");
        std::fs::write(&tool, b"").unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let from_cwd = resolve_program(OsStr::new("rumdl-lookup-probe"), Some(elsewhere.path().as_os_str()));
        let from_path = resolve_program(OsStr::new("rumdl-lookup-probe"), Some(dir.path().as_os_str()));
        std::env::set_current_dir(previous).unwrap();
        assert_eq!(from_cwd, None);
        assert_eq!(from_path, Some(tool));
    }
}
