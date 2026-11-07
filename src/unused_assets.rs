//! Explicit, uncached project scans for assets unreferenced in Markdown sources.
//!
//! This is separate from per-document linting: a partial lint invocation cannot
//! establish that an asset has no incoming references.

use crate::config::MarkdownFlavor;
use crate::discovery::has_markdown_extension;
use crate::lint_context::LintContext;
use crate::utils::html_elements;
use crate::workspace_index::link_path_part;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use pulldown_cmark::{Event, Parser, Tag};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Resolves the Markdown flavor of each scanned source, given the sorted list of
/// source files. Returns one flavor per file, in the same order.
pub type FlavorResolver<'a> = &'a dyn Fn(&[PathBuf]) -> io::Result<Vec<MarkdownFlavor>>;

pub struct AssetScanOptions<'a> {
    /// Site root for leading-slash URLs. All scanned directories must be within it.
    pub root: PathBuf,
    pub sources: Vec<PathBuf>,
    pub assets: Vec<PathBuf>,
    /// Root-relative globs. An empty list selects every non-Markdown asset.
    pub include: Vec<String>,
    /// Assets used outside Markdown (themes, CSS, templates, generated pages).
    pub keep: Vec<String>,
    /// Decides how each source is parsed, so constructs that only exist in some
    /// flavors (MkDocs admonitions and tabs, MDX components, MyST directives)
    /// are read the way `rumdl check` reads them.
    pub flavors: FlavorResolver<'a>,
}

#[derive(Debug, Serialize)]
pub struct AssetScan {
    pub schema_version: u32,
    pub source_extensions: Vec<String>,
    pub sources_scanned: usize,
    pub assets_considered: usize,
    /// Paths relative to the site root, sorted and deduplicated.
    pub unreferenced: Vec<String>,
    /// Literal filename mentions that could not be resolved as parsed links.
    pub mentioned_but_unresolved: Vec<String>,
}

const MENTION_ENCODE: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'#')
    .add(b'?')
    .add(b'%')
    .add(b'"')
    .add(b'<')
    .add(b'>');

/// Characters that continue a file name. Anything else (quotes, brackets,
/// slashes, whitespace, punctuation, CJK brackets) delimits it.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '~' | '+' | '@')
}

/// Percent-encoded spellings of characters that delimit a name inside a URL or
/// attribute (`a%2Flogo.png`, `logo.png%3Fv=1`). Other escapes, such as `%20`,
/// are part of a longer name.
fn is_encoded_delimiter(hex: &[u8]) -> bool {
    matches!(
        hex.to_ascii_uppercase().as_slice(),
        b"2F"
            | b"5C"
            | b"22"
            | b"27"
            | b"28"
            | b"29"
            | b"3C"
            | b"3D"
            | b"3E"
            | b"2C"
            | b"5B"
            | b"5D"
            | b"60"
            | b"3F"
            | b"23"
    )
}

/// Whether a substring hit at `start..end` is a complete file name rather than
/// part of a longer one (`readme_logo.png`, `logo.png.bak`, `my-logo.png`).
fn is_whole_name(content: &str, start: usize, end: usize) -> bool {
    let bytes = content.as_bytes();
    let left_ok = match content[..start].chars().next_back() {
        None => true,
        Some('.') => false,
        // Emphasis such as `_logo.png_`: an underscore run that starts a word.
        Some('_') => !content[..start]
            .trim_end_matches('_')
            .chars()
            .next_back()
            .is_some_and(is_name_char),
        Some(c) if is_name_char(c) => {
            // `F` of `%2F` is alphanumeric but the escape as a whole is a delimiter.
            start >= 3 && bytes[start - 3] == b'%' && is_encoded_delimiter(&bytes[start - 2..start])
        }
        Some(_) => true,
    };
    if !left_ok {
        return false;
    }
    let rest = &content[end..];
    let mut chars = rest.chars();
    match chars.next() {
        None => true,
        // A dot continues the name only when more name follows (`.bak`); a
        // sentence-ending dot does not.
        Some('.') => !chars.next().is_some_and(is_name_char),
        // Closing emphasis such as `_logo.png_`: an underscore run that ends a word.
        Some('_') => !rest.trim_start_matches('_').chars().next().is_some_and(is_name_char),
        Some('%') => match rest.as_bytes().get(1..3) {
            Some(hex) if hex.iter().all(u8::is_ascii_hexdigit) => is_encoded_delimiter(hex),
            _ => true,
        },
        Some(c) => !is_name_char(c),
    }
}

fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

fn patterns(patterns: &[String]) -> io::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
                .map_err(|e| error(format!("Invalid asset glob {pattern:?}: {e}")))?,
        );
    }
    builder.build().map_err(|e| error(e.to_string()))
}

/// Walk without ignore filters. Skipping a source directory could turn a used
/// asset into an apparent orphan. Symlinks are rejected rather than silently
/// skipping their contents or following them outside the scan boundary.
fn files(directories: &[PathBuf], root: &Path) -> io::Result<BTreeSet<PathBuf>> {
    let mut result = BTreeSet::new();
    let mut pending = Vec::new();
    for directory in directories {
        if std::fs::symlink_metadata(directory)
            .map_err(|e| path_error(directory, &e))?
            .file_type()
            .is_symlink()
        {
            return Err(error(format!("Scan directory is a symlink: {}", directory.display())));
        }
        let directory = directory.canonicalize().map_err(|e| path_error(directory, &e))?;
        if !directory.starts_with(root) || !directory.is_dir() {
            return Err(error(format!(
                "Scan roots must be directories within {}: {}",
                root.display(),
                directory.display()
            )));
        }
        pending.push(directory);
    }
    let mut visited = HashSet::new();
    while let Some(directory) = pending.pop() {
        if !visited.insert(directory.clone()) {
            continue;
        }
        for entry in std::fs::read_dir(&directory).map_err(|e| path_error(&directory, &e))? {
            let entry = entry.map_err(|e| path_error(&directory, &e))?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| path_error(&path, &e))?;
            if kind.is_symlink() {
                return Err(error(format!(
                    "Cannot complete asset scan through symlink: {}",
                    path.display()
                )));
            }
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                result.insert(path);
            } else {
                return Err(error(format!("Cannot scan special file: {}", path.display())));
            }
        }
    }
    Ok(result)
}

