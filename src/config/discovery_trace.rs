//! A record of how configuration discovery reached its answer.
//!
//! Discovery decides in several places: which directory the walk started in,
//! which home directory bounds it, which candidate files exist, why the walk
//! stopped and which file won. Each of those places records its decision here
//! as it makes it, so the trace is the discovery that ran rather than a second
//! description of it that could drift. `rumdl config file --explain` renders the
//! trace, and every event is also written to the debug log as it is recorded.
//!
//! The trace names paths and the reasons discovery acted on them. It never holds
//! file contents, and it does not follow `extends`: the files a configuration
//! pulled in are answered by `SourcedConfig::loaded_files`.

use serde::Serialize;

use crate::utils::upward_walk::WalkStop;

/// Which part of discovery an event belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchStage {
    /// A config named with `--config`.
    Explicit,
    /// The upward walk for a rumdl project config.
    Project,
    /// The upward walk for a markdownlint config.
    Markdownlint,
    /// The user-level config locations.
    User,
}

impl SearchStage {
    fn heading(self) -> &'static str {
        match self {
            Self::Explicit => "Explicit config",
            Self::Project => "Project config search",
            Self::Markdownlint => "Markdownlint config search",
            Self::User => "User-level config search",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Explicit => "explicit config",
            Self::Project => "project config",
            Self::Markdownlint => "markdownlint config",
            Self::User => "user config",
        }
    }
}

/// What was found at one candidate path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeOutcome {
    Found,
    Missing,
    /// A `pyproject.toml` that exists but has no rumdl section.
    IneligiblePyproject,
    /// The path could not be inspected or read for a reason other than being
    /// absent (permissions, a directory where a file belongs, ...).
    AccessError {
        error: String,
    },
}

impl ProbeOutcome {
    fn text(&self) -> String {
        match self {
            Self::Found => "found".to_string(),
            Self::Missing => "missing".to_string(),
            Self::IneligiblePyproject => "ignored (pyproject.toml without a rumdl section)".to_string(),
            Self::AccessError { error } => format!("not usable ({error})"),
        }
    }
}

/// Where the home directory came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeSource {
    /// The `HOME` environment variable (honored on Windows when it is a native path).
    HomeEnv,
    /// `USERPROFILE`, then the user profile API (Windows).
    UserProfile,
    /// `HOME`, then the user database (everywhere but Windows).
    PlatformDefault,
    /// A directory the caller supplied (a language server or a test).
    Caller,
}

impl HomeSource {
    fn text(self) -> &'static str {
        match self {
            Self::HomeEnv => "HOME",
            Self::UserProfile => "USERPROFILE or the user profile API",
            Self::PlatformDefault => "HOME or the user database",
            Self::Caller => "supplied by the caller",
        }
    }
}

/// Why a `HOME` value was not used as the home directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoredHomeReason {
    Empty,
    NotUnicode,
    /// `/c/Users/x`, the form MSYS and Cygwin shells use for a Windows path.
    MsysStyle,
    /// Anything else that is not a fully qualified Windows path: relative,
    /// drive-relative (`C:foo`), rooted without a drive (`/home/x`).
    NotFullyQualified,
}

impl IgnoredHomeReason {
    fn text(self) -> &'static str {
        match self {
            Self::Empty => "it is empty",
            Self::NotUnicode => "it is not valid unicode",
            Self::MsysStyle => "it is an MSYS-style path (/c/Users/...), not a native Windows path",
            Self::NotFullyQualified => "it is not a fully qualified Windows path",
        }
    }
}

/// Why an upward walk ended without finding anything more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum WalkEnd {
    /// The walk reached the home directory, which is user-level and not searched as a project.
    HomeBoundary {
        path: String,
    },
    /// The walk reached the user profile directory, a home in its own right next to
    /// the resolved one, which is user-level and not searched as a project.
    ProfileBoundary {
        path: String,
    },
    /// The walk reached the root of a git repository.
    GitRoot {
        path: String,
    },
    /// The walk reached the directory it was told to stop at.
    StopRoot {
        path: String,
    },
    DepthCap,
    FilesystemRoot,
}

