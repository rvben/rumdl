/// Edge case tests for MkDocs features
/// Tests malformed syntax, boundary conditions, and error handling
use rumdl_lib::utils::mkdocs_admonitions;
use rumdl_lib::utils::mkdocs_footnotes;
use rumdl_lib::utils::mkdocs_snippets;
use rumdl_lib::utils::mkdocs_tabs;
use rumdl_lib::utils::mkdocstrings_refs;

#[cfg(test)]
mod malformed_syntax_tests {
    use super::*;

    #[test]
    fn test_malformed_admonition_markers() {
        // Missing type
        assert!(!mkdocs_admonitions::is_admonition_start("!!!"));
        assert!(!mkdocs_admonitions::is_admonition_start("!!! "));
        assert!(!mkdocs_admonitions::is_admonition_start("???"));

        // Wrong number of markers
        assert!(!mkdocs_admonitions::is_admonition_start("!! note"));
        assert!(!mkdocs_admonitions::is_admonition_start("!!!! note"));
        assert!(!mkdocs_admonitions::is_admonition_start("?? note"));
        assert!(!mkdocs_admonitions::is_admonition_start("???? note"));

        // Unclosed quotes in title - the multi-line version would be tested elsewhere
        assert!(mkdocs_admonitions::is_admonition_start("!!! note \"Unclosed title"));

        // Special characters in type
        assert!(mkdocs_admonitions::is_admonition_start("!!! note-with-dash"));
        assert!(mkdocs_admonitions::is_admonition_start("!!! note_with_underscore"));
        assert!(!mkdocs_admonitions::is_admonition_start("!!! note@with@at"));
        assert!(!mkdocs_admonitions::is_admonition_start("!!! note<script>"));
    }

    #[test]
    fn test_malformed_footnote_syntax() {
        // Invalid footnote references
        assert!(!mkdocs_footnotes::contains_footnote_reference("[^]"));
        assert!(!mkdocs_footnotes::contains_footnote_reference("[^ ]"));
        assert!(!mkdocs_footnotes::contains_footnote_reference("[^"));
        assert!(!mkdocs_footnotes::contains_footnote_reference("^1]"));

        // Invalid footnote definitions
        assert!(!mkdocs_footnotes::is_footnote_definition("[^]: Empty name"));
        assert!(!mkdocs_footnotes::is_footnote_definition("[^ ]: Space in name"));
        assert!(!mkdocs_footnotes::is_footnote_definition("[^1] Not a definition"));
        assert!(!mkdocs_footnotes::is_footnote_definition("^1]: Missing bracket"));

        // Special characters in footnote names
        assert!(mkdocs_footnotes::contains_footnote_reference("[^valid-name]"));
        assert!(mkdocs_footnotes::contains_footnote_reference("[^valid_name]"));
        assert!(mkdocs_footnotes::contains_footnote_reference("[^123]"));
        assert!(!mkdocs_footnotes::contains_footnote_reference("[^invalid@name]"));
        assert!(!mkdocs_footnotes::contains_footnote_reference("[^invalid!name]"));
    }

    #[test]
    fn test_malformed_tab_syntax() {
        // Wrong number of equals
        assert!(!mkdocs_tabs::is_tab_marker("== \"Tab\""));
        assert!(!mkdocs_tabs::is_tab_marker("==== \"Tab\""));
        assert!(!mkdocs_tabs::is_tab_marker("= \"Tab\""));

        // Missing label
        assert!(!mkdocs_tabs::is_tab_marker("==="));
        assert!(!mkdocs_tabs::is_tab_marker("=== "));
        assert!(!mkdocs_tabs::is_tab_marker("===  "));

        // Unclosed quotes
        assert!(mkdocs_tabs::is_tab_marker("=== \"Unclosed"));
        assert!(mkdocs_tabs::is_tab_marker("=== Unclosed\""));

        // Special characters in labels
        assert!(mkdocs_tabs::is_tab_marker("=== \"Tab & More\""));
        assert!(mkdocs_tabs::is_tab_marker("=== \"Tab with \\\"quotes\\\"\""));
        assert!(mkdocs_tabs::is_tab_marker("=== \"Tab <script>alert('xss')</script>\""));
    }

