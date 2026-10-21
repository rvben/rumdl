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
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct AssetScanOptions {
    /// Site root for leading-slash URLs. All scanned directories must be within it.
    pub root: PathBuf,
    pub sources: Vec<PathBuf>,
    pub assets: Vec<PathBuf>,
    /// Root-relative globs. An empty list selects every non-Markdown asset.
    pub include: Vec<String>,
    /// Assets used outside Markdown (themes, CSS, templates, generated pages).
    pub keep: Vec<String>,
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
pub fn scan(options: &AssetScanOptions) -> io::Result<AssetScan> {
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
    for source in &sources {
        let content = std::fs::read_to_string(source)
            .map_err(|e| error(format!("Cannot read source {}: {e}", source.display())))?;
        for found in mentions.find_overlapping_iter(&content) {
            mentioned_patterns.insert(found.pattern().as_usize());
        }
        for destination in destinations(&content) {
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

fn destinations(content: &str) -> Vec<String> {
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let body_start = ctx
        .lines
        .get(ctx.front_matter_end_line())
        .map_or(content.len(), |line| line.byte_offset);
    let body = if ctx.front_matter_end_line() == 0 {
        content
    } else {
        &content[body_start..]
    };
    let parser = Parser::new_ext(body, crate::utils::rumdl_parser_options_for(body));
    // Definitions are conservatively counted even when currently unused.
    let mut result: Vec<_> = parser
        .reference_definitions()
        .iter()
        .map(|(_, def)| def.dest.to_string())
        .collect();
    for event in parser {
        if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = event {
            result.push(dest_url.to_string());
        }
    }
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
    result
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
