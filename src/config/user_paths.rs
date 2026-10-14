//! Where a user's home directory and user-level config live.
//!
//! Every place that needs `~` goes through [`process_home`]: the user-config
//! fallback, the project-walk home boundary, `extends = "~/..."`, the language
//! server's boundary and `~/` in exclude/include patterns. They must agree, or
//! one of them looks for `~` where the others do not.
//!
//! The policy is pure and takes the platform and its inputs as arguments
//! ([`resolve_home`], [`user_config_candidates`]), so the Windows rules run in
//! tests on any host. [`process_home`] is the only function that reads the real
//! environment.
//!
//! # Home directory
//!
//! On Windows `std::env::home_dir` reads `USERPROFILE` (then the profile API)
//! and ignores `HOME`, but shells such as Git Bash and MSYS set `HOME` and mean
//! `~` by it. `HOME` is therefore honored there when it is a fully qualified
//! native Windows path, and ignored (with the reason recorded) otherwise: a
//! relative value cannot be anchored, and an MSYS path like `/c/Users/x` names
//! no directory a native process can open. Everywhere else the home is whatever
//! `std::env::home_dir` returns, as it always was.
//!
//! # User config locations
//!
//! 1. `<config dir>/rumdl/{.rumdl.toml, rumdl.toml, pyproject.toml}`, where the
//!    config dir is the platform's: `$XDG_CONFIG_HOME` or `~/.config` on Linux and
//!    macOS, Roaming AppData on Windows. `pyproject.toml` counts only with a rumdl
//!    section.
//! 2. Windows only: `<home>/.config/rumdl/{.rumdl.toml, rumdl.toml, pyproject.toml}`,
//!    the path every other platform's documentation names. On Linux and macOS
//!    step 1 already is that directory, and probing `~/.config` again would
//!    either duplicate it or, with `XDG_CONFIG_HOME` set elsewhere, change which
//!    file wins.
//! 3. `<home>/.rumdl.toml`, then `<home>/rumdl.toml`. `pyproject.toml` is not
//!    probed in the home directory: there it belongs to unrelated Python tooling.
//!
//! `XDG_CONFIG_HOME` is not consulted on Windows.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::discovery_trace::{HomeSource, IgnoredHomeReason};

/// The platform whose conventions apply. Only Windows differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Windows,
    Other,
}

impl Platform {
    pub(crate) fn current() -> Self {
        if cfg!(windows) { Self::Windows } else { Self::Other }
    }
}

/// What the home choice depends on, so a test can state it outright.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HomeInputs<'a> {
    pub platform: Platform,
    /// The raw `HOME` environment variable, if set.
    pub home_env: Option<&'a OsStr>,
    /// What `std::env::home_dir` reports (`USERPROFILE`/profile API on Windows).
    pub native_home: Option<&'a Path>,
}

/// A `HOME` value that was set but not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IgnoredHome {
    pub value: String,
    pub reason: IgnoredHomeReason,
}

/// The home directory and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedHome {
    pub path: PathBuf,
    pub source: HomeSource,
}

/// The outcome of the home choice. `home` is `None` only when nothing could be
/// resolved; the directory need not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HomeResolution {
    pub home: Option<ResolvedHome>,
    pub ignored: Option<IgnoredHome>,
    /// The native profile directory, set only on Windows when `HOME` won and names
    /// a different directory than the profile. User-level discovery falls back to
    /// it after the home's own locations, so a config that was found in the profile
    /// before `HOME` was honored keeps applying.
    pub profile_fallback: Option<PathBuf>,
}

/// Choose the home directory from `inputs`.
pub(crate) fn resolve_home(inputs: HomeInputs<'_>) -> HomeResolution {
    let native = |platform: Platform| {
        inputs.native_home.map(|path| ResolvedHome {
            path: path.to_path_buf(),
            source: match platform {
                Platform::Windows => HomeSource::UserProfile,
                Platform::Other => HomeSource::PlatformDefault,
            },
        })
    };

    if inputs.platform != Platform::Windows {
        return HomeResolution {
            home: native(inputs.platform),
            ignored: None,
            profile_fallback: None,
        };
    }

    let Some(raw) = inputs.home_env else {
        return HomeResolution {
            home: native(inputs.platform),
            ignored: None,
            profile_fallback: None,
        };
    };

    match classify_windows_home(raw) {
        Ok(path) => {
            let profile_fallback = inputs
                .native_home
                .filter(|profile| !same_windows_dir(&path, profile))
                .map(Path::to_path_buf);
            HomeResolution {
                home: Some(ResolvedHome {
                    path,
                    source: HomeSource::HomeEnv,
                }),
                ignored: None,
                profile_fallback,
            }
        }
        Err(reason) => HomeResolution {
            home: native(inputs.platform),
            ignored: Some(IgnoredHome {
                value: raw.to_string_lossy().into_owned(),
                reason,
            }),
            profile_fallback: None,
        },
    }
}