    #[test]
    fn test_malformed_snippet_syntax() {
        // Invalid snippet markers
        assert!(!mkdocs_snippets::is_snippet_marker("-8<-- \"file.md\""));
        assert!(!mkdocs_snippets::is_snippet_marker("--8<- \"file.md\""));
        assert!(!mkdocs_snippets::is_snippet_marker("--8< \"file.md\""));
        assert!(!mkdocs_snippets::is_snippet_marker("8<-- \"file.md\""));

        // Bare markers are valid for block format (issue #70)
        assert!(mkdocs_snippets::is_snippet_marker("--8<--")); // Valid bare marker for block format
        assert!(!mkdocs_snippets::is_snippet_marker("--8<-- ")); // Invalid - has trailing space

        // Unclosed quotes
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- \"unclosed"));
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- unclosed\""));

        // Invalid section markers
        assert!(!mkdocs_snippets::is_snippet_section_start("<!-- --8<-- [start] -->"));
        assert!(!mkdocs_snippets::is_snippet_section_start(
            "<!-- --8<-- start:name] -->"
        ));
        assert!(!mkdocs_snippets::is_snippet_section_start("<!-- --8<-- [start: -->"));
        assert!(!mkdocs_snippets::is_snippet_section_end("<!-- --8<-- [end] -->"));
        assert!(!mkdocs_snippets::is_snippet_section_end("<!-- --8<-- end:name] -->"));

        // Empty section names
        assert!(mkdocs_snippets::is_snippet_section_start("<!-- --8<-- [start:] -->"));
        assert!(mkdocs_snippets::is_snippet_section_end("<!-- --8<-- [end:] -->"));
    }

    #[test]
    fn test_malformed_autodoc_syntax() {
        // Invalid autodoc markers
        assert!(!mkdocstrings_refs::is_autodoc_marker(":: module.Class"));
        assert!(!mkdocstrings_refs::is_autodoc_marker(":::: module.Class"));
        assert!(!mkdocstrings_refs::is_autodoc_marker(": module.Class"));

        // Missing module path
        assert!(!mkdocstrings_refs::is_autodoc_marker(":::"));
        assert!(!mkdocstrings_refs::is_autodoc_marker("::: "));

        // Invalid module paths
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module"));
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module.Class"));
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module:function"));
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module.Class.method"));
        assert!(!mkdocstrings_refs::is_autodoc_marker("::: module..Class"));
        assert!(!mkdocstrings_refs::is_autodoc_marker("::: .module.Class"));
        assert!(!mkdocstrings_refs::is_autodoc_marker("::: module.Class."));
    }

    #[test]
    fn test_empty_content_handling() {
        // Empty admonition
        let content = "!!! note\n\nNext paragraph";
        assert!(mkdocs_admonitions::is_within_admonition(content, 0));
        assert!(!mkdocs_admonitions::is_within_admonition(
            content,
            content.find("Next").unwrap()
        ));

        // Empty footnote definition
        let content = "[^1]:\n\nNext paragraph";
        assert!(mkdocs_footnotes::is_within_footnote_definition(content, 0));
        assert!(!mkdocs_footnotes::is_within_footnote_definition(
            content,
            content.find("Next").unwrap()
        ));

        // Empty tab content
        let content = "=== \"Tab\"\n\n=== \"Tab 2\"";
        assert!(mkdocs_tabs::is_within_tab_content(content, 0));

        // Empty autodoc block
        let content = "::: module.Class\n\nNext paragraph";
        assert!(mkdocstrings_refs::is_within_autodoc_block(content, 0));
        assert!(!mkdocstrings_refs::is_within_autodoc_block(
            content,
            content.find("Next").unwrap()
        ));
    }

    #[test]
    fn test_inconsistent_indentation() {
        // Mixed tabs and spaces in admonition content
        let content = "!!! note\n\tTab indented\n    Space indented";
        assert!(mkdocs_admonitions::is_within_admonition(
            content,
            content.find("Tab").unwrap()
        ));
        assert!(mkdocs_admonitions::is_within_admonition(
            content,
            content.find("Space").unwrap()
        ));

        // Insufficient indentation
        let content = "!!! note\n  Only 2 spaces";
        assert!(!mkdocs_admonitions::is_within_admonition(
            content,
            content.find("Only").unwrap()
        ));

        // Tabs counted as 4 spaces
        let content = "[^1]: Definition\n\tTab continuation";
        assert!(mkdocs_footnotes::is_within_footnote_definition(
            content,
            content.find("Tab").unwrap()
        ));
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn test_extremely_long_titles() {
        let long_title = "a".repeat(1000);
        let admonition = format!("!!! note \"{long_title}\"");
        assert!(mkdocs_admonitions::is_admonition_start(&admonition));

        let tab = format!("=== \"{long_title}\"");
        assert!(mkdocs_tabs::is_tab_marker(&tab));
    }

    #[test]
    fn test_extremely_long_references() {
        let long_ref = "a".repeat(500);
        let footnote_ref = format!("[^{long_ref}]");
        // Should handle gracefully even if exceeding MAX_REFERENCE_LENGTH
        assert!(mkdocs_footnotes::contains_footnote_reference(&footnote_ref));

        let footnote_def = format!("[^{long_ref}]: Definition");
        assert!(mkdocs_footnotes::is_footnote_definition(&footnote_def));
    }

    #[test]
    fn test_deeply_nested_indentation() {
        // 20 levels of indentation
        let indent = "    ".repeat(20);
        let line = format!("{indent}!!! note");
        assert!(mkdocs_admonitions::is_admonition_start(&line));
        assert_eq!(mkdocs_admonitions::get_admonition_indent(&line), Some(80));
    }

    #[test]
    fn test_content_at_file_boundaries() {
        // Feature at start of file
        let content = "!!! note\n    Content";
        assert!(mkdocs_admonitions::is_within_admonition(content, 0));

        // Feature at end of file without trailing newline
        let content = "Text\n!!! note\n    Content";
        let last_pos = content.len() - 1;
        assert!(mkdocs_admonitions::is_within_admonition(content, last_pos));

        // Empty file with just a marker
        let content = "!!! note";
        assert!(mkdocs_admonitions::is_within_admonition(content, 0));
    }

    #[test]
    fn test_maximum_line_count() {
        // Create document with 10000 lines
        let mut lines = Vec::new();
        for i in 0..5000 {
            lines.push(format!("Line {i}"));
        }
        lines.push("!!! note".to_string());
        lines.push("    Content".to_string());
        for i in 5002..10000 {
            lines.push(format!("Line {i}"));
        }
        let content = lines.join("\n");

        // Should still detect the admonition in the middle
        let note_pos = content.find("!!! note").unwrap();
        assert!(mkdocs_admonitions::is_within_admonition(&content, note_pos + 10));
    }
}