/// Scan complete directory trees. No lint caches, rule enablement, file ignores
/// or prior invocations influence the answer. A scan error returns no findings.
pub fn scan(options: &AssetScanOptions<'_>) -> io::Result<AssetScan> {
    if options.sources.is_empty() || options.assets.is_empty() {
        return Err(error(
            "At least one source directory and one asset directory are required",
        ));
    }
    let root = options.root.canonicalize().map_err(|e| path_error(&options.root, &e))?;
    if !root.is_dir() {
        return Err(error(format!(
            "The site root must be a directory: {}",
            options.root.display()
        )));
    }
    let include = patterns(&options.include)?;
    let keep = patterns(&options.keep)?;
    let sources: Vec<_> = files(&options.sources, &root)?
        .into_iter()
        .filter(|p| has_markdown_extension(p))
        .collect();
    if sources.is_empty() {
        return Err(error(
            "No Markdown sources found; refusing to classify assets without a source scan",
        ));
    }
    let flavors = (options.flavors)(&sources)?;
    if flavors.len() != sources.len() {
        return Err(error("Flavor resolution did not cover every Markdown source"));
    }
    let asset_files = files(&options.assets, &root)?;
    // Markdown in an asset tree can itself reference assets. Silently excluding
    // it would make known incomplete coverage look like a complete source scan.
    for path in asset_files.iter().filter(|path| has_markdown_extension(path)) {
        if sources.binary_search(path).is_err() {
            return Err(error(format!(
                "Markdown in asset tree is outside the source scan: {}; add its directory with --source",
                path.display()
            )));
        }
    }
    let assets: BTreeSet<_> = asset_files
        .into_iter()
        .filter(|path| {
            let relative = path.strip_prefix(&root).expect("validated scan boundary");
            !has_markdown_extension(path)
                && (options.include.is_empty() || include.is_match(relative))
                && !keep.is_match(relative)
        })
        .collect();
    for asset in &assets {
        if asset.to_str().is_none() {
            return Err(error(format!("Asset path is not valid UTF-8: {}", asset.display())));
        }
    }
    let identities: Vec<_> = assets
        .iter()
        .map(|path| file_identity(path))
        .collect::<io::Result<_>>()?;
    // A literal mention can be a template, wiki link, import, code example or
    // frontmatter value. Preserve it conservatively without claiming resolution.
    let mut unique_patterns = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for (i, asset) in assets.iter().enumerate() {
        let name = asset.file_name().unwrap().to_str().unwrap();
        let encoded = percent_encoding::utf8_percent_encode(name, MENTION_ENCODE).to_string();
        unique_patterns.entry(name.to_owned()).or_default().push(i);
        if encoded != name {
            unique_patterns.entry(encoded).or_default().push(i);
        }
    }
    let (spellings, pattern_assets): (Vec<_>, Vec<_>) = unique_patterns.into_iter().unzip();
    let mentions = aho_corasick::AhoCorasick::new(&spellings)
        .map_err(|e| error(format!("Cannot build asset mention index: {e}")))?;
    let mut mentioned_patterns = HashSet::new();
    let mut referenced = HashSet::new();
    for (source, flavor) in sources.iter().zip(flavors) {
        let content = std::fs::read_to_string(source)
            .map_err(|e| error(format!("Cannot read source {}: {e}", source.display())))?;
        for found in mentions.find_overlapping_iter(&content) {
            // Each hit is judged on its own: a rejected hit never hides a later valid one.
            if is_whole_name(&content, found.start(), found.end()) {
                mentioned_patterns.insert(found.pattern().as_usize());
            }
        }
        for destination in destinations(&content, flavor) {
            if let Some(path) = resolve(&destination, source, &root) {
                referenced.insert(file_identity(&path)?);
            }
        }
    }
    // Expand each spelling once, even if thousands of assets share that name
    // and it occurs repeatedly across the source tree.
    let mentioned: HashSet<_> = mentioned_patterns
        .into_iter()
        .flat_map(|pattern| pattern_assets[pattern].iter().copied())
        .collect();
    let mut unreferenced = Vec::new();
    let mut mentioned_but_unresolved = Vec::new();
    for (i, asset) in assets.iter().enumerate() {
        if !referenced.contains(&identities[i]) {
            let path = asset
                .strip_prefix(&root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_str().unwrap())
                .collect::<Vec<_>>()
                .join("/");
            if mentioned.contains(&i) {
                mentioned_but_unresolved.push(path);
            } else {
                unreferenced.push(path);
            }
        }
    }
    unreferenced.sort();
    mentioned_but_unresolved.sort();
    Ok(AssetScan {
        schema_version: 1,
        source_extensions: crate::discovery::MARKDOWN_EXTENSIONS
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        sources_scanned: sources.len(),
        assets_considered: assets.len(),
        unreferenced,
        mentioned_but_unresolved,
    })
}

fn resolve(destination: &str, source: &Path, root: &Path) -> Option<PathBuf> {
    // URL schemes, network-path URLs and same-document fragments are not files.
    let destination = destination.trim();
    if destination.starts_with("//")
        || destination.starts_with('#')
        || destination
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|head| head.contains(':'))
    {
        return None;
    }
    let path = link_path_part(destination);
    if path.is_empty() {
        return None;
    }
    // Path::join normalizes separators and parent components for Windows
    // verbatim bases; on Unix, canonicalize preserves filesystem semantics.
    let path = if let Some(relative) = path.strip_prefix('/') {
        root.join(relative)
    } else {
        source.parent()?.join(path)
    };
    // A malformed or broken link is MD057's concern, not an incomplete source
    // scan. Only existing, in-boundary targets can establish a reference.
    path.canonicalize().ok().filter(|path| path.starts_with(root))
}

