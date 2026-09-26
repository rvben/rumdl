//! Where a link destination leads, answered the way MD057 answers it.
//!
//! MD057 decides whether a destination exists; other rules need to know which
//! file it is. Both go through the resolution below, so a rule checking a
//! fragment looks in the same file MD057 says the link lands on: the same
//! decoding, the same directory and extension fallbacks, the same search paths
//! and absolute-link handling, and the same supplied document set.

use super::{AbsoluteLinksOption, CURRENT_DIR, MD057Config, MD057ExistingRelativeLinks, PROTOCOL_DOMAIN_REGEX};
use crate::lint_context::LinkTargetPolicy;
use crate::utils::project_root::project_root;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Check if a URL is external or should be skipped for validation.
///
/// Returns `true` (skip validation) for:
/// - URLs with protocols: `https://`, `http://`, `ftp://`, `mailto:`, etc.
/// - Bare domains: `www.example.com`, `example.com`
/// - Email addresses: `user@example.com` (without `mailto:`)
/// - Template variables: `{{URL}}`, `{{% include %}}`
/// - Framework path aliases: `~/assets/logo.png`, `@/components/Button.vue`
///
/// Returns `false` (validate) for:
/// - Relative filesystem paths: `./file.md`, `../parent/file.md`, `file.md`
pub(crate) fn is_external_url(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }

    // Quick checks for common external URL patterns
    if PROTOCOL_DOMAIN_REGEX.is_match(url) || url.starts_with("www.") {
        return true;
    }

    // Skip template variables (Handlebars/Mustache/Jinja2 syntax)
    // Examples: {{URL}}, {{#URL}}, {{> partial}}, {{% include %}}, {{ variable }}
    if url.starts_with("{{") || url.starts_with("{%") {
        return true;
    }

    // Simple check: if URL contains @, it's almost certainly an email address
    // File paths with @ are extremely rare, so this is a safe heuristic
    if url.contains('@') {
        return true;
    }

    // Bare domain check (e.g., "example.com")
    // Note: We intentionally DON'T skip all TLDs like .org, .net, etc.
    // Links like [text](nodejs.org/path) without a protocol are broken -
    // they'll be treated as relative paths by markdown renderers.
    // Flagging them helps users find missing protocols.
    // We only skip .com as a minimal safety net for the most common case.
    // Require the absence of a path separator so a relative file reference
    // that merely ends in ".com" (e.g. "../../vendor.com") is still
    // validated rather than assumed to be a bare domain.
    if !url.contains('/') && url.ends_with(".com") {
        return true;
    }

    // Framework path aliases (resolved by build tools like Vite, webpack, etc.)
    // These are not filesystem paths but module/asset aliases
    // Examples: ~/assets/image.png, @images/photo.jpg, @/components/Button.vue
    if url.starts_with('~') || url.starts_with('@') {
        return true;
    }

    false
}

/// Whether a destination names no file relative to the Markdown source:
/// external destinations, and gh-aw output placeholders in that flavor.
pub(crate) fn is_non_file_destination(url: &str, flavor: crate::config::MarkdownFlavor) -> bool {
    is_external_url(url)
        || (flavor == crate::config::MarkdownFlavor::GhAw && crate::utils::gh_aw::is_output_placeholder(url))
}

/// What a link destination resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkResolution {
    /// The file or directory the destination lands on.
    Target(PathBuf),
    /// The destination was resolved and names nothing.
    Missing,
    /// The configured handling does not resolve this destination: an absolute
    /// link under `absolute-links = "ignore"` or `"warn"`, or a docs-dir route
    /// with no mkdocs.yml to find the docs directory by.
    Unchecked,
}

/// Resolves link destinations with MD057's configuration.
#[derive(Debug, Clone, Default)]
pub struct LinkResolver {
    config: MD057Config,
    /// Settles the flavor per file, which decides whether the Obsidian
    /// attachment folder is searched. Without it every file is Standard.
    markdown_config: Option<Arc<crate::config::Config>>,
}

impl LinkResolver {
    pub fn new(config: MD057Config, markdown_config: Option<Arc<crate::config::Config>>) -> Self {
        Self {
            config,
            markdown_config,
        }
    }