#[cfg(test)]
mod unicode_tests {
    use super::*;

    #[test]
    fn test_unicode_in_titles_and_labels() {
        // Unicode in admonition titles
        assert!(mkdocs_admonitions::is_admonition_start("!!! note \"测试\""));
        assert!(mkdocs_admonitions::is_admonition_start("!!! tip \"naïve\""));
        assert!(mkdocs_admonitions::is_admonition_start("!!! warning \"🚨 Alert\""));
        assert!(mkdocs_admonitions::is_admonition_start("!!! info \"Здравствуйте\""));

        // Unicode in tab labels
        assert!(mkdocs_tabs::is_tab_marker("=== \"中文\""));
        assert!(mkdocs_tabs::is_tab_marker("=== \"Español\""));
        assert!(mkdocs_tabs::is_tab_marker("=== \"🔥 Hot\""));

        // Unicode in footnote names
        // Note: Current implementation may not support Unicode in references
        assert!(!mkdocs_footnotes::contains_footnote_reference("[^测试]"));
        assert!(!mkdocs_footnotes::contains_footnote_reference("[^naïve]"));
    }

    #[test]
    fn test_rtl_text() {
        // Right-to-left text in titles
        assert!(mkdocs_admonitions::is_admonition_start("!!! note \"العربية\""));
        assert!(mkdocs_admonitions::is_admonition_start("!!! tip \"עברית\""));
        assert!(mkdocs_tabs::is_tab_marker("=== \"فارسی\""));
    }