// Unix realpath need not recover stored spelling on case/normalization-insensitive
// mounts. File identity also recognizes such references without folding names on
// case-sensitive filesystems. Other platforms use their canonical path spelling.
#[cfg(unix)]
type FileIdentity = (u64, u64);
#[cfg(not(unix))]
type FileIdentity = PathBuf;

fn file_identity(path: &Path) -> io::Result<FileIdentity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path).map_err(|e| path_error(path, &e))?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    path.canonicalize().map_err(|e| path_error(path, &e))
}

/// Every destination a source can reference, as the union of three views of it.
/// Destinations that come from the Markdown parser are final; only values read
/// from HTML attributes are entity-decoded, once.
///
/// 1. The document body parsed as CommonMark (the flavor-independent baseline).
/// 2. The bodies of MkDocs admonitions and content tabs, dedented. To CommonMark
///    their 4-space indentation is a code block, so the body view never sees
///    them; they are parsed again without that indentation.
/// 3. The flavor-aware link, image and definition scan, which covers constructs
///    such as MDX components whose children a plain CommonMark parse hides.
fn destinations(content: &str, flavor: MarkdownFlavor) -> Vec<String> {
    let ctx = LintContext::new(content, flavor, None);
    let body_start = ctx
        .lines
        .get(ctx.front_matter_end_line())
        .map_or(content.len(), |line| line.byte_offset);
    let body = if ctx.front_matter_end_line() == 0 {
        content
    } else {
        &content[body_start..]
    };
    let mut result = Vec::new();
    markdown_destinations(body, &mut result);
    container_destinations(content, &ctx, &mut result);
    context_destinations(content, &ctx, &mut result);
    html_destinations(content, &ctx, &mut result);
    result
}

/// Link, image and reference-definition destinations of `text` as parsed by
/// CommonMark. Definitions are conservatively counted even when unused.
fn markdown_destinations(text: &str, result: &mut Vec<String>) {
    let parser = Parser::new_ext(text, crate::utils::rumdl_parser_options_for(text));
    result.extend(
        parser
            .reference_definitions()
            .iter()
            .map(|(_, def)| def.dest.to_string()),
    );
    for event in parser {
        if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = event {
            result.push(dest_url.to_string());
        }
    }
}

static ONE_LINE_DEFINITION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[[^\]]+\]:\s*\S").expect("valid definition pattern"));
static SOLE_HTML_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^</?[A-Za-z][A-Za-z0-9-]*(\s[^<>]*)?/?>$").expect("valid tag pattern"));

fn is_container_opener(line: &str) -> bool {
    ["!!!", "???", "???+", "==="].iter().any(|marker| {
        line.strip_prefix(marker)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    })
}

/// Destinations inside MkDocs containers, parsed from the dedented body text.
fn container_destinations(content: &str, ctx: &LintContext<'_>, result: &mut Vec<String>) {
    let mut text = String::new();
    let mut previous = None;
    for (index, line) in ctx.lines.iter().enumerate() {
        if !line.in_mkdocs_container() || line.in_code_block || line.in_front_matter || line.in_html_comment {
            continue;
        }
        let trimmed = content[line.byte_offset..line.byte_offset + line.byte_len].trim();
        // A gap keeps unrelated container lines from merging into one paragraph.
        if previous.is_some_and(|p| p + 1 != index) {
            text.push('\n');
        }
        previous = Some(index);
        // Openers carry no destinations and a wrapper tag such as
        // `<div markdown="1">` would turn the lines after it into an HTML block.
        if is_container_opener(trimmed) || SOLE_HTML_TAG.is_match(trimmed) {
            text.push('\n');
            continue;
        }
        // A one-line definition is parsed alone, so the line above it (for
        // example an admonition title) cannot absorb it.
        if ONE_LINE_DEFINITION.is_match(trimmed) {
            markdown_destinations(trimmed, result);
        }
        text.push_str(trimmed);
        text.push('\n');
    }
    markdown_destinations(&text, result);
}

