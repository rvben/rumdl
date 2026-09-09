//! Merge conflicts are a document safety condition, independent of lint rules.
//!
//! Scan raw lines, including fenced code: Git can insert conflicts anywhere.
//! Opening or closing markers suffice because conflicts may be partially resolved.
//! Separators alone are valid Setext headings, and diff3 base markers alone can
//! be table content, so neither is evidence of a conflict on its own.

use crate::rule::{LintWarning, Severity};

/// Find the first Git conflict marker, including custom marker widths >= 7.
/// Labels must be separated by whitespace, as in Git's marker syntax.
/// Literal conflict examples deliberately receive the same protection.
pub fn detect(content: &str) -> Option<LintWarning> {
    content.lines().enumerate().find_map(|(index, line)| {
        let column = if index == 0 && line.starts_with('\u{feff}') {
            2
        } else {
            1
        };
        let line = if index == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        };
        let marker = *line.as_bytes().first()?;
        if !matches!(marker, b'<' | b'>') {
            return None;
        }
        let width = line.bytes().take_while(|&byte| byte == marker).count();
        if width < 7 || !matches!(line.as_bytes().get(width), None | Some(b' ' | b'\t')) {
            return None;
        }
        Some(LintWarning {
            rule_name: Some("merge-conflict".to_string()),
            message: "Unresolved merge conflict; formatting skipped until conflict markers are removed".to_string(),
            line: index + 1,
            column,
            end_line: index + 1,
            end_column: width + column,
            severity: Severity::Error,
            fix: None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_partial_custom_and_fenced_conflicts() {
        for content in [
            "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n",
            ">>>>>>> branch",
            "<<<<<<<",
            "<<<<<<<<< HEAD",
            "```text\n<<<<<<< HEAD\n```\n",
            "\u{feff}<<<<<<< HEAD\r\n",
            "<<<<<<< HEAD\nours\n||||||| base\nbase\n=======\ntheirs\n>>>>>>> branch",
        ] {
            assert!(detect(content).is_some(), "{content:?}");
        }
        assert_eq!(detect("# Title\r\n\r\n>>>>>>> branch").unwrap().line, 3);
    }

    #[test]
    fn merge_conflict_protects_shared_fix_engine_and_document_run() {
        let content = "# Title\r\n\n<<<<<<< HEAD\r\ntext   ";
        let config = crate::config::Config::default();
        let rules = crate::rules::all_rules(&config);
        let mut fixed = content.to_string();
        let result = crate::fix_coordinator::FixCoordinator::new()
            .apply_fixes_iterative(&rules, &[], &mut fixed, &config, 100, None)
            .unwrap();
        assert_eq!(fixed, content);
        assert_eq!(result.rules_fixed, 0);
        assert_eq!(result.iterations, 0);
        let run = crate::document_run::DocumentRun::new(content, &rules, &config);
        assert_eq!(run.fix(100).unwrap().0, content);
        let analysis = run.analyze().unwrap();
        assert_eq!(analysis.warnings.len(), 1);
        assert!(analysis.warnings[0].fix.is_none());
    }

    #[test]
    fn preserves_ordinary_markdown_syntax() {
        for content in [
            "Title\n=======\n",
            "||||||| base",
            "<<<<<< HEAD",
            ">>> quote",
            ">>>>>>>quote",
            "<<<<<<<not-a-label",
            "text <<<<<<< HEAD",
            "    <<<<<<< HEAD",
            "> > > > > > > quote",
        ] {
            assert!(detect(content).is_none(), "{content:?}");
        }
    }
}