    #[test]
    fn test_emoji_and_symbols() {
        // Emoji as type is NOT valid (not a valid CSS class name)
        assert!(!mkdocs_admonitions::is_admonition_start("!!! 📝"));

        // Emoji in quoted titles is valid
        assert!(mkdocs_tabs::is_tab_marker("=== \"📊 Charts\""));
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- \"📁/file.md\""));

        // Mathematical symbols
        assert!(mkdocs_admonitions::is_admonition_start("!!! note \"∑ ∏ ∫\""));
        assert!(mkdocs_tabs::is_tab_marker("=== \"λ calculus\""));
    }

    #[test]
    fn test_byte_position_with_multibyte_chars() {
        // Multibyte characters affect byte positions
        let content = "🔥 Hot\n!!! note\n    Content";
        let note_pos = content.find("!!! note").unwrap();
        assert!(mkdocs_admonitions::is_within_admonition(content, note_pos));

        // Content with mixed ASCII and Unicode
        let content = "Text 测试\n[^1]: Footnote 脚注\n    Continuation 继续";
        let def_pos = content.find("[^1]:").unwrap();
        assert!(mkdocs_footnotes::is_within_footnote_definition(content, def_pos));
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn test_html_injection_in_titles() {
        // HTML tags in titles should be handled safely
        let dangerous_title = "<script>alert('xss')</script>";
        assert!(mkdocs_admonitions::is_admonition_start(&format!(
            "!!! note \"{dangerous_title}\""
        )));
        assert!(mkdocs_tabs::is_tab_marker(&format!("=== \"{dangerous_title}\"")));

        // HTML entities
        assert!(mkdocs_admonitions::is_admonition_start("!!! note \"&lt;script&gt;\""));
        assert!(mkdocs_tabs::is_tab_marker("=== \"&amp;nbsp;\""));
    }

    #[test]
    fn test_path_traversal_in_snippets() {
        // Path traversal attempts should be detected as valid syntax
        // (validation should happen at a different layer)
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- \"../../../etc/passwd\""));
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- \"/etc/shadow\""));
        assert!(mkdocs_snippets::is_snippet_marker(
            "--8<-- \"C:\\Windows\\System32\\config\\sam\""
        ));
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- \"../../.env\""));

        // URL attempts
        assert!(mkdocs_snippets::is_snippet_marker(
            "--8<-- \"http://evil.com/malware.md\""
        ));
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- \"file:///etc/passwd\""));
    }

    #[test]
    fn test_command_injection_attempts() {
        // Command injection in autodoc paths
        assert!(mkdocstrings_refs::is_autodoc_marker("::: os.system('rm -rf /')"));
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module;ls"));
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module`whoami`"));

        // These should be handled safely by the parser
        // Real validation should happen when actually processing the paths
    }

    #[test]
    fn test_recursive_references() {
        // Circular footnote references
        let content = "[^1]: See [^2]\n[^2]: See [^1]";
        assert!(mkdocs_footnotes::is_within_footnote_definition(content, 0));

        // Self-referencing snippet (should be detected as valid syntax)
        assert!(mkdocs_snippets::is_snippet_marker("--8<-- \"./same_file.md\""));
    }
}

#[cfg(test)]
mod empty_content_tests {
    use super::*;

    #[test]
    fn test_empty_admonition_behavior() {
        // Empty admonition should still create a skip context
        let content = "!!! note\n\nNext paragraph";
        assert!(mkdocs_admonitions::is_admonition_start("!!! note"));
        // The blank line after should not be in the admonition
        assert!(!mkdocs_admonitions::is_within_admonition(
            content,
            content.find("Next").unwrap()
        ));

        // Admonition with only blank indented lines should maintain context
        let content_with_blanks = "!!! note\n    \n    \n\nNext paragraph";
        let blank_pos = content_with_blanks.find("    ").unwrap();
        assert!(mkdocs_admonitions::is_within_admonition(content_with_blanks, blank_pos));

        // Title-only admonition
        assert!(mkdocs_admonitions::is_admonition_start("!!! note \"Just a title\""));
    }

    #[test]
    fn test_empty_tab_behavior() {
        // Empty tab should still create context until next tab or unindented content
        let empty_tab = "=== \"Tab 1\"\n\n=== \"Tab 2\"\n    Content";
        assert!(mkdocs_tabs::is_tab_marker("=== \"Tab 1\""));

        // Check the second tab marker separately (extract the line)
        let lines: Vec<&str> = empty_tab.lines().collect();
        assert!(mkdocs_tabs::is_tab_marker(lines[2])); // "=== \"Tab 2\""

        // Tab with only blank indented lines
        let blank_content = "=== \"Tab\"\n    \n    \n\nNext";
        assert!(mkdocs_tabs::is_within_tab_content(
            blank_content,
            blank_content.find("    ").unwrap()
        ));
    }

