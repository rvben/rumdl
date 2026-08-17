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
/// On windows this follows the standard library's own resolution: a name with a
/// path separator is used as written, trying `.exe` first when it has no
/// extension; a bare name is searched in the directory of the running executable,
/// the current directory and then each `PATH` entry, with `.exe` appended when the
/// name has no extension. `PATHEXT` is deliberately not consulted, because
/// `Command::new` does not consult it either: a `tool.cmd` shim is not something
/// the spawn would find under the bare name.
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
    let program_path = Path::new(program);
    let has_separator = program.to_string_lossy().contains(['\\', '/']);
    let has_extension = program_path.extension().is_some();

    if has_separator {
        if !has_extension {
            let with_exe = program_path.with_extension("exe");
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
        return program_path.is_file().then(|| program_path.to_path_buf());
    }

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        dirs.push(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    if let Some(search_path) = search_path {
        dirs.extend(std::env::split_paths(search_path).filter(|dir| !dir.as_os_str().is_empty()));
    }

    dirs.into_iter()
        .map(|dir| {
            let mut candidate = dir.join(program);
            if !has_extension {
                candidate.set_extension("exe");
            }
            candidate
        })
        .find(|candidate| candidate.is_file())
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
}