/// Destinations the flavor-aware context finds outside MkDocs containers.
fn context_destinations(content: &str, ctx: &LintContext<'_>, result: &mut Vec<String>) {
    let skipped = |line: usize| {
        ctx.line_info(line)
            .is_none_or(|info| info.in_mkdocs_container() || info.in_front_matter || info.in_html_comment)
    };
    result.extend(
        ctx.links()
            .iter()
            .filter(|link| !link.is_reference && !skipped(link.line))
            .map(|link| link.url.to_string()),
    );
    result.extend(
        ctx.images()
            .iter()
            .filter(|image| !image.is_reference && !skipped(image.line))
            .map(|image| image.url.to_string()),
    );
    for definition in ctx.reference_definitions().iter().filter(|d| !skipped(d.line)) {
        // Parsed alone: the span's own indentation (an MDX child) is not code.
        if let Some(span) = content.get(definition.byte_offset..definition.byte_end) {
            markdown_destinations(span.trim_start(), result);
        }
    }
}

/// `href`, `src` and similar attributes of HTML elements outside code.
fn html_destinations(content: &str, ctx: &LintContext<'_>, result: &mut Vec<String>) {
    for tag in ctx.html_tags().iter().filter(|tag| {
        !tag.is_closing
            && !ctx.is_in_code_span_byte(tag.byte_offset)
            && !ctx
                .line_info(tag.line)
                .is_some_and(|line| line.in_code_block || line.in_front_matter)
    }) {
        let text = &content[tag.byte_offset..tag.byte_end];
        for attr in ["href", "src", "poster", "data", "data-src"] {
            if let Some((value, _)) = html_elements::extract_attribute_with_range(text, attr) {
                result.push(decode_entities(&value));
            }
        }
        if let Some((value, _)) = html_elements::extract_attribute_with_range(text, "srcset") {
            // Tokenize URLs before descriptors; embedded commas in data URLs
            // belong to the URL, not an extra local asset candidate.
            let value = decode_entities(&value);
            let mut remaining = value.as_str();
            while !remaining.is_empty() {
                remaining = remaining.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
                let end = remaining
                    .find(|c: char| c.is_ascii_whitespace())
                    .unwrap_or(remaining.len());
                let url = &remaining[..end];
                result.push(url.trim_end_matches(',').to_owned());
                remaining = &remaining[end..];
                if !url.ends_with(',') {
                    remaining = remaining.split_once(',').map_or("", |(_, rest)| rest);
                }
            }
        }
    }
}

fn decode_entities(value: &str) -> String {
    let mut result = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find('&') {
        result.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let decoded = remaining
            .as_bytes()
            .iter()
            .take(34)
            .position(|b| *b == b';')
            .and_then(|end| {
                let entity = &remaining[1..end];
                let decoded = if let Some(number) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
                    decode_numeric(number, 16)
                } else if let Some(number) = entity.strip_prefix('#') {
                    decode_numeric(number, 10)
                } else {
                    crate::mdx_parser::decode_named(entity, true)
                };
                decoded.map(|decoded| (end + 1, decoded))
            });
        if let Some((end, decoded)) = decoded {
            result.push_str(&decoded);
            remaining = &remaining[end..];
        } else {
            result.push('&');
            remaining = &remaining[1..];
        }
    }
    result.push_str(remaining);
    result
}

fn decode_numeric(number: &str, radix: u32) -> Option<String> {
    if number.is_empty() || !number.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    match u32::from_str_radix(number, radix) {
        Ok(_) => Some(crate::mdx_parser::decode_numeric(number, radix)),
        Err(_) => Some(char::REPLACEMENT_CHARACTER.to_string()),
    }
}

fn path_error(path: &Path, e: &io::Error) -> io::Error {
    error(format!("Cannot scan {}: {e}", path.display()))
}