    /// A resolver reading MD057's section of `config`, whichever rules are
    /// enabled: where a link leads does not depend on whether MD057 reports it.
    pub fn from_config(config: &crate::config::Config) -> Self {
        let md057 = crate::rule_config_serde::load_rule_config::<MD057Config>(config);
        Self::new(md057, Some(Arc::new(config.clone())))
    }

    /// Resolve `url`, written in the document at `source_file`.
    ///
    /// Resolving several links from one document goes through
    /// [`Self::for_document`], which settles the document's side once.
    pub fn resolve(&self, source_file: &Path, url: &str, policy: Option<&LinkTargetPolicy>) -> LinkResolution {
        self.for_document(source_file).resolve(url, policy)
    }

    /// The links of the document at `source_file`, resolved from its
    /// directory as MD057 reads it: the directory of the file a symlink
    /// points to, so a link means what it means where the content lives.
    pub fn for_document(&self, source_file: &Path) -> DocumentLinks<'_> {
        let flavor = self
            .markdown_config
            .as_ref()
            .map_or(crate::config::MarkdownFlavor::Standard, |config| {
                config.get_flavor_for_file(source_file)
            });
        let base_path = source_file
            .canonicalize()
            .ok()
            .and_then(|file| file.parent().map(Path::to_path_buf))
            .or_else(|| source_file.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| CURRENT_DIR.clone());
        let project_root = project_root();
        let search_paths = MD057ExistingRelativeLinks::search_paths_for(
            &self.config.search_paths,
            flavor,
            Some(source_file),
            &base_path,
            project_root,
        );
        DocumentLinks {
            resolver: self,
            flavor,
            base_path,
            project_root,
            search_paths,
        }
    }
}

/// A [`LinkResolver`] bound to the document whose links it resolves.
pub struct DocumentLinks<'a> {
    resolver: &'a LinkResolver,
    flavor: crate::config::MarkdownFlavor,
    base_path: PathBuf,
    project_root: &'static Path,
    search_paths: Vec<PathBuf>,
}

impl DocumentLinks<'_> {
    /// Resolve `url` as written in this document.
    ///
    /// The destination's query and fragment are ignored. A destination that
    /// names no file (external, fragment-only, empty) is `Unchecked`.
    pub fn resolve(&self, url: &str, policy: Option<&LinkTargetPolicy>) -> LinkResolution {
        if url.is_empty() || url.starts_with('#') || is_non_file_destination(url, self.flavor) {
            return LinkResolution::Unchecked;
        }
        if MD057ExistingRelativeLinks::is_absolute_path(url) {
            return self
                .resolver
                .resolve_absolute(url, &self.base_path, self.project_root, policy);
        }
        match MD057ExistingRelativeLinks::resolve_relative(url, &self.base_path, &self.search_paths, policy) {
            Some(target) => LinkResolution::Target(target),
            None => LinkResolution::Missing,
        }
    }
}

impl LinkResolver {
    fn resolve_absolute(
        &self,
        url: &str,
        base_path: &Path,
        project_root: &Path,
        policy: Option<&LinkTargetPolicy>,
    ) -> LinkResolution {
        match self.config.absolute_links {
            AbsoluteLinksOption::Ignore | AbsoluteLinksOption::Warn => LinkResolution::Unchecked,
            AbsoluteLinksOption::RelativeToDocs => {
                match MD057ExistingRelativeLinks::resolve_absolute_via_docs_dir(url, base_path, policy) {
                    None => LinkResolution::Unchecked,
                    Some(super::Resolution::Found(target)) => LinkResolution::Target(target),
                    Some(_) => LinkResolution::Missing,
                }
            }
            AbsoluteLinksOption::RelativeToRoots => {
                match MD057ExistingRelativeLinks::resolve_absolute_via_roots(
                    url,
                    &self.config.roots,
                    project_root,
                    policy,
                ) {
                    Some(target) => LinkResolution::Target(target),
                    None => LinkResolution::Missing,
                }
            }
        }
    }
}