    #[test]
    fn test_empty_footnote_behavior() {
        // Footnote definition needs some content after colon + space
        let empty_footnote = "[^1]: \n\nNext paragraph";
        assert!(mkdocs_footnotes::is_footnote_definition("[^1]: "));

        // Footnote with text after colon
        assert!(mkdocs_footnotes::is_footnote_definition("[^1]: Text"));

        // Verify next paragraph is not in footnote
        assert!(!mkdocs_footnotes::is_within_footnote_definition(
            empty_footnote,
            empty_footnote.find("Next").unwrap()
        ));

        // Footnote with blank indented lines
        let with_blanks = "[^1]: Start\n    \n    \n\nNext";
        let blank_pos = with_blanks.find("\n    ").unwrap() + 1;
        assert!(mkdocs_footnotes::is_within_footnote_definition(with_blanks, blank_pos));
    }

    #[test]
    fn test_empty_snippet_section_behavior() {
        // Empty snippet section markers
        let empty_section = "<!-- --8<-- [start:empty] -->\n<!-- --8<-- [end:empty] -->\nAfter";
        assert!(mkdocs_snippets::is_snippet_section_start(
            "<!-- --8<-- [start:empty] -->"
        ));
        assert!(mkdocs_snippets::is_snippet_section_end("<!-- --8<-- [end:empty] -->"));

        // Nothing between markers - should not affect content after
        assert!(!mkdocs_snippets::is_within_snippet_section(
            empty_section,
            empty_section.find("After").unwrap()
        ));

        // Section with only whitespace
        let whitespace_only = "<!-- --8<-- [start:ws] -->\n    \n    \n<!-- --8<-- [end:ws] -->";
        let ws_pos = whitespace_only.find("    ").unwrap();
        assert!(mkdocs_snippets::is_within_snippet_section(whitespace_only, ws_pos));
    }

    #[test]
    fn test_empty_autodoc_behavior() {
        // Autodoc with no options should be valid
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module.Class"));

        // Autodoc with empty YAML block
        let empty_yaml = "::: module.Class\n    \n\nNext";
        assert!(mkdocstrings_refs::is_autodoc_marker("::: module.Class"));

        // The blank indented line should be considered part of options
        let yaml_pos = empty_yaml.find("    ").unwrap();
        assert!(mkdocstrings_refs::is_autodoc_options(
            &empty_yaml[yaml_pos..yaml_pos + 4],
            0
        ));
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;

    #[test]
    fn test_many_features_in_one_document() {
        let mut content = String::new();

        // Add 100 of each feature type
        for i in 0..100 {
            content.push_str(&format!("!!! note \"Note {i}\"\n    Content\n\n"));
            content.push_str(&format!("[^{i}]: Footnote {i}\n\n"));
            content.push_str(&format!("=== \"Tab {i}\"\n    Content\n\n"));
            content.push_str(&format!("::: module{i}.Class\n\n"));
            content.push_str(&format!("--8<-- \"file{i}.md\"\n\n"));
        }

        // Should handle all features without performance issues
        assert!(mkdocs_admonitions::is_within_admonition(&content, 10));
        assert!(mkdocs_footnotes::is_within_footnote_definition(
            &content,
            content.find("[^0]:").unwrap()
        ));
        assert!(mkdocs_tabs::is_within_tab_content(
            &content,
            content.find("=== \"Tab 0\"").unwrap()
        ));
    }

    #[test]
    fn test_rapid_context_switches() {
        // Rapidly switching between different contexts
        let content = r#"!!! note
    [^1]: Footnote in admonition
    === "Tab in admonition"
        ::: module.Class
        Content
Regular text
!!! warning
    Content
[^2]: Outside
=== "Outside tab"
    Content"#;

        // Each feature should maintain its own context correctly
        let note_pos = content.find("!!! note").unwrap();
        assert!(mkdocs_admonitions::is_within_admonition(content, note_pos + 10));

        let footnote_pos = content.find("[^2]:").unwrap();
        assert!(mkdocs_footnotes::is_within_footnote_definition(
            content,
            footnote_pos + 5
        ));
    }

    #[test]
    fn test_parser_confusion_attempts() {
        // Try to confuse the parser with similar syntax
        // Should handle gracefully
        assert!(!mkdocs_admonitions::is_admonition_start("!!! !!! note"));
        assert!(!mkdocs_tabs::is_tab_marker("=== === \"Tab\""));
        assert!(!mkdocstrings_refs::is_autodoc_marker("::: ::: module"));
    }
}
