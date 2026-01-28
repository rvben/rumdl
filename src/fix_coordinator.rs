use crate::config::Config;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Rule};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Maximum number of fix iterations before stopping (same as Ruff)
const MAX_ITERATIONS: usize = 100;

/// Result of applying fixes iteratively
///
/// This struct provides named fields instead of a tuple to prevent
/// confusion about the meaning of each value.
#[derive(Debug, Clone)]
pub struct FixResult {
    /// Total number of rules that successfully applied fixes
    pub rules_fixed: usize,
    /// Number of fix iterations performed
    pub iterations: usize,
    /// Number of LintContext instances created during fixing
    pub context_creations: usize,
    /// Names of rules that applied fixes
    pub fixed_rule_names: HashSet<String>,
    /// Whether the fix process converged (content stabilized)
    pub converged: bool,
}

/// Calculate hash of content for convergence detection
fn hash_content(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Coordinates rule fixing to minimize the number of passes needed
pub struct FixCoordinator {
    /// Rules that should run before others (rule -> rules that depend on it)
    dependencies: HashMap<&'static str, Vec<&'static str>>,
}

impl Default for FixCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl FixCoordinator {
    pub fn new() -> Self {
        let mut dependencies = HashMap::new();

        // CRITICAL DEPENDENCIES:
        // These dependencies prevent cascading issues that require multiple passes

        // MD064 (multiple consecutive spaces) MUST run before:
        // - MD010 (tabs->spaces) - MD010 replaces tabs with multiple spaces (e.g., 4),
        //   which MD064 would incorrectly collapse back to 1 space if it ran after
        dependencies.insert("MD064", vec!["MD010"]);

        // MD010 (tabs->spaces) MUST run before:
        // - MD007 (list indentation) - because tabs affect indent calculation
        // - MD005 (list indent consistency) - same reason
        dependencies.insert("MD010", vec!["MD007", "MD005"]);

        // MD013 (line length) MUST run before:
        // - MD009 (trailing spaces) - line wrapping might add trailing spaces that need cleanup
        // - MD012 (multiple blanks) - reflowing can affect blank lines
        // Note: MD013 now trims trailing whitespace during reflow to prevent mid-line spaces
        dependencies.insert("MD013", vec!["MD009", "MD012"]);

        // MD004 (list style) should run before:
        // - MD007 (list indentation) - changing markers affects indentation
        dependencies.insert("MD004", vec!["MD007"]);

        // MD022/MD023 (heading spacing) should run before:
        // - MD012 (multiple blanks) - heading fixes can affect blank lines
        dependencies.insert("MD022", vec!["MD012"]);
        dependencies.insert("MD023", vec!["MD012"]);

        // MD070 (nested fence collision) MUST run before:
        // - MD040 (code language) - MD070 changes block structure, making orphan fences into content
        // - MD031 (blanks around fences) - same reason
        dependencies.insert("MD070", vec!["MD040", "MD031"]);

        Self { dependencies }
    }

    /// Get the optimal order for running rules based on dependencies
    pub fn get_optimal_order<'a>(&self, rules: &'a [Box<dyn Rule>]) -> Vec<&'a dyn Rule> {
        // Build a map of rule names to rules for quick lookup
        let rule_map: HashMap<&str, &dyn Rule> = rules.iter().map(|r| (r.name(), r.as_ref())).collect();

        // Build reverse dependencies (rule -> rules it depends on)
        let mut reverse_deps: HashMap<&str, HashSet<&str>> = HashMap::new();
        for (prereq, dependents) in &self.dependencies {
            for dependent in dependents {
                reverse_deps.entry(dependent).or_default().insert(prereq);
            }
        }

        // Perform topological sort
        let mut sorted = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        fn visit<'a>(
            rule_name: &str,
            rule_map: &HashMap<&str, &'a dyn Rule>,
            reverse_deps: &HashMap<&str, HashSet<&str>>,
            visited: &mut HashSet<String>,
            visiting: &mut HashSet<String>,
            sorted: &mut Vec<&'a dyn Rule>,
        ) {
            if visited.contains(rule_name) {
                return;
            }

            if visiting.contains(rule_name) {
                // Cycle detected, but we'll just skip it
                return;
            }

            visiting.insert(rule_name.to_string());

            // Visit dependencies first
            if let Some(deps) = reverse_deps.get(rule_name) {
                for dep in deps {
                    if rule_map.contains_key(dep) {
                        visit(dep, rule_map, reverse_deps, visited, visiting, sorted);
                    }
                }
            }

            visiting.remove(rule_name);
            visited.insert(rule_name.to_string());

            // Add this rule to sorted list
            if let Some(&rule) = rule_map.get(rule_name) {
                sorted.push(rule);
            }
        }

        // Visit all rules
        for rule in rules {
            visit(
                rule.name(),
                &rule_map,
                &reverse_deps,
                &mut visited,
                &mut visiting,
                &mut sorted,
            );
        }

        // Add any rules not in dependency graph
        for rule in rules {
            if !sorted.iter().any(|r| r.name() == rule.name()) {
                sorted.push(rule.as_ref());
            }
        }

        sorted
    }

    /// Apply fixes iteratively until no more fixes are needed or max iterations reached.
    ///
    /// This implements a Ruff-inspired fix loop that re-checks ALL rules after each fix
    /// to detect cascading issues (e.g., MD046 creating code blocks that MD040 needs to fix).
    pub fn apply_fixes_iterative(
        &self,
        rules: &[Box<dyn Rule>],
        _all_warnings: &[LintWarning], // Kept for API compatibility, but we re-check all rules
        content: &mut String,
        config: &Config,
        max_iterations: usize,
    ) -> Result<FixResult, String> {
        // Use the minimum of max_iterations parameter and MAX_ITERATIONS constant
        let max_iterations = max_iterations.min(MAX_ITERATIONS);

        // Get optimal rule order based on dependencies
        let ordered_rules = self.get_optimal_order(rules);

        let mut total_fixed = 0;
        let mut total_ctx_creations = 0;
        let mut iterations = 0;
        let mut previous_hash = hash_content(content);

        // Track which rules actually applied fixes
        let mut fixed_rule_names = HashSet::new();

        // Build set of unfixable rules for quick lookup
        let unfixable_rules: HashSet<&str> = config.global.unfixable.iter().map(|s| s.as_str()).collect();

        // Build set of fixable rules (if specified)
        let fixable_rules: HashSet<&str> = config.global.fixable.iter().map(|s| s.as_str()).collect();
        let has_fixable_allowlist = !fixable_rules.is_empty();

        // Ruff-style fix loop: keep applying fixes until content stabilizes
        while iterations < max_iterations {
            iterations += 1;

            // Create fresh context for this iteration
            let ctx = LintContext::new(content, config.markdown_flavor(), None);
            total_ctx_creations += 1;

            let mut any_fix_applied = false;

            // Check and fix each rule in dependency order
            for rule in &ordered_rules {
                // Skip disabled rules
                if unfixable_rules.contains(rule.name()) {
                    continue;
                }
                if has_fixable_allowlist && !fixable_rules.contains(rule.name()) {
                    continue;
                }

                // Skip rules that indicate they should be skipped (opt-in rules, content-based skipping)
                if rule.should_skip(&ctx) {
                    continue;
                }

                // Check if this rule has any current warnings
                let warnings = match rule.check(&ctx) {
                    Ok(w) => w,
                    Err(_) => continue,
                };

                if warnings.is_empty() {
                    continue;
                }

                // Check if any warnings are fixable
                let has_fixable = warnings.iter().any(|w| w.fix.is_some());
                if !has_fixable {
                    continue;
                }

                // Apply fix
                match rule.fix(&ctx) {
                    Ok(fixed_content) => {
                        if fixed_content != *content {
                            *content = fixed_content;
                            total_fixed += 1;
                            any_fix_applied = true;
                            fixed_rule_names.insert(rule.name().to_string());

                            // Break to re-check all rules with the new content
                            // This is the key difference from the old approach:
                            // we always restart from the beginning after a fix
                            break;
                        }
                    }
                    Err(_) => {
                        // Error applying fix, continue to next rule
                        continue;
                    }
                }
            }

            // Check if content has stabilized (hash-based convergence)
            let current_hash = hash_content(content);
            if current_hash == previous_hash {
                // Content unchanged - converged!
                return Ok(FixResult {
                    rules_fixed: total_fixed,
                    iterations,
                    context_creations: total_ctx_creations,
                    fixed_rule_names,
                    converged: true,
                });
            }
            previous_hash = current_hash;

            // If no fixes were applied this iteration, we've converged
            if !any_fix_applied {
                return Ok(FixResult {
                    rules_fixed: total_fixed,
                    iterations,
                    context_creations: total_ctx_creations,
                    fixed_rule_names,
                    converged: true,
                });
            }
        }

        // Hit max iterations - did not converge
        Ok(FixResult {
            rules_fixed: total_fixed,
            iterations,
            context_creations: total_ctx_creations,
            fixed_rule_names,
            converged: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock rule that checks content and applies fixes based on a condition
    #[derive(Clone)]
    struct ConditionalFixRule {
        name: &'static str,
        /// Function to check if content has issues
        check_fn: fn(&str) -> bool,
        /// Function to fix content
        fix_fn: fn(&str) -> String,
    }

    impl Rule for ConditionalFixRule {
        fn name(&self) -> &'static str {
            self.name
        }

        fn check(&self, ctx: &LintContext) -> LintResult {
            if (self.check_fn)(ctx.content) {
                Ok(vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    message: format!("{} issue found", self.name),
                    rule_name: Some(self.name.to_string()),
                    severity: Severity::Error,
                    fix: Some(Fix {
                        range: 0..0,
                        replacement: String::new(),
                    }),
                }])
            } else {
                Ok(vec![])
            }
        }

        fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
            Ok((self.fix_fn)(ctx.content))
        }

        fn description(&self) -> &'static str {
            "Conditional fix rule for testing"
        }

        fn category(&self) -> RuleCategory {
            RuleCategory::Whitespace
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // Simple mock rule for basic tests
    #[derive(Clone)]
    struct MockRule {
        name: &'static str,
        warnings: Vec<LintWarning>,
        fix_content: String,
    }

    impl Rule for MockRule {
        fn name(&self) -> &'static str {
            self.name
        }

        fn check(&self, _ctx: &LintContext) -> LintResult {
            Ok(self.warnings.clone())
        }

        fn fix(&self, _ctx: &LintContext) -> Result<String, LintError> {
            Ok(self.fix_content.clone())
        }

        fn description(&self) -> &'static str {
            "Mock rule for testing"
        }

        fn category(&self) -> RuleCategory {
            RuleCategory::Whitespace
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn test_dependency_ordering() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MockRule {
                name: "MD009",
                warnings: vec![],
                fix_content: "".to_string(),
            }),
            Box::new(MockRule {
                name: "MD013",
                warnings: vec![],
                fix_content: "".to_string(),
            }),
            Box::new(MockRule {
                name: "MD010",
                warnings: vec![],
                fix_content: "".to_string(),
            }),
            Box::new(MockRule {
                name: "MD007",
                warnings: vec![],
                fix_content: "".to_string(),
            }),
        ];

        let ordered = coordinator.get_optimal_order(&rules);
        let ordered_names: Vec<&str> = ordered.iter().map(|r| r.name()).collect();

        // MD010 should come before MD007 (dependency)
        let md010_idx = ordered_names.iter().position(|&n| n == "MD010").unwrap();
        let md007_idx = ordered_names.iter().position(|&n| n == "MD007").unwrap();
        assert!(md010_idx < md007_idx, "MD010 should come before MD007");

        // MD013 should come before MD009 (dependency)
        let md013_idx = ordered_names.iter().position(|&n| n == "MD013").unwrap();
        let md009_idx = ordered_names.iter().position(|&n| n == "MD009").unwrap();
        assert!(md013_idx < md009_idx, "MD013 should come before MD009");
    }

    #[test]
    fn test_single_rule_fix() {
        let coordinator = FixCoordinator::new();

        // Rule that removes "BAD" from content
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(ConditionalFixRule {
            name: "RemoveBad",
            check_fn: |content| content.contains("BAD"),
            fix_fn: |content| content.replace("BAD", "GOOD"),
        })];

        let mut content = "This is BAD content".to_string();
        let config = Config::default();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5)
            .unwrap();

        assert_eq!(content, "This is GOOD content");
        assert_eq!(result.rules_fixed, 1);
        assert!(result.converged);
    }

    #[test]
    fn test_cascading_fixes() {
        // Simulates MD046 -> MD040 cascade:
        // Rule1: converts "INDENT" to "FENCE" (like MD046 converting indented to fenced)
        // Rule2: converts "FENCE" to "FENCE_LANG" (like MD040 adding language)
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "Rule1_IndentToFence",
                check_fn: |content| content.contains("INDENT"),
                fix_fn: |content| content.replace("INDENT", "FENCE"),
            }),
            Box::new(ConditionalFixRule {
                name: "Rule2_FenceToLang",
                check_fn: |content| content.contains("FENCE") && !content.contains("FENCE_LANG"),
                fix_fn: |content| content.replace("FENCE", "FENCE_LANG"),
            }),
        ];

        let mut content = "Code: INDENT".to_string();
        let config = Config::default();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 10)
            .unwrap();

        // Should reach final state in one run (internally multiple iterations)
        assert_eq!(content, "Code: FENCE_LANG");
        assert_eq!(result.rules_fixed, 2);
        assert!(result.converged);
        assert!(result.iterations >= 2, "Should take at least 2 iterations for cascade");
    }

    #[test]
    fn test_indirect_cascade() {
        // Simulates MD022 -> MD046 -> MD040 indirect cascade:
        // Rule1: adds "BLANK" (like MD022 adding blank line)
        // Rule2: only triggers if "BLANK" present, converts "CODE" to "FENCE"
        // Rule3: converts "FENCE" to "FENCE_LANG"
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "Rule1_AddBlank",
                check_fn: |content| content.contains("HEADING") && !content.contains("BLANK"),
                fix_fn: |content| content.replace("HEADING", "HEADING BLANK"),
            }),
            Box::new(ConditionalFixRule {
                name: "Rule2_CodeToFence",
                // Only detects CODE as issue if BLANK is present (simulates CommonMark rule)
                check_fn: |content| content.contains("BLANK") && content.contains("CODE"),
                fix_fn: |content| content.replace("CODE", "FENCE"),
            }),
            Box::new(ConditionalFixRule {
                name: "Rule3_AddLang",
                check_fn: |content| content.contains("FENCE") && !content.contains("LANG"),
                fix_fn: |content| content.replace("FENCE", "FENCE_LANG"),
            }),
        ];

        let mut content = "HEADING CODE".to_string();
        let config = Config::default();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 10)
            .unwrap();

        // Key assertion: all fixes applied in single run
        assert_eq!(content, "HEADING BLANK FENCE_LANG");
        assert_eq!(result.rules_fixed, 3);
        assert!(result.converged);
    }

    #[test]
    fn test_unfixable_rules_skipped() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![Box::new(ConditionalFixRule {
            name: "MD001",
            check_fn: |content| content.contains("BAD"),
            fix_fn: |content| content.replace("BAD", "GOOD"),
        })];

        let mut content = "BAD content".to_string();
        let mut config = Config::default();
        config.global.unfixable = vec!["MD001".to_string()];

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5)
            .unwrap();

        assert_eq!(content, "BAD content"); // Should not be changed
        assert_eq!(result.rules_fixed, 0);
        assert!(result.converged);
    }

    #[test]
    fn test_fixable_allowlist() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "AllowedRule",
                check_fn: |content| content.contains("A"),
                fix_fn: |content| content.replace("A", "X"),
            }),
            Box::new(ConditionalFixRule {
                name: "NotAllowedRule",
                check_fn: |content| content.contains("B"),
                fix_fn: |content| content.replace("B", "Y"),
            }),
        ];

        let mut content = "AB".to_string();
        let mut config = Config::default();
        config.global.fixable = vec!["AllowedRule".to_string()];

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5)
            .unwrap();

        assert_eq!(content, "XB"); // Only A->X, B unchanged
        assert_eq!(result.rules_fixed, 1);
    }

    #[test]
    fn test_max_iterations_limit() {
        let coordinator = FixCoordinator::new();

        // Rule that always changes content (pathological case)
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(Clone)]
        struct AlwaysChangeRule;
        impl Rule for AlwaysChangeRule {
            fn name(&self) -> &'static str {
                "AlwaysChange"
            }
            fn check(&self, _: &LintContext) -> LintResult {
                Ok(vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    message: "Always".to_string(),
                    rule_name: Some("AlwaysChange".to_string()),
                    severity: Severity::Error,
                    fix: Some(Fix {
                        range: 0..0,
                        replacement: String::new(),
                    }),
                }])
            }
            fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                Ok(format!("{}x", ctx.content))
            }
            fn description(&self) -> &'static str {
                "Always changes"
            }
            fn category(&self) -> RuleCategory {
                RuleCategory::Whitespace
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }

        COUNTER.store(0, Ordering::SeqCst);
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(AlwaysChangeRule)];

        let mut content = "test".to_string();
        let config = Config::default();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5)
            .unwrap();

        // Should stop at max iterations
        assert_eq!(result.iterations, 5);
        assert!(!result.converged);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_empty_rules() {
        let coordinator = FixCoordinator::new();
        let rules: Vec<Box<dyn Rule>> = vec![];

        let mut content = "unchanged".to_string();
        let config = Config::default();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5)
            .unwrap();

        assert_eq!(result.rules_fixed, 0);
        assert_eq!(result.iterations, 1);
        assert!(result.converged);
        assert_eq!(content, "unchanged");
    }

    #[test]
    fn test_no_warnings_no_changes() {
        let coordinator = FixCoordinator::new();

        // Rule that finds no issues
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(ConditionalFixRule {
            name: "NoIssues",
            check_fn: |_| false, // Never finds issues
            fix_fn: |content| content.to_string(),
        })];

        let mut content = "clean content".to_string();
        let config = Config::default();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5)
            .unwrap();

        assert_eq!(content, "clean content");
        assert_eq!(result.rules_fixed, 0);
        assert!(result.converged);
    }

    #[test]
    fn test_cyclic_dependencies_handled() {
        let mut coordinator = FixCoordinator::new();

        // Create a cycle: A -> B -> C -> A
        coordinator.dependencies.insert("RuleA", vec!["RuleB"]);
        coordinator.dependencies.insert("RuleB", vec!["RuleC"]);
        coordinator.dependencies.insert("RuleC", vec!["RuleA"]);

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MockRule {
                name: "RuleA",
                warnings: vec![],
                fix_content: "".to_string(),
            }),
            Box::new(MockRule {
                name: "RuleB",
                warnings: vec![],
                fix_content: "".to_string(),
            }),
            Box::new(MockRule {
                name: "RuleC",
                warnings: vec![],
                fix_content: "".to_string(),
            }),
        ];

        // Should not panic or infinite loop
        let ordered = coordinator.get_optimal_order(&rules);

        // Should return all rules despite cycle
        assert_eq!(ordered.len(), 3);
    }

    #[test]
    fn test_fix_is_idempotent() {
        // This is the key test for issue #271
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "Rule1",
                check_fn: |content| content.contains("A"),
                fix_fn: |content| content.replace("A", "B"),
            }),
            Box::new(ConditionalFixRule {
                name: "Rule2",
                check_fn: |content| content.contains("B") && !content.contains("C"),
                fix_fn: |content| content.replace("B", "BC"),
            }),
        ];

        let config = Config::default();

        // First run
        let mut content1 = "A".to_string();
        let result1 = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content1, &config, 10)
            .unwrap();

        // Second run on same final content
        let mut content2 = content1.clone();
        let result2 = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content2, &config, 10)
            .unwrap();

        // Should be identical (idempotent)
        assert_eq!(content1, content2);
        assert_eq!(result2.rules_fixed, 0, "Second run should fix nothing");
        assert!(result1.converged);
        assert!(result2.converged);
    }
}
