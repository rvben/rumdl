use crate::config::Config;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Rule};
use std::collections::{HashMap, HashSet};

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

    /// Apply fixes iteratively until no more fixes are needed or max iterations reached
    /// Returns (rules_fixed_count, iterations, context_creations, fixed_rule_names)
    pub fn apply_fixes_iterative(
        &self,
        rules: &[Box<dyn Rule>],
        all_warnings: &[LintWarning],
        content: &mut String,
        config: &Config,
        max_iterations: usize,
    ) -> Result<(usize, usize, usize, HashSet<String>), String> {
        // Get optimal rule order
        let ordered_rules = self.get_optimal_order(rules);

        // Group warnings by rule for quick lookup
        let mut warnings_by_rule: HashMap<&str, Vec<&LintWarning>> = HashMap::new();
        for warning in all_warnings {
            if let Some(rule_name) = warning.rule_name {
                warnings_by_rule.entry(rule_name).or_default().push(warning);
            }
        }

        let mut total_fixed = 0;
        let mut total_ctx_creations = 0;
        let mut iterations = 0;

        // Keep track of which rules have been processed successfully
        let mut processed_rules = HashSet::new();

        // Track which rules actually applied fixes
        let mut fixed_rule_names = HashSet::new();

        // Keep applying fixes until content stabilizes
        while iterations < max_iterations {
            iterations += 1;

            let mut fixes_in_iteration = 0;
            let mut any_fix_applied = false;

            // Process one rule at a time with its own context
            for rule in &ordered_rules {
                // Skip rules we've already successfully processed
                if processed_rules.contains(rule.name()) {
                    continue;
                }

                // Only process rules that had warnings
                if !warnings_by_rule.contains_key(rule.name()) {
                    processed_rules.insert(rule.name());
                    continue;
                }

                // Check if rule is disabled
                if config
                    .global
                    .unfixable
                    .iter()
                    .any(|r| r.eq_ignore_ascii_case(rule.name()))
                {
                    processed_rules.insert(rule.name());
                    continue;
                }

                if !config.global.fixable.is_empty()
                    && !config
                        .global
                        .fixable
                        .iter()
                        .any(|r| r.eq_ignore_ascii_case(rule.name()))
                {
                    processed_rules.insert(rule.name());
                    continue;
                }

                // Create context for this specific rule
                let ctx = LintContext::new(content, config.markdown_flavor());
                total_ctx_creations += 1;

                // Apply fix
                match rule.fix(&ctx) {
                    Ok(fixed_content) => {
                        if fixed_content != *content {
                            *content = fixed_content;
                            fixes_in_iteration += 1;
                            any_fix_applied = true;
                            processed_rules.insert(rule.name());
                            fixed_rule_names.insert(rule.name().to_string());

                            // If this rule has dependents, break to start fresh iteration
                            if self.dependencies.contains_key(rule.name()) {
                                break;
                            }
                            // Otherwise continue with the next rule
                        } else {
                            // No changes from this rule, mark as processed
                            processed_rules.insert(rule.name());
                        }
                    }
                    Err(_) => {
                        // Error applying fix, mark as processed to avoid retrying
                        processed_rules.insert(rule.name());
                    }
                }
            }

            total_fixed += fixes_in_iteration;

            // If no fixes were made in this iteration, we're done
            if !any_fix_applied {
                break;
            }

            // If all rules have been processed, we're done
            if processed_rules.len() >= ordered_rules.len() {
                break;
            }
        }

        Ok((total_fixed, iterations, total_ctx_creations, fixed_rule_names))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GlobalConfig;
    use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory};

    // Mock rule for testing
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
    fn test_single_iteration_fix() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MockRule {
            name: "MD001",
            warnings: vec![LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 10,
                message: "Test warning".to_string(),
                rule_name: Some("MD001"),
                severity: crate::rule::Severity::Error,
                fix: None,
            }],
            fix_content: "fixed content".to_string(),
        })];

        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            message: "Test warning".to_string(),
            rule_name: Some("MD001"),
            severity: crate::rule::Severity::Error,
            fix: None,
        }];

        let mut content = "original content".to_string();
        let config = Config {
            global: GlobalConfig::default(),
            per_file_ignores: HashMap::new(),
            rules: Default::default(),
        };

        let result = coordinator.apply_fixes_iterative(&rules, &warnings, &mut content, &config, 5);

        assert!(result.is_ok());
        let (total_fixed, iterations, ctx_creations, _) = result.unwrap();
        assert_eq!(total_fixed, 1);
        assert_eq!(iterations, 1);
        assert_eq!(ctx_creations, 1);
        assert_eq!(content, "fixed content");
    }

    #[test]
    fn test_multiple_iteration_with_dependencies() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(MockRule {
                name: "MD010", // Has dependents
                warnings: vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 10,
                    message: "Tabs".to_string(),
                    rule_name: Some("MD010"),
                    severity: crate::rule::Severity::Error,
                    fix: None,
                }],
                fix_content: "content with spaces".to_string(),
            }),
            Box::new(MockRule {
                name: "MD007", // Depends on MD010
                warnings: vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 10,
                    message: "Indentation".to_string(),
                    rule_name: Some("MD007"),
                    severity: crate::rule::Severity::Error,
                    fix: None,
                }],
                fix_content: "content with spaces and proper indent".to_string(),
            }),
        ];

        let warnings = vec![
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 10,
                message: "Tabs".to_string(),
                rule_name: Some("MD010"),
                severity: crate::rule::Severity::Error,
                fix: None,
            },
            LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 10,
                message: "Indentation".to_string(),
                rule_name: Some("MD007"),
                severity: crate::rule::Severity::Error,
                fix: None,
            },
        ];

        let mut content = "content with tabs".to_string();
        let config = Config {
            global: GlobalConfig::default(),
            per_file_ignores: HashMap::new(),
            rules: Default::default(),
        };

        let result = coordinator.apply_fixes_iterative(&rules, &warnings, &mut content, &config, 5);

        assert!(result.is_ok());
        let (total_fixed, iterations, ctx_creations, _) = result.unwrap();
        assert_eq!(total_fixed, 2);
        assert_eq!(iterations, 2); // Should take 2 iterations due to dependency
        assert!(ctx_creations >= 2);
    }

    #[test]
    fn test_unfixable_rules_skipped() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![Box::new(MockRule {
            name: "MD001",
            warnings: vec![LintWarning {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 10,
                message: "Test".to_string(),
                rule_name: Some("MD001"),
                severity: crate::rule::Severity::Error,
                fix: None,
            }],
            fix_content: "fixed".to_string(),
        })];

        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            message: "Test".to_string(),
            rule_name: Some("MD001"),
            severity: crate::rule::Severity::Error,
            fix: None,
        }];

        let mut content = "original".to_string();
        let mut config = Config {
            global: GlobalConfig::default(),
            per_file_ignores: HashMap::new(),
            rules: Default::default(),
        };
        config.global.unfixable = vec!["MD001".to_string()];

        let result = coordinator.apply_fixes_iterative(&rules, &warnings, &mut content, &config, 5);

        assert!(result.is_ok());
        let (total_fixed, _, _, _) = result.unwrap();
        assert_eq!(total_fixed, 0);
        assert_eq!(content, "original"); // Should not be changed
    }

    #[test]
    fn test_max_iterations_limit() {
        // This test ensures we don't loop infinitely
        let coordinator = FixCoordinator::new();

        // Create a rule that always changes content
        #[derive(Clone)]
        struct AlwaysChangeRule;
        impl Rule for AlwaysChangeRule {
            fn name(&self) -> &'static str {
                "MD999"
            }
            fn check(&self, _: &LintContext) -> LintResult {
                Ok(vec![LintWarning {
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 10,
                    message: "Always warns".to_string(),
                    rule_name: Some("MD999"),
                    severity: crate::rule::Severity::Error,
                    fix: None,
                }])
            }
            fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
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

        let rules: Vec<Box<dyn Rule>> = vec![Box::new(AlwaysChangeRule)];
        let warnings = vec![LintWarning {
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 10,
            message: "Always warns".to_string(),
            rule_name: Some("MD999"),
            severity: crate::rule::Severity::Error,
            fix: None,
        }];

        let mut content = "test".to_string();
        let config = Config {
            global: GlobalConfig::default(),
            per_file_ignores: HashMap::new(),
            rules: Default::default(),
        };

        let result = coordinator.apply_fixes_iterative(&rules, &warnings, &mut content, &config, 3);

        assert!(result.is_ok());
        let (_, iterations, _, _) = result.unwrap();
        assert_eq!(iterations, 1); // Should stop after first successful fix
    }

    #[test]
    fn test_empty_rules_and_warnings() {
        let coordinator = FixCoordinator::new();
        let rules: Vec<Box<dyn Rule>> = vec![];
        let warnings: Vec<LintWarning> = vec![];

        let mut content = "unchanged".to_string();
        let config = Config {
            global: GlobalConfig::default(),
            per_file_ignores: HashMap::new(),
            rules: Default::default(),
        };

        let result = coordinator.apply_fixes_iterative(&rules, &warnings, &mut content, &config, 5);

        assert!(result.is_ok());
        let (total_fixed, iterations, ctx_creations, _) = result.unwrap();
        assert_eq!(total_fixed, 0);
        assert_eq!(iterations, 1);
        assert_eq!(ctx_creations, 0);
        assert_eq!(content, "unchanged");
    }

    #[test]
    fn test_cyclic_dependencies_handled() {
        // Test that cyclic dependencies don't cause infinite loops
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
}