/// Whether two Windows directory paths name the same directory, judged from the
/// text alone (no filesystem access, so it runs on any host): case-insensitive,
/// `/` and `\` interchangeable, trailing separators and a verbatim prefix ignored.
fn same_windows_dir(a: &Path, b: &Path) -> bool {
    fn normalize(path: &Path) -> String {
        let text = path.to_string_lossy().replace('/', "\\");
        let text = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else if let Some(rest) = text.strip_prefix(r"\\?\") {
            rest.to_string()
        } else {
            text
        };
        text.trim_end_matches('\\').to_lowercase()
    }
    normalize(a) == normalize(b)
}

/// `Ok` with the path when `raw` is a usable native Windows home, else why not.
fn classify_windows_home(raw: &OsStr) -> Result<PathBuf, IgnoredHomeReason> {
    let Some(value) = raw.to_str() else {
        return Err(IgnoredHomeReason::NotUnicode);
    };
    if value.trim().is_empty() {
        return Err(IgnoredHomeReason::Empty);
    }
    if is_fully_qualified_windows_path(value) {
        return Ok(PathBuf::from(value));
    }
    if is_msys_drive_path(value) {
        return Err(IgnoredHomeReason::MsysStyle);
    }
    Err(IgnoredHomeReason::NotFullyQualified)
}

/// A path a native Windows process can use from any working directory:
/// `C:\x` or `C:/x`, a UNC path `\\server\share[\x]` (either separator), or a
/// verbatim path `\\?\C:\x` / `\\?\UNC\server\share`. Judged on the text, so the
/// answer is the same on every host.
fn is_fully_qualified_windows_path(value: &str) -> bool {
    const BACKSLASH: &[char] = &['\\'];
    const EITHER: &[char] = &['\\', '/'];
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        // Verbatim paths are passed to the OS unmodified, so only backslashes count.
        let verbatim_drive = is_drive_absolute(rest) && rest.as_bytes().get(2) == Some(&b'\\');
        let verbatim_unc = rest
            .strip_prefix(r"UNC\")
            .is_some_and(|unc| is_server_share(unc, BACKSLASH));
        return verbatim_drive || verbatim_unc;
    }
    let mut chars = value.chars();
    if chars.next().is_some_and(|c| EITHER.contains(&c)) && chars.next().is_some_and(|c| EITHER.contains(&c)) {
        let unc = &value[2..];
        // `\\.\` and `//./` name devices; `//?/` is not a verbatim prefix.
        let mut head = unc.chars();
        let device = matches!(head.next(), Some('.' | '?')) && head.next().is_none_or(|c| EITHER.contains(&c));
        return !device && is_server_share(unc, EITHER);
    }
    is_drive_absolute(value)
}

/// `X:\...` or `X:/...`.
fn is_drive_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// `server<sep>share` with both parts nonempty, optionally followed by more path.
fn is_server_share(value: &str, separators: &[char]) -> bool {
    let mut parts = value.split(separators);
    let server = parts.next().unwrap_or("");
    let share = parts.next().unwrap_or("");
    !server.is_empty() && !share.is_empty()
}

/// `/c` or `/c/...`: the drive-letter mount MSYS and Cygwin shells use.
fn is_msys_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && (bytes.len() == 2 || bytes[2] == b'/')
}

/// The home directory of this process, with the reasoning behind it.
pub(crate) fn process_home() -> HomeResolution {
    #[cfg(feature = "native")]
    {
        let home_env = std::env::var_os("HOME");
        let native_home = etcetera::home_dir().ok();
        resolve_home(HomeInputs {
            platform: Platform::current(),
            home_env: home_env.as_deref(),
            native_home: native_home.as_deref(),
        })
    }
    #[cfg(not(feature = "native"))]
    {
        HomeResolution {
            home: None,
            ignored: None,
            profile_fallback: None,
        }
    }
}

impl HomeResolution {
    /// The profile fallback that goes with `home_in_use`. The fallback belongs to the
    /// process's own resolved home: the loader forwards that home as a plain path
    /// once resolved, which must keep the fallback, while a caller that chose a
    /// different home (tests, the language server) has no profile to fall back to.
    pub(crate) fn profile_fallback_for(&self, home_in_use: Option<&Path>) -> Option<PathBuf> {
        match (home_in_use, &self.home) {
            (None, _) => self.profile_fallback.clone(),
            (Some(in_use), Some(resolved)) if in_use == resolved.path => self.profile_fallback.clone(),
            _ => None,
        }
    }
}

