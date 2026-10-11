//! CLI2 ignore patterns retain the directory and phase in which CLI2 applies them.
//! Root patterns filter discovery; nested patterns filter a configuration group.

use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};

#[derive(Debug, Clone)]
enum Matcher {
    Glob(GlobMatcher),
    Extglob(fancy_regex::Regex),
}

impl Matcher {
    fn new(pattern: &str) -> Result<Self, String> {
        Self::compile(&translate_glob_ranges(pattern, true)?)
    }

    fn for_scope(pattern: &str, root: bool) -> Result<Self, String> {
        if root {
            Self::new(pattern)
        } else {
            Self::compile(&translate_glob_ranges(pattern, false)?)
        }
    }

    fn compile(pattern: &str) -> Result<Self, String> {
        if pattern
            .as_bytes()
            .windows(2)
            .any(|pair| b"@?+*!".contains(&pair[0]) && pair[1] == b'(')
        {
            let expression = extglob_regex(pattern)?;
            fancy_regex::Regex::new(&format!("^(?:{expression})$"))
                .map(Self::Extglob)
                .map_err(|error| error.to_string())
        } else {
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .empty_alternates(true)
                .backslash_escape(true)
                .build()
                .map(|glob| Self::Glob(glob.compile_matcher()))
                .map_err(|error| error.to_string())
        }
    }

    fn is_match(&self, path: &str) -> bool {
        match self {
            Self::Glob(glob) => glob.is_match(path),
            Self::Extglob(regex) => regex.is_match(path).unwrap_or(false),
        }
    }
}

/// Preserve globset's globstar and character-class handling, substituting
/// extglob groups in its generated expression. Alternatives can themselves
/// contain extglobs. Negation follows picomatch's prefix matching except for
/// terminal groups and wildcard groups followed by an extension.
fn extglob_regex(pattern: &str) -> Result<String, String> {
    extglob_regex_with_suffix(pattern, "")
}

fn extglob_regex_with_suffix(pattern: &str, following: &str) -> Result<String, String> {
    let mut marker_prefix = "RUMDL_EXTGLOB_".to_string();
    while pattern.contains(&marker_prefix) {
        marker_prefix.push('_');
    }
    let mut masked = String::new();
    let mut substitutions = Vec::new();
    let bytes = pattern.as_bytes();
    let mut index = 0;
    let mut in_class = false;
    while index < bytes.len() {
        let start = index;
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            let next = pattern[index + 1..].chars().next().unwrap();
            index += 1 + next.len_utf8();
            masked.push_str(&pattern[start..index]);
            continue;
        }
        if bytes[index] == b'[' {
            in_class = true;
        } else if bytes[index] == b']' {
            in_class = false;
        }
        if !in_class && b"@?+*!".contains(&bytes[index]) && bytes.get(index + 1) == Some(&b'(') {
            let operator = bytes[index];
            let mut depth = 1;
            let mut end = index + 2;
            let mut alternative_start = end;
            let mut alternatives = Vec::new();
            let mut class = false;
            while end < bytes.len() {
                match bytes[end] {
                    b'\\' => {
                        end += 2;
                        continue;
                    }
                    b'[' => class = true,
                    b']' => class = false,
                    b'(' if !class => depth += 1,
                    b')' if !class => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'|' if !class && depth == 1 => {
                        alternatives.push(extglob_regex_with_suffix(
                            &pattern[alternative_start..end],
                            &format!("{}{following}", &pattern[end..]),
                        )?);
                        alternative_start = end + 1;
                    }
                    _ => {}
                }
                end += 1;
            }
            if depth != 0 {
                return Err("Unclosed extglob group".into());
            }
            alternatives.push(extglob_regex_with_suffix(
                &pattern[alternative_start..end],
                &format!("{}{following}", &pattern[end..]),
            )?);
            let alternatives = alternatives.join("|");
            let expression = match operator {
                b'@' => format!("(?:{alternatives})"),
                b'?' => format!("(?:{alternatives})?"),
                b'+' => format!("(?:{alternatives})+"),
                b'*' => format!("(?:{alternatives})*"),
                b'!' => {
                    let inner = &pattern[start + 2..end];
                    let rest = format!("{}{following}", &pattern[end + 1..]);
                    let spans_components = inner.contains('/');
                    let terminal = rest.is_empty() || rest.chars().all(|c| c == ')');
                    let suffix = if inner.contains('*')
                        && rest.starts_with('.')
                        && !rest.contains(['/', '\\'])
                        && !rest[1..].contains('.')
                    {
                        extglob_regex(&rest)?
                    } else if terminal || spans_components {
                        "$".into()
                    } else {
                        String::new()
                    };
                    let wildcard = if spans_components { ".*" } else { "[^/]*" };
                    format!("(?!(?:{alternatives}){suffix}){wildcard}")
                }
                _ => unreachable!(),
            };
            let marker = format!("{marker_prefix}{}TOKEN", substitutions.len());
            masked.push_str(&marker);
            substitutions.push((marker, expression));
            index = end + 1;
        } else {
            let character = pattern[index..].chars().next().unwrap();
            masked.push(character);
            index += character.len_utf8();
        }
    }
    let glob = GlobBuilder::new(&masked)
        .literal_separator(true)
        .empty_alternates(true)
        .backslash_escape(true)
        .build()
        .map_err(|error| error.to_string())?;
    let mut expression = glob
        .regex()
        .strip_prefix("(?-u)^")
        .unwrap()
        .strip_suffix('$')
        .unwrap()
        .to_string();
    for (marker, substitution) in substitutions {
        expression = expression.replace(&marker, &substitution);
    }
    // Globset targets bytes and writes non-ASCII literals as UTF-8 byte
    // escapes. Fancy-regex targets text: restore those literals as characters.
    static BYTE_ESCAPES: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?:\\x[0-9a-fA-F]{2})+").unwrap());
    Ok(BYTE_ESCAPES
        .replace_all(&expression, |capture: &regex::Captures<'_>| {
            let bytes = capture[0]
                .as_bytes()
                .chunks_exact(4)
                .map(|byte| u8::from_str_radix(std::str::from_utf8(&byte[2..]).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            std::str::from_utf8(&bytes).map_or_else(|_| capture[0].to_string(), regex::escape)
        })
        .into_owned())
}

