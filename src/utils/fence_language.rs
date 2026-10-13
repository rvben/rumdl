//! Shared classification of fence info strings for language coverage checks.

use crate::config::MarkdownFlavor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceLanguage {
    Language,
    /// Valid flavor code syntax, accepted by MD040 but still subject to tool coverage.
    CodeSyntax,
    Missing,
    /// A flavor construct whose info string is not a plain language label.
    Exempt,
}

pub fn is_superfences_attribute(info: &str) -> bool {
    ["title=", "hl_lines=", "linenums=", ".", "#"]
        .iter()
        .any(|prefix| info.starts_with(prefix))
}

pub fn classify(info: &str, flavor: MarkdownFlavor) -> FenceLanguage {
    // Only spaces and tabs surround an info string; U+00A0 and the other Unicode
    // spaces are part of it.
    let info = info.trim_matches([' ', '\t']);
    if flavor.supports_myst_directives() && info.starts_with('{') && info.contains('}') {
        let name = info[1..].split('}').next().unwrap_or("");
        if name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
            return if matches!(name, "code" | "code-block" | "sourcecode" | "code-cell") {
                FenceLanguage::CodeSyntax
            } else {
                FenceLanguage::Exempt
            };
        }
    }
    if flavor.is_pandoc_compatible() && crate::utils::pandoc::is_pandoc_raw_block_lang(info) {
        return FenceLanguage::Exempt;
    }
    if (flavor.is_pandoc_compatible() && crate::utils::pandoc::is_pandoc_code_class_attr(info))
        || (flavor == MarkdownFlavor::Quarto && info.starts_with('{') && info.ends_with('}'))
    {
        return FenceLanguage::CodeSyntax;
    }
    if flavor == MarkdownFlavor::MkDocs && is_superfences_attribute(info) {
        FenceLanguage::Exempt
    } else if info.is_empty() || is_superfences_attribute(info) || (info.starts_with('{') && info.ends_with('}')) {
        FenceLanguage::Missing
    } else {
        FenceLanguage::Language
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flavor_constructs_are_exempt_but_unknown_plain_labels_are_languages() {
        for (flavor, info) in [
            (MarkdownFlavor::Pandoc, "{=html}"),
            (MarkdownFlavor::MyST, "{note}"),
            (MarkdownFlavor::MkDocs, "title=\"example\""),
        ] {
            assert_eq!(classify(info, flavor), FenceLanguage::Exempt);
        }
        for (flavor, info) in [
            (MarkdownFlavor::Quarto, "{python}"),
            (MarkdownFlavor::Quarto, "{{python}}"),
            (MarkdownFlavor::Pandoc, "{.python}"),
            (MarkdownFlavor::MyST, "{code-cell} python"),
        ] {
            assert_eq!(classify(info, flavor), FenceLanguage::CodeSyntax);
        }
        assert_eq!(classify("pyhton", MarkdownFlavor::Standard), FenceLanguage::Language);
        for info in ["", "{}", "{python}", "title=example"] {
            assert_eq!(classify(info, MarkdownFlavor::Standard), FenceLanguage::Missing);
        }
    }

    #[test]
    fn unicode_space_is_part_of_the_info_string() {
        assert_eq!(classify("\u{a0}", MarkdownFlavor::Standard), FenceLanguage::Language);
        assert_eq!(classify(" \t", MarkdownFlavor::Standard), FenceLanguage::Missing);
    }
}