/// The profile directory that bounds project discovery next to `home_in_use`:
/// the process's profile fallback, when `home_in_use` is the process's own home.
pub(crate) fn profile_boundary_for(home_in_use: Option<&Path>) -> Option<PathBuf> {
    process_home().profile_fallback_for(home_in_use)
}

/// The home directory of this process, for callers that need only the path.
pub(crate) fn process_home_path() -> Option<PathBuf> {
    let resolution = process_home();
    if let Some(ignored) = &resolution.ignored {
        log::debug!("[rumdl-config] Ignoring HOME={:?}: {:?}", ignored.value, ignored.reason);
    }
    resolution.home.map(|home| home.path)
}

/// The platform's user config directory (`rumdl/` goes inside it): Roaming
/// AppData on Windows, `$XDG_CONFIG_HOME` or `~/.config` elsewhere. Independent
/// of `HOME`: setting `HOME` must not move `%APPDATA%`.
pub(crate) fn platform_config_dir() -> Option<PathBuf> {
    #[cfg(feature = "native")]
    {
        use etcetera::{BaseStrategy, choose_base_strategy};
        match choose_base_strategy() {
            Ok(strategy) => Some(strategy.config_dir()),
            Err(e) => {
                log::debug!("[rumdl-config] Failed to determine user config directory: {e}");
                None
            }
        }
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
}

/// One place a user-level config may live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UserCandidate {
    pub path: PathBuf,
    /// `pyproject.toml` counts only when it declares a rumdl section.
    pub requires_rumdl_section: bool,
    /// Whether this location belongs to the profile fallback tier rather than the
    /// resolved home; the trace labels it so the user can see why it applied.
    pub profile_fallback: bool,
}