fn translate_glob_ranges(pattern: &str, expand_for_discovery: bool) -> Result<String, String> {
    static RANGE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\{(-?\d+|[A-Za-z])\.\.(-?\d+|[A-Za-z])(?:\.\.(-?\d+))?\}").unwrap()
    });
    let mut expanded = String::new();
    let mut previous = 0;
    for capture in RANGE.captures_iter(pattern) {
        let range = capture.get(0).unwrap();
        expanded.push_str(&pattern[previous..range.start()]);
        previous = range.end();
        // Escaped braces and character classes do not declare ranges.
        let prefix = &pattern[..range.start()];
        let escaped = prefix.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1;
        let in_class = prefix
            .rfind('[')
            .is_some_and(|open| prefix.rfind(']').is_none_or(|close| open > close));
        let alphabetic = capture[1].len() == 1 && capture[1].as_bytes()[0].is_ascii_alphabetic();
        let end_alphabetic = capture[2].len() == 1 && capture[2].as_bytes()[0].is_ascii_alphabetic();
        if escaped || in_class || alphabetic != end_alphabetic {
            expanded.push_str(range.as_str());
            continue;
        }
        if !expand_for_discovery {
            // Picomatch sorts range terms and tries them as a character class,
            // even for stepped or padded ranges. Invalid classes fall back to
            // the literal terms joined with dots, without the braces.
            let mut terms = vec![capture[1].to_string(), capture[2].to_string()];
            if let Some(step) = capture.get(3) {
                terms.push(step.as_str().to_string());
            }
            terms.sort();
            let class = format!("[{}]", terms.join("-"));
            if let Ok(class) = regex::Regex::new(&format!("^{class}$")) {
                // Globset's class parser treats consecutive hyphens as chained
                // ranges. Enumerate this ASCII class to retain JS regex semantics.
                let characters = (0u8..=127)
                    .filter_map(|byte| {
                        let character = char::from(byte);
                        class.is_match(&character.to_string()).then(|| {
                            if "[]?*{},\\".contains(character) {
                                format!("\\{character}")
                            } else {
                                character.to_string()
                            }
                        })
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                expanded.push('{');
                expanded.push_str(&characters);
                expanded.push('}');
            } else {
                expanded.push_str(&terms.join(".."));
            }
            continue;
        }
        let parse = |value: &str| value.parse::<i64>().map_err(|error| error.to_string());
        let (start, end) = if alphabetic {
            (i64::from(capture[1].as_bytes()[0]), i64::from(capture[2].as_bytes()[0]))
        } else {
            (parse(&capture[1])?, parse(&capture[2])?)
        };
        let step = capture
            .get(3)
            .map_or(Ok(1), |step| parse(step.as_str()))?
            .checked_abs()
            .ok_or("CLI2 range step is too large")?
            .max(1);
        let count = (i128::from(end) - i128::from(start)).abs() / i128::from(step) + 1;
        if count > 1000 {
            return Err("CLI2 ignore range exceeds 1000 alternatives".into());
        }
        let width = [&capture[1], &capture[2]]
            .into_iter()
            .filter(|value| value.trim_start_matches('-').starts_with('0') && value.len() > 1)
            .map(|value| value.trim_start_matches('-').len())
            .max()
            .unwrap_or(0);
        let direction = if end >= start { 1 } else { -1 };
        let alternatives = (0..count)
            .map(|index| {
                let value = i128::from(start) + index * i128::from(step) * direction;
                if alphabetic {
                    let character = char::from_u32(value as u32).unwrap();
                    if "[]?*{},\\".contains(character) {
                        format!("\\{character}")
                    } else {
                        character.to_string()
                    }
                } else if value < 0 {
                    format!("-{:0width$}", -value)
                } else {
                    format!("{value:0width$}")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        expanded.push('{');
        expanded.push_str(&alternatives);
        expanded.push('}');
    }
    expanded.push_str(&pattern[previous..]);
    Ok(expanded)
}

#[derive(Debug, Clone)]
pub struct Cli2Ignore {
    pub directory: PathBuf,
    pub patterns: Vec<String>,
    pub root: bool,
    matchers: Vec<(bool, Matcher)>,
    directory_pruners: Vec<(bool, Matcher)>,
}

impl PartialEq for Cli2Ignore {
    fn eq(&self, other: &Self) -> bool {
        self.directory == other.directory && self.patterns == other.patterns && self.root == other.root
    }
}

impl Cli2Ignore {
    pub(crate) fn new(directory: PathBuf, patterns: Vec<String>, root: bool) -> Result<Self, String> {
        let mut matchers = Vec::new();
        let mut directory_pruners = Vec::new();
        for pattern in &patterns {
            let mut glob = pattern.as_str();
            let mut negations = 0;
            while let Some(rest) = glob.strip_prefix('!').filter(|rest| !rest.starts_with('(')) {
                negations += 1;
                glob = rest;
            }
            // Globby removes one negation before fast-glob compiles its ignore
            // regex; micromatch toggles inclusion for every remaining negation.
            let include = if root {
                negations > 0 && negations % 2 == 0
            } else {
                negations % 2 == 1
            };
            let glob = glob.strip_prefix("./").unwrap_or(glob);
            // globby expands a literal directory argument only at the root.
            // Nested ignores are micromatch expressions over file paths.
            let glob =
                if root && negations == 0 && !glob.contains(['*', '?', '[', '{']) && directory.join(glob).is_dir() {
                    format!("{}/**", glob.trim_end_matches('/'))
                } else {
                    glob.to_string()
                };
            let matcher =
                Matcher::for_scope(&glob, root).map_err(|e| format!("Invalid CLI2 ignore pattern {pattern:?}: {e}"))?;
            // Fast-glob also applies static-basename and trailing-globstar
            // ignore regexes to directories before descending into them.
            let basename = glob.rsplit('/').next().unwrap_or(&glob);
            if root && (glob.ends_with("/**") || !basename.contains(['*', '?', '[', '{', '(', '|', '\\'])) {
                directory_pruners.push((include, matcher.clone()));
            }
            matchers.push((include, matcher));
        }
        Ok(Self {
            directory,
            patterns,
            root,
            matchers,
            directory_pruners,
        })
    }

    /// Whether this discovery scope can prune directories during a walk.
    pub fn prunes_directories(&self) -> bool {
        !self.directory_pruners.is_empty()
    }

    /// Directory pruning applies to a walk, but not to explicitly named files.
    pub fn prunes_walked_path(&self, path: &Path, walk_root: &Path) -> bool {
        if self.directory_pruners.is_empty() {
            return false;
        }
        let absolute = |path: &Path| crate::workspace_index::absolute_selection_path(path).ok();
        let (Some(path), Some(walk_root)) = (absolute(path), absolute(walk_root)) else {
            return false;
        };
        if !path.starts_with(&walk_root) {
            return false;
        }
        path.ancestors()
            .skip(1)
            .take_while(|parent| *parent != walk_root)
            .any(|parent| {
                let relative = relative_ignore_path(parent, &self.directory, true);
                let absolute = parent.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
                self.directory_pruners.iter().any(|(negated, matcher)| {
                    let matches = relative.as_deref().is_some_and(|relative| matcher.is_match(relative))
                        || matcher.is_match(&absolute);
                    matches != *negated
                })
            })
    }

    pub fn matches(&self, path: &Path) -> bool {
        // Ignore globs name the path supplied to CLI2, not a symlink's target.
        // Resolve aliases of the scope itself without resolving names below it.
        let relative = relative_ignore_path(path, &self.directory, self.root);
        if relative.is_none() && !self.root {
            return false;
        }
        let absolute = crate::workspace_index::absolute_selection_path(path)
            .ok()
            .map(|path| path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
        let matches = |matcher: &Matcher| {
            relative.as_deref().is_some_and(|relative| matcher.is_match(relative))
                || (self.root && absolute.as_deref().is_some_and(|absolute| matcher.is_match(absolute)))
        };
        // Globby's discovery phase excludes matching root patterns, including
        // doubly negated ones. Micromatch's group phase is a separate scope.
        if self.root {
            return self
                .matchers
                .iter()
                .any(|(negated, matcher)| matches(matcher) != *negated);
        }
        // Doubly negated micromatch expressions are inclusions: if any exists,
        // a file must match one to survive this group's filtering.
        let mut ignored = self.matchers.iter().any(|(include, _)| *include);
        for (include, matcher) in &self.matchers {
            if matches(matcher) {
                ignored = !include;
            }
        }
        ignored
    }
}

fn relative_ignore_path(path: &Path, directory: &Path, allow_parent: bool) -> Option<String> {
    let absolute = crate::workspace_index::absolute_selection_path(path).ok()?;
    let base = crate::workspace_index::absolute_selection_path(directory).ok()?;
    let relative = absolute
        .strip_prefix(&base)
        .ok()
        .map(Path::to_path_buf)
        .or_else(|| {
            let canonical_base = std::fs::canonicalize(&base).ok()?;
            absolute.ancestors().skip(1).find_map(|ancestor| {
                (std::fs::canonicalize(ancestor).ok().as_ref() == Some(&canonical_base))
                    .then(|| absolute.strip_prefix(ancestor).ok().map(Path::to_path_buf))
                    .flatten()
            })
        })
        .or_else(|| allow_parent.then(|| lexical_relative_path(&absolute, &base)).flatten())?;
    Some(relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"))
}

fn lexical_relative_path(path: &Path, base: &Path) -> Option<PathBuf> {
    let path_components = path.components().collect::<Vec<_>>();
    let base_components = base.components().collect::<Vec<_>>();
    if path_components.first() != base_components.first() {
        return None; // Different Windows drives cannot be expressed relatively.
    }
    let common = path_components
        .iter()
        .zip(&base_components)
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative = PathBuf::new();
    for _ in &base_components[common..] {
        relative.push("..");
    }
    for component in &path_components[common..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_keep_discovery_and_group_selection_phases() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let patterns = vec!["chapter{5..1..2}.md".into()];
        let top = Cli2Ignore::new(root.to_path_buf(), patterns.clone(), true).unwrap();
        let nested = Cli2Ignore::new(root.to_path_buf(), patterns, false).unwrap();
        assert!(!top.matches(&root.join("chapter2.md")));
        assert!(top.matches(&root.join("chapter3.md")));
        assert!(!top.matches(&root.join("chapter4.md")));
        assert!(nested.matches(&root.join("chapter2.md")));
        assert!(!nested.matches(&root.join("chapter3.md")));
        let padded = vec!["chapter{01..03}.md".into()];
        assert!(
            Cli2Ignore::new(root.to_path_buf(), padded.clone(), true)
                .unwrap()
                .matches(&root.join("chapter02.md"))
        );
        assert!(
            !Cli2Ignore::new(root.to_path_buf(), padded, false)
                .unwrap()
                .matches(&root.join("chapter02.md"))
        );
    }
    #[test]
    fn extglobs_match_alternatives_repetitions_and_negated_components() {
        for (pattern, yes, no) in [
            ("**/@(generated|vendor)/**", "docs/generated/x.md", "docs/manual/x.md"),
            ("docs/?(draft).md", "docs/.md", "docs/draftdraft.md"),
            ("docs/+(ab|c).md", "docs/abcc.md", "docs/.md"),
            ("docs/*(ab|c).md", "docs/.md", "docs/d.md"),
            ("docs/!(generated|vendor).md", "docs/keep.md", "docs/generated.md"),
            ("docs/!(@(generated|vendor)).md", "docs/keep.md", "docs/vendor.md"),
            ("docs/@(a|+(b|c)).md", "docs/bc.md", "docs/d.md"),
            ("docs/!(generated).md", "docs/keep.md", "docs/generated-extra.md"),
            ("docs/!(foo)", "docs/foobar", "docs/foo"),
            ("docs/!(*foo*).md", "docs/bar.md", "docs/foo-extra.md"),
            ("docs/chapter{1..3}.md", "docs/chapter2.md", "docs/chapter4.md"),
            ("docs/chapter{01..03}.md", "docs/chapter02.md", "docs/chapter2.md"),
            ("docs/chapter{5..1..2}.md", "docs/chapter3.md", "docs/chapter2.md"),
            ("docs/{a..c}.md", "docs/b.md", "docs/d.md"),
            (
                "docs/{preface,chapter{1..3}}.md",
                "docs/chapter2.md",
                "docs/chapter4.md",
            ),
            ("@(café|東京).md", "café.md", "cafe.md"),
            ("docs/@([é]|東京).md", "docs/é.md", "docs/e.md"),
            ("docs/@(a|!(foo)).md", "docs/bar.md", "docs/foobar.md"),
            ("docs/@(a|!(foo))", "docs/foobar", "docs/foo"),
        ] {
            let matcher = Matcher::new(pattern).unwrap();
            assert!(matcher.is_match(yes), "{pattern}: {yes}");
            assert!(!matcher.is_match(no), "{pattern}: {no}");
        }
        assert!(Matcher::new("docs/@(unclosed").is_err());
    }

    #[test]
    fn repeated_negations_keep_discovery_and_group_parity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let keep = root.join("keep.md");
        let other = root.join("other.md");
        for (pattern, root_keep, root_other, group_keep, group_other) in [
            ("!keep.md", true, false, false, true),
            ("!!keep.md", false, true, true, false),
            ("!!!keep.md", true, false, false, true),
        ] {
            let discovery = Cli2Ignore::new(root.to_path_buf(), vec![pattern.into()], true).unwrap();
            let group = Cli2Ignore::new(root.to_path_buf(), vec![pattern.into()], false).unwrap();
            assert_eq!(discovery.matches(&keep), root_keep, "{pattern}");
            assert_eq!(discovery.matches(&other), root_other, "{pattern}");
            assert_eq!(group.matches(&keep), group_keep, "{pattern}");
            assert_eq!(group.matches(&other), group_other, "{pattern}");
        }
    }

    #[test]
    fn root_and_nested_patterns_keep_cli2_glob_scope() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("docs/notes")).unwrap();
        let patterns = vec!["**/*.md".into(), "!notes/keep.md".into()];
        let nested = Cli2Ignore::new(root.join("docs"), patterns.clone(), false).unwrap();
        assert!(nested.matches(&root.join("docs/.hidden.md")));
        assert!(nested.matches(&root.join("docs/notes/x.md")));
        assert!(!nested.matches(&root.join("docs/notes/keep.md")));
        let top = Cli2Ignore::new(root.to_path_buf(), patterns, true).unwrap();
        assert!(top.matches(&root.join("notes/keep.md")));
        let direct = Cli2Ignore::new(root.to_path_buf(), vec!["docs/*".into()], true).unwrap();
        assert!(direct.matches(&root.join("docs/x.md")));
        assert!(!direct.matches(&root.join("docs/notes/x.md")));
        let directory = Cli2Ignore::new(root.to_path_buf(), vec!["docs".into()], true).unwrap();
        assert!(directory.matches(&root.join("docs/notes/x.md")));
        let nested_directory = Cli2Ignore::new(root.join("docs"), vec!["notes".into()], false).unwrap();
        assert!(!nested_directory.matches(&root.join("docs/notes/x.md")));
    }
}