impl WalkEnd {
    fn text(&self) -> String {
        match self {
            Self::HomeBoundary { path } => {
                format!("reached the home directory boundary ({path}), which is not searched")
            }
            Self::ProfileBoundary { path } => {
                format!("reached the profile directory (fallback home) boundary ({path}), which is not searched")
            }
            Self::GitRoot { path } => format!("reached the git repository root ({path})"),
            Self::StopRoot { path } => format!("reached the search root ({path})"),
            Self::DepthCap => "reached the maximum search depth".to_string(),
            Self::FilesystemRoot => "reached the filesystem root".to_string(),
        }
    }
}

impl From<&WalkStop> for WalkEnd {
    fn from(stop: &WalkStop) -> Self {
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
        match stop {
            WalkStop::HomeBoundary(p) => Self::HomeBoundary { path: path(p) },
            WalkStop::ProfileBoundary(p) => Self::ProfileBoundary { path: path(p) },
            WalkStop::GitRoot(p) => Self::GitRoot { path: path(p) },
            WalkStop::StopRoot(p) => Self::StopRoot { path: path(p) },
            WalkStop::DepthCap => Self::DepthCap,
            WalkStop::FilesystemRoot => Self::FilesystemRoot,
        }
    }
}

/// Why a stage of discovery did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    ProjectConfigFound,
    ExplicitConfig,
    Isolated,
}

impl SkipReason {
    fn text(self) -> &'static str {
        match self {
            Self::ProjectConfigFound => "a project config was found",
            Self::ExplicitConfig => "--config was given",
            Self::Isolated => "--no-config/--isolated was given",
        }
    }
}

/// One decision discovery made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TraceEvent {
    StartDirectory {
        path: String,
    },
    Home {
        path: Option<String>,
        source: Option<HomeSource>,
    },
    IgnoredHome {
        value: String,
        reason: IgnoredHomeReason,
    },
    Probe {
        stage: SearchStage,
        path: String,
        #[serde(flatten)]
        outcome: ProbeOutcome,
        /// The location belongs to the profile fallback tier (Windows, when `HOME`
        /// names a different directory than the user profile).
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        profile_fallback: bool,
    },
    WalkEnded {
        stage: SearchStage,
        #[serde(flatten)]
        end: WalkEnd,
    },
    Skipped {
        stage: SearchStage,
        reason: SkipReason,
    },
    Selected {
        stage: SearchStage,
        path: String,
        /// Lower-priority candidates that exist and lost to `path`.
        shadows: Vec<String>,
        /// `path` came from the profile fallback tier.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        profile_fallback: bool,
    },
}

/// What the text rendering appends to a path that belongs to the profile fallback tier.
const PROFILE_FALLBACK_LABEL: &str = "profile fallback: HOME is set to a different directory";

impl TraceEvent {
    /// One line for the debug log and the text rendering.
    pub fn describe(&self) -> String {
        match self {
            Self::StartDirectory { path } => format!("start directory: {path}"),
            Self::Home {
                path: Some(path),
                source: Some(source),
            } => format!("home directory: {path} (from {})", source.text()),
            Self::Home { .. } => "home directory: not resolved".to_string(),
            Self::IgnoredHome { value, reason } => format!("ignored HOME={value:?}: {}", reason.text()),
            Self::Probe {
                path,
                outcome,
                profile_fallback,
                ..
            } => {
                let label = if *profile_fallback {
                    format!(" [{PROFILE_FALLBACK_LABEL}]")
                } else {
                    String::new()
                };
                format!("{path}: {}{label}", outcome.text())
            }
            Self::WalkEnded { end, .. } => format!("search ended: {}", end.text()),
            Self::Skipped { stage, reason } => {
                let what = match stage {
                    SearchStage::User => "user-level search",
                    SearchStage::Markdownlint => "markdownlint search",
                    SearchStage::Project => "project search",
                    SearchStage::Explicit => "explicit config",
                };
                format!("{what} skipped: {}", reason.text())
            }
            Self::Selected {
                stage,
                path,
                shadows,
                profile_fallback,
            } => {
                let mut line = format!("selected {}: {path}", stage.label());
                if *profile_fallback {
                    line.push_str(&format!(" [{PROFILE_FALLBACK_LABEL}]"));
                }
                if !shadows.is_empty() {
                    line.push_str(&format!(" (shadows: {})", shadows.join(", ")));
                }
                line
            }
        }
    }
}