/// The user-level config locations, highest priority first. See the module docs.
pub(crate) fn user_config_candidates(
    platform: Platform,
    platform_config_dir: Option<&Path>,
    home: Option<&Path>,
    profile_fallback: Option<&Path>,
) -> Vec<UserCandidate> {
    let in_rumdl_dir = |base: &Path, fallback: bool, out: &mut Vec<UserCandidate>| {
        let dir = base.join("rumdl");
        for name in [".rumdl.toml", "rumdl.toml", "pyproject.toml"] {
            out.push(UserCandidate {
                path: dir.join(name),
                requires_rumdl_section: name == "pyproject.toml",
                profile_fallback: fallback,
            });
        }
    };
    let dotfiles = |base: &Path, fallback: bool, out: &mut Vec<UserCandidate>| {
        for name in [".rumdl.toml", "rumdl.toml"] {
            out.push(UserCandidate {
                path: base.join(name),
                requires_rumdl_section: false,
                profile_fallback: fallback,
            });
        }
    };

    let mut candidates = Vec::new();
    if let Some(dir) = platform_config_dir {
        in_rumdl_dir(dir, false, &mut candidates);
    }
    if platform == Platform::Windows
        && let Some(home) = home
    {
        in_rumdl_dir(&home.join(".config"), false, &mut candidates);
    }
    if let Some(home) = home {
        dotfiles(home, false, &mut candidates);
    }
    if platform == Platform::Windows
        && let Some(profile) = profile_fallback
    {
        in_rumdl_dir(&profile.join(".config"), true, &mut candidates);
        dotfiles(profile, true, &mut candidates);
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SourcedConfig;
    use crate::config::discovery_trace::ProbeOutcome;
    use std::fs;
    use tempfile::tempdir;

    fn windows_home(home_env: Option<&str>, native: Option<&str>) -> HomeResolution {
        resolve_home(HomeInputs {
            platform: Platform::Windows,
            home_env: home_env.map(OsStr::new),
            native_home: native.map(Path::new),
        })
    }

    fn chosen(resolution: &HomeResolution) -> Option<(&Path, HomeSource)> {
        resolution.home.as_ref().map(|h| (h.path.as_path(), h.source))
    }

    const PROFILE: &str = r"C:\Users\profile";

    #[test]
    fn windows_honors_a_native_home_that_differs_from_the_profile() {
        for home in [
            r"D:\work\home",
            "D:/work/home",
            r"\\server\share\home",
            r"\\server\share",
            "//server/share/home",
            "//server/share",
            r"\\server/share\home",
            r"\\?\D:\work\home",
            r"\\?\UNC\server\share\home",
        ] {
            let resolution = windows_home(Some(home), Some(PROFILE));
            assert_eq!(
                chosen(&resolution),
                Some((Path::new(home), HomeSource::HomeEnv)),
                "HOME={home} must win"
            );
            assert_eq!(resolution.ignored, None, "HOME={home} is valid, nothing to report");
        }
    }

    #[test]
    fn windows_ignores_a_home_that_is_not_fully_qualified_and_says_why() {
        let cases = [
            ("", IgnoredHomeReason::Empty),
            ("   ", IgnoredHomeReason::Empty),
            ("/c/Users/x", IgnoredHomeReason::MsysStyle),
            ("/c", IgnoredHomeReason::MsysStyle),
            ("/home/x", IgnoredHomeReason::NotFullyQualified),
            ("/", IgnoredHomeReason::NotFullyQualified),
            ("relative/dir", IgnoredHomeReason::NotFullyQualified),
            (".", IgnoredHomeReason::NotFullyQualified),
            ("~", IgnoredHomeReason::NotFullyQualified),
            (r"C:foo", IgnoredHomeReason::NotFullyQualified),
            ("C:", IgnoredHomeReason::NotFullyQualified),
            (r"\Users\x", IgnoredHomeReason::NotFullyQualified),
            (r"\\server", IgnoredHomeReason::NotFullyQualified),
            (r"\\server\", IgnoredHomeReason::NotFullyQualified),
            ("//server", IgnoredHomeReason::NotFullyQualified),
            ("//server/", IgnoredHomeReason::NotFullyQualified),
            ("//?/C:/x", IgnoredHomeReason::NotFullyQualified),
            ("//./C:/x", IgnoredHomeReason::NotFullyQualified),
            (r"\\?\UNC/server/share", IgnoredHomeReason::NotFullyQualified),
            (r"\\?\relative", IgnoredHomeReason::NotFullyQualified),
            (r"\\?\UNC\server", IgnoredHomeReason::NotFullyQualified),
            (r"\\.\C:\dev", IgnoredHomeReason::NotFullyQualified),
        ];
        for (home, reason) in cases {
            let resolution = windows_home(Some(home), Some(PROFILE));
            assert_eq!(
                chosen(&resolution),
                Some((Path::new(PROFILE), HomeSource::UserProfile)),
                "HOME={home:?} must fall back to the profile"
            );
            assert_eq!(
                resolution.ignored,
                Some(IgnoredHome {
                    value: home.to_string(),
                    reason
                }),
                "HOME={home:?}"
            );
        }
    }

    #[test]
    fn windows_without_home_uses_the_profile_and_reports_nothing() {
        let resolution = windows_home(None, Some(PROFILE));
        assert_eq!(chosen(&resolution), Some((Path::new(PROFILE), HomeSource::UserProfile)));
        assert_eq!(resolution.ignored, None);
    }

    #[test]
    fn windows_resolves_nothing_when_neither_source_is_usable_but_still_reports_the_ignored_home() {
        let resolution = windows_home(Some("/c/Users/x"), None);
        assert_eq!(resolution.home, None);
        assert_eq!(resolution.ignored.map(|i| i.reason), Some(IgnoredHomeReason::MsysStyle));
        assert_eq!(windows_home(None, None).home, None);
    }

    #[test]
    fn windows_home_need_not_exist() {
        let resolution = windows_home(Some(r"Z:\definitely\not\here"), None);
        assert_eq!(
            chosen(&resolution),
            Some((Path::new(r"Z:\definitely\not\here"), HomeSource::HomeEnv))
        );
    }

    #[test]
    fn other_platforms_keep_the_native_home_and_never_read_home_env() {
        // On Unix `native_home` already is HOME; the resolver must not second-guess it,
        // so a HOME that would be invalid on Windows changes nothing.
        let resolution = resolve_home(HomeInputs {
            platform: Platform::Other,
            home_env: Some(OsStr::new("/c/Users/x")),
            native_home: Some(Path::new("/home/dev")),
        });
        assert_eq!(
            chosen(&resolution),
            Some((Path::new("/home/dev"), HomeSource::PlatformDefault))
        );
        assert_eq!(resolution.ignored, None);

        let none = resolve_home(HomeInputs {
            platform: Platform::Other,
            home_env: Some(OsStr::new("/somewhere")),
            native_home: None,
        });
        assert_eq!(none.home, None, "no native home means no home, whatever HOME says");
    }

    #[cfg(unix)]
    #[test]
    fn windows_rejects_a_home_that_is_not_unicode_with_an_explicit_reason() {
        use std::os::unix::ffi::OsStrExt;
        let raw = OsStr::from_bytes(b"C:\\Users\\caf\xe9");
        let resolution = resolve_home(HomeInputs {
            platform: Platform::Windows,
            home_env: Some(raw),
            native_home: Some(Path::new(PROFILE)),
        });
        assert_eq!(chosen(&resolution), Some((Path::new(PROFILE), HomeSource::UserProfile)));
        assert_eq!(
            resolution.ignored.map(|i| i.reason),
            Some(IgnoredHomeReason::NotUnicode),
            "an invalid-unicode HOME is reported, not treated as unset"
        );
    }

    fn paths(candidates: &[UserCandidate]) -> Vec<String> {
        candidates
            .iter()
            .map(|c| c.path.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn windows_candidate_order_is_appdata_then_home_config_then_home_dotfiles() {
        let candidates =
            user_config_candidates(Platform::Windows, Some(Path::new("/roam")), Some(Path::new("/h")), None);
        assert_eq!(
            paths(&candidates),
            [
                "/roam/rumdl/.rumdl.toml",
                "/roam/rumdl/rumdl.toml",
                "/roam/rumdl/pyproject.toml",
                "/h/.config/rumdl/.rumdl.toml",
                "/h/.config/rumdl/rumdl.toml",
                "/h/.config/rumdl/pyproject.toml",
                "/h/.rumdl.toml",
                "/h/rumdl.toml",
            ]
        );
        let needs_section: Vec<bool> = candidates.iter().map(|c| c.requires_rumdl_section).collect();
        assert_eq!(needs_section, [false, false, true, false, false, true, false, false]);
    }

    #[test]
    fn other_platforms_do_not_probe_home_dot_config_even_when_xdg_points_elsewhere() {
        // XDG_CONFIG_HOME=/xdg makes the platform dir /xdg. A `~/.config` probe would
        // pick up a file the user never exposed to rumdl before.
        let candidates = user_config_candidates(Platform::Other, Some(Path::new("/xdg")), Some(Path::new("/h")), None);
        assert_eq!(
            paths(&candidates),
            [
                "/xdg/rumdl/.rumdl.toml",
                "/xdg/rumdl/rumdl.toml",
                "/xdg/rumdl/pyproject.toml",
                "/h/.rumdl.toml",
                "/h/rumdl.toml",
            ]
        );
    }

    #[test]
    fn candidates_degrade_when_a_directory_is_unknown() {
        assert_eq!(
            paths(&user_config_candidates(
                Platform::Windows,
                None,
                Some(Path::new("/h")),
                None
            )),
            [
                "/h/.config/rumdl/.rumdl.toml",
                "/h/.config/rumdl/rumdl.toml",
                "/h/.config/rumdl/pyproject.toml",
                "/h/.rumdl.toml",
                "/h/rumdl.toml",
            ]
        );
        assert_eq!(
            paths(&user_config_candidates(
                Platform::Windows,
                Some(Path::new("/roam")),
                None,
                None
            )),
            [
                "/roam/rumdl/.rumdl.toml",
                "/roam/rumdl/rumdl.toml",
                "/roam/rumdl/pyproject.toml",
            ]
        );
        assert!(user_config_candidates(Platform::Windows, None, None, None).is_empty());
    }

    /// The rule a tier disables: a distinct, valid rule name per tier.
    fn marker(index: usize) -> String {
        format!("MD{:03}", index + 1)
    }

    /// A user config tier laid out on disk, each marking the setting it carries.
    struct Layout {
        root: tempfile::TempDir,
    }

    const TIERS: [&str; 13] = [
        "roam/rumdl/.rumdl.toml",
        "roam/rumdl/rumdl.toml",
        "roam/rumdl/pyproject.toml",
        "home/.config/rumdl/.rumdl.toml",
        "home/.config/rumdl/rumdl.toml",
        "home/.config/rumdl/pyproject.toml",
        "home/.rumdl.toml",
        "home/rumdl.toml",
        "profile/.config/rumdl/.rumdl.toml",
        "profile/.config/rumdl/rumdl.toml",
        "profile/.config/rumdl/pyproject.toml",
        "profile/.rumdl.toml",
        "profile/rumdl.toml",
    ];

    impl Layout {
        fn new() -> Self {
            Self {
                root: tempdir().unwrap(),
            }
        }

        /// Write tier `index` so that loading it disables exactly `MD0{index+1}1`.
        fn write(&self, index: usize) {
            let path = self.root.path().join(TIERS[index]);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let marker = marker(index);
            let content = if TIERS[index].ends_with("pyproject.toml") {
                format!("[tool.rumdl.global]\ndisable = [\"{marker}\"]\n")
            } else {
                format!("[global]\ndisable = [\"{marker}\"]\n")
            };
            fs::write(path, content).unwrap();
        }

        fn candidates(&self, platform: Platform) -> Vec<UserCandidate> {
            user_config_candidates(
                platform,
                Some(&self.root.path().join("roam")),
                Some(&self.root.path().join("home")),
                Some(&self.root.path().join("profile")),
            )
        }

        fn load(&self, platform: Platform) -> SourcedConfig {
            let mut sourced = SourcedConfig::default();
            SourcedConfig::load_user_candidates(&mut sourced, &self.candidates(platform)).unwrap();
            sourced
        }
    }

    fn disabled(sourced: &SourcedConfig) -> Vec<String> {
        let mut rules = sourced.global.disable.value.clone();
        rules.sort();
        rules
    }

    #[test]
    fn windows_policy_picks_the_first_existing_tier_for_every_combination() {
        // Every subset of the thirteen locations (home and profile fallback included):
        // the winner is always the first one present in the documented order, and
        // nothing else contributes.
        // A Gray-code walk changes one file per step, so one directory serves all of them.
        let layout = Layout::new();
        for step in 0u32..(1 << TIERS.len()) {
            let mask = step ^ (step >> 1);
            if step > 0 {
                let flipped = (step.trailing_zeros()) as usize;
                if mask & (1 << flipped) != 0 {
                    layout.write(flipped);
                } else {
                    fs::remove_file(layout.root.path().join(TIERS[flipped])).unwrap();
                }
            }
            let present: Vec<usize> = (0..TIERS.len()).filter(|i| mask & (1 << i) != 0).collect();
            let sourced = layout.load(Platform::Windows);
            match present.first() {
                Some(&winner) => assert_eq!(
                    disabled(&sourced),
                    [marker(winner)],
                    "present tiers {present:?}: tier {winner} ({}) must win alone",
                    TIERS[winner]
                ),
                None => assert!(disabled(&sourced).is_empty(), "no file, no settings"),
            }
        }
    }

    #[test]
    fn other_platforms_never_reach_the_home_dot_config_tier() {
        // Only ~/.config/rumdl exists; the platform dir is elsewhere and empty.
        let layout = Layout::new();
        layout.write(4);
        assert!(disabled(&layout.load(Platform::Other)).is_empty());
        assert_eq!(disabled(&layout.load(Platform::Windows)), [marker(4)]);

        // With the dotfile also present, the non-Windows result is the dotfile.
        layout.write(7);
        assert_eq!(disabled(&layout.load(Platform::Other)), [marker(7)]);
    }

    #[test]
    fn other_platforms_never_reach_the_profile_fallback_tier() {
        let layout = Layout::new();
        for index in 8..TIERS.len() {
            layout.write(index);
        }
        assert!(disabled(&layout.load(Platform::Other)).is_empty());
        assert_eq!(disabled(&layout.load(Platform::Windows)), [marker(8)]);
    }

    #[test]
    fn a_pyproject_without_a_rumdl_section_never_wins_and_is_reported() {
        let layout = Layout::new();
        let appdata_pyproject = layout.root.path().join(TIERS[2]);
        fs::create_dir_all(appdata_pyproject.parent().unwrap()).unwrap();
        fs::write(&appdata_pyproject, "[project]\nname = \"x\"\n").unwrap();
        let home_pyproject = layout.root.path().join(TIERS[5]);
        fs::create_dir_all(home_pyproject.parent().unwrap()).unwrap();
        fs::write(&home_pyproject, "[tool.black]\nline-length = 100\n").unwrap();
        layout.write(7);

        let sourced = layout.load(Platform::Windows);
        assert_eq!(
            disabled(&sourced),
            [marker(7)],
            "both ineligible pyprojects are skipped"
        );

        let outcomes: Vec<_> = sourced
            .discovery_trace
            .events()
            .iter()
            .filter_map(|e| match e {
                crate::config::discovery_trace::TraceEvent::Probe { path, outcome, .. } => {
                    Some((path.replace('\\', "/"), outcome.clone()))
                }
                _ => None,
            })
            .collect();
        let outcome_of = |suffix: &str| {
            outcomes
                .iter()
                .find(|(p, _)| p.ends_with(suffix))
                .unwrap_or_else(|| panic!("no probe for {suffix}: {outcomes:?}"))
                .1
                .clone()
        };
        assert_eq!(
            outcome_of("roam/rumdl/pyproject.toml"),
            ProbeOutcome::IneligiblePyproject
        );
        assert_eq!(
            outcome_of("home/.config/rumdl/pyproject.toml"),
            ProbeOutcome::IneligiblePyproject
        );
        assert_eq!(outcome_of("home/rumdl.toml"), ProbeOutcome::Found);
        assert_eq!(outcome_of("roam/rumdl/.rumdl.toml"), ProbeOutcome::Missing);
    }

    #[test]
    fn an_eligible_pyproject_in_the_home_config_dir_wins_over_the_home_dotfile() {
        let layout = Layout::new();
        layout.write(5);
        layout.write(6);
        assert_eq!(disabled(&layout.load(Platform::Windows)), [marker(5)]);
    }

    #[test]
    fn an_unreadable_pyproject_is_an_access_error_not_a_missing_file() {
        let layout = Layout::new();
        // A directory where the file belongs: it can be inspected but not read.
        fs::create_dir_all(layout.root.path().join(TIERS[2])).unwrap();
        layout.write(7);

        let sourced = layout.load(Platform::Windows);
        assert_eq!(disabled(&sourced), [marker(7)]);
        let outcome = sourced
            .discovery_trace
            .events()
            .iter()
            .find_map(|e| match e {
                crate::config::discovery_trace::TraceEvent::Probe { path, outcome, .. }
                    if path.replace('\\', "/").ends_with("roam/rumdl/pyproject.toml") =>
                {
                    Some(outcome.clone())
                }
                _ => None,
            })
            .expect("the unreadable pyproject is probed");
        assert!(
            matches!(outcome, ProbeOutcome::AccessError { .. }),
            "must not read as missing: {outcome:?}"
        );
    }

    #[test]
    fn the_winner_records_the_lower_priority_files_it_shadows() {
        let layout = Layout::new();
        layout.write(1);
        layout.write(4);
        layout.write(6);
        let sourced = layout.load(Platform::Windows);
        let shadows: Vec<_> = sourced
            .discovery_trace
            .events()
            .iter()
            .filter_map(|e| match e {
                crate::config::discovery_trace::TraceEvent::Selected { shadows, .. } => {
                    Some(shadows.iter().map(|s| s.replace('\\', "/")).collect::<Vec<_>>())
                }
                _ => None,
            })
            .collect();
        assert_eq!(shadows.len(), 1);
        assert_eq!(shadows[0].len(), 2);
        assert!(shadows[0][0].ends_with("home/.config/rumdl/rumdl.toml"), "{shadows:?}");
        assert!(shadows[0][1].ends_with("home/.rumdl.toml"), "{shadows:?}");
    }

    fn windows_candidates(home_env: &str, profile: &str) -> (HomeResolution, Vec<UserCandidate>) {
        let resolution = windows_home(Some(home_env), Some(profile));
        let candidates = user_config_candidates(
            Platform::Windows,
            Some(Path::new("/roam")),
            resolution.home.as_ref().map(|h| h.path.as_path()),
            resolution.profile_fallback.as_deref(),
        );
        (resolution, candidates)
    }

    #[test]
    fn a_home_that_differs_from_the_profile_adds_the_profile_tiers_last() {
        let (resolution, candidates) = windows_candidates("D:/home", "C:/prof");
        assert_eq!(resolution.profile_fallback.as_deref(), Some(Path::new("C:/prof")));
        assert_eq!(
            paths(&candidates),
            [
                "/roam/rumdl/.rumdl.toml",
                "/roam/rumdl/rumdl.toml",
                "/roam/rumdl/pyproject.toml",
                "D:/home/.config/rumdl/.rumdl.toml",
                "D:/home/.config/rumdl/rumdl.toml",
                "D:/home/.config/rumdl/pyproject.toml",
                "D:/home/.rumdl.toml",
                "D:/home/rumdl.toml",
                "C:/prof/.config/rumdl/.rumdl.toml",
                "C:/prof/.config/rumdl/rumdl.toml",
                "C:/prof/.config/rumdl/pyproject.toml",
                "C:/prof/.rumdl.toml",
                "C:/prof/rumdl.toml",
            ]
        );
        let fallback: Vec<bool> = candidates.iter().map(|c| c.profile_fallback).collect();
        assert_eq!(fallback[..8], [false; 8]);
        assert_eq!(fallback[8..], [true; 5]);
        let needs_section: Vec<bool> = candidates[8..].iter().map(|c| c.requires_rumdl_section).collect();
        assert_eq!(needs_section, [false, false, true, false, false]);
    }

    #[test]
    fn the_profile_tiers_are_not_listed_when_home_is_the_profile_in_any_spelling() {
        let profile = r"C:\Users\Prof";
        for home in [
            r"C:\Users\Prof",
            r"C:\Users\Prof\",
            r"C:\Users\Prof\\",
            "C:/Users/Prof",
            "C:/Users/Prof/",
            r"c:\users\prof",
            r"C:\USERS\PROF",
            r"C:/users\Prof/",
            r"\\?\C:\Users\Prof",
        ] {
            let (resolution, candidates) = windows_candidates(home, profile);
            assert_eq!(resolution.profile_fallback, None, "HOME={home}");
            assert_eq!(candidates.len(), 8, "HOME={home}: {:?}", paths(&candidates));
            assert!(candidates.iter().all(|c| !c.profile_fallback), "HOME={home}");
        }
        let unc = r"\\srv\share\me";
        for home in [unc, "//srv/share/me/", r"\\SRV\Share\me\", r"\\?\UNC\srv\share\me"] {
            let (resolution, _) = windows_candidates(home, unc);
            assert_eq!(resolution.profile_fallback, None, "HOME={home}");
        }
    }

    #[test]
    fn directories_that_only_look_alike_are_not_the_profile() {
        for home in [r"C:\Users\Prof2", r"C:\Users\Prof\sub", r"D:\Users\Prof", r"C:\Users"] {
            let (resolution, candidates) = windows_candidates(home, r"C:\Users\Prof");
            assert_eq!(
                resolution.profile_fallback.as_deref(),
                Some(Path::new(r"C:\Users\Prof")),
                "HOME={home}"
            );
            assert_eq!(candidates.len(), 13, "HOME={home}");
        }
    }

    #[test]
    fn there_is_no_profile_fallback_without_a_home_that_won_or_a_profile() {
        // No HOME, an ignored HOME, and a missing profile all leave one home only.
        assert_eq!(windows_home(None, Some(PROFILE)).profile_fallback, None);
        assert_eq!(windows_home(Some("/c/Users/x"), Some(PROFILE)).profile_fallback, None);
        assert_eq!(windows_home(Some(r"D:\h"), None).profile_fallback, None);
        let other = resolve_home(HomeInputs {
            platform: Platform::Other,
            home_env: Some(OsStr::new("/elsewhere")),
            native_home: Some(Path::new("/home/dev")),
        });
        assert_eq!(other.profile_fallback, None);
    }

    #[test]
    fn the_profile_fallback_follows_the_resolved_home_but_not_a_different_one() {
        let resolution = windows_home(Some(r"D:\home"), Some(r"C:\prof"));
        let resolved = Path::new(r"D:\home");
        let profile = Some(PathBuf::from(r"C:\prof"));
        assert_eq!(resolution.profile_fallback_for(None), profile);
        assert_eq!(
            resolution.profile_fallback_for(Some(resolved)),
            profile,
            "the resolved home forwarded as a path keeps its fallback"
        );
        assert_eq!(
            resolution.profile_fallback_for(Some(Path::new(r"E:\elsewhere"))),
            None,
            "a caller-chosen home has no profile fallback"
        );
        let unresolved = windows_home(None, None);
        assert_eq!(unresolved.profile_fallback_for(Some(resolved)), None);
    }

    #[test]
    fn the_trace_labels_the_profile_tier_in_text_and_json() {
        let layout = Layout::new();
        layout.write(10);
        layout.write(11);
        let sourced = layout.load(Platform::Windows);
        assert_eq!(disabled(&sourced), [marker(10)]);

        let text = sourced.discovery_trace.render_text();
        let selected = text.lines().find(|l| l.starts_with("selected ")).expect(&text);
        assert!(selected.contains("profile fallback"), "{text}");
        assert!(
            selected
                .replace('\\', "/")
                .contains("profile/.config/rumdl/pyproject.toml"),
            "{text}"
        );
        let labeled = text.lines().filter(|l| l.contains("profile fallback")).count();
        assert_eq!(labeled, 5 + 1, "five probes and the selection carry the label: {text}");
        assert!(
            !text
                .lines()
                .any(|l| l.contains("home/") && l.contains("profile fallback")),
            "home tiers are never labeled: {text}"
        );

        let json: serde_json::Value = serde_json::from_str(&sourced.discovery_trace.to_json()).unwrap();
        let events = json["events"].as_array().unwrap();
        let flagged = |event: &str| {
            events
                .iter()
                .filter(|e| e["event"] == event && e["profile_fallback"] == true)
                .count()
        };
        assert_eq!(flagged("probe"), 5);
        assert_eq!(flagged("selected"), 1);
        let unflagged_probes = events
            .iter()
            .filter(|e| e["event"] == "probe" && e.get("profile_fallback").is_none())
            .count();
        assert_eq!(unflagged_probes, 8, "home-side probes carry no label");
    }

    #[test]
    fn a_home_tier_wins_over_the_profile_tier_and_the_profile_file_is_reported_as_shadowed() {
        let layout = Layout::new();
        layout.write(7);
        layout.write(11);
        let sourced = layout.load(Platform::Windows);
        assert_eq!(disabled(&sourced), [marker(7)]);
        let events = sourced.discovery_trace.events();
        let (shadows, fallback) = events
            .iter()
            .find_map(|e| match e {
                crate::config::discovery_trace::TraceEvent::Selected {
                    shadows,
                    profile_fallback,
                    ..
                } => Some((shadows.clone(), *profile_fallback)),
                _ => None,
            })
            .unwrap();
        assert!(!fallback, "the home file is not a fallback");
        assert_eq!(shadows.len(), 1, "{shadows:?}");
        assert!(
            shadows[0].replace('\\', "/").ends_with("profile/.rumdl.toml"),
            "{shadows:?}"
        );
    }

    #[test]
    fn the_profile_fallback_wins_alone_and_is_labeled_when_home_has_no_config() {
        let layout = Layout::new();
        layout.write(11);
        let sourced = layout.load(Platform::Windows);
        assert_eq!(disabled(&sourced), [marker(11)]);
        assert!(sourced.discovery_trace.events().iter().any(|e| matches!(
            e,
            crate::config::discovery_trace::TraceEvent::Selected {
                profile_fallback: true,
                ..
            }
        )));
    }
}