/// The decisions discovery made, in the order it made them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DiscoveryTrace {
    events: Vec<TraceEvent>,
}

impl DiscoveryTrace {
    /// Append an event and write it to the debug log.
    pub fn record(&mut self, event: TraceEvent) {
        log::debug!("[rumdl-config] discovery: {}", event.describe());
        self.events.push(event);
    }

    pub fn events(&self) -> &[TraceEvent] {
        &self.events
    }

    /// Withdraw a selection whose file turned out not to be loadable and record the
    /// file as unusable instead, so the trace names only configuration that applies.
    /// The reason is fixed text: a parser error can quote the file.
    pub fn reject_selected(&mut self, stage: SearchStage, path: &str, reason: &str) {
        let profile_fallback = self.events.iter().any(|e| {
            matches!(e, TraceEvent::Selected { stage: s, path: p, profile_fallback: true, .. } if *s == stage && p == path)
        });
        self.events
            .retain(|e| !matches!(e, TraceEvent::Selected { stage: s, path: p, .. } if *s == stage && p == path));
        self.record(TraceEvent::Probe {
            stage,
            path: path.to_string(),
            outcome: ProbeOutcome::AccessError {
                error: reason.to_string(),
            },
            profile_fallback,
        });
    }

    /// Whether the home directory decision is already on the record. A load
    /// resolves the home once and the loaders it calls would otherwise repeat it.
    pub fn has_home(&self) -> bool {
        self.events.iter().any(|e| matches!(e, TraceEvent::Home { .. }))
    }

    pub fn has_start_directory(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, TraceEvent::StartDirectory { .. }))
    }

    /// Every file discovery selected, in the order it selected them.
    pub fn selected(&self) -> impl Iterator<Item = (&SearchStage, &str)> {
        self.events.iter().filter_map(|e| match e {
            TraceEvent::Selected { stage, path, .. } => Some((stage, path.as_str())),
            _ => None,
        })
    }

    /// The human-readable trace printed by `rumdl config file --explain`.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let mut stage: Option<SearchStage> = None;
        let mut markdownlint_selected = false;
        let mut any_selected = false;
        for event in &self.events {
            let event_stage = match event {
                TraceEvent::Probe { stage, .. } | TraceEvent::WalkEnded { stage, .. } => Some(*stage),
                _ => None,
            };
            if let Some(s) = event_stage
                && stage != Some(s)
            {
                out.push_str(&format!("{}:\n", s.heading()));
                stage = Some(s);
            }
            match event {
                TraceEvent::Probe { .. } | TraceEvent::WalkEnded { .. } => {
                    out.push_str(&format!("  {}\n", event.describe()));
                }
                TraceEvent::Selected {
                    stage: s,
                    path,
                    shadows,
                    profile_fallback,
                } => {
                    stage = None;
                    any_selected = true;
                    let note = if *s == SearchStage::User && markdownlint_selected {
                        " (applied as the base under the markdownlint config)"
                    } else {
                        ""
                    };
                    if *s == SearchStage::Markdownlint {
                        markdownlint_selected = true;
                    }
                    let label = if *profile_fallback {
                        format!(" [{PROFILE_FALLBACK_LABEL}]")
                    } else {
                        String::new()
                    };
                    out.push_str(&format!("selected {}: {path}{label}{note}\n", s.label()));
                    for shadow in shadows {
                        out.push_str(&format!("  shadows (lower priority): {shadow}\n"));
                    }
                }
                _ => {
                    stage = None;
                    out.push_str(&format!("{}\n", event.describe()));
                }
            }
        }
        if !any_selected {
            out.push_str("selected: none (using defaults)\n");
        }
        out
    }

    /// The machine-readable trace for `rumdl config --output json file --explain`.
    pub fn to_json(&self) -> String {
        #[derive(Serialize)]
        struct Selected<'a> {
            stage: SearchStage,
            path: &'a str,
        }
        #[derive(Serialize)]
        struct Document<'a> {
            events: &'a [TraceEvent],
            selected: Vec<Selected<'a>>,
        }
        let selected = self
            .selected()
            .map(|(stage, path)| Selected { stage: *stage, path })
            .collect();
        serde_json::to_string_pretty(&Document {
            events: &self.events,
            selected,
        })
        .unwrap_or_else(|_| "{}".to_string())
    }
}
