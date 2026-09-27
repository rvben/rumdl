use crate::config::Config;
use crate::lint_context::LintContext;
use crate::rule::{FixCapability, LintWarning, Rule};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Maximum number of fix iterations before stopping (same as Ruff)
const MAX_ITERATIONS: usize = 100;

/// Rules whose fix lays text out on lines rather than changing what the text says.
///
/// Reflow picks wrap points from the width of the inline content in front of it,
/// and its default mode only revisits a line that is over the limit. A rewrite that
/// shortens inline content AFTER reflow (a link turned into a shortcut reference, a
/// bare URL wrapped in angle brackets, spaces removed inside emphasis) therefore
/// leaves every affected line short for good, and a second run cannot repair it
/// because a short line is never a finding. Ordering layout after every content
/// rewrite is the invariant; the sort applies it to the whole rule set, so a rewrite
/// does not have to be listed here to be covered (listing MD054 by hand is exactly
/// what was missing when it slipped through).
const LAYOUT_RULES: &[&str] = &["MD013"];

/// Whether configuration lets `rule_name` apply fixes: `unfixable` does not
/// list it, and `fixable`, when set, does.
///
/// Config rule lists are canonical (`Config::canonicalize_rule_lists`), so a
/// name compares exactly with `Rule::name()`.
pub fn config_allows_fix(config: &Config, rule_name: &str) -> bool {
    let global = &config.global;
    !global.unfixable.iter().any(|name| name == rule_name)
        && (global.fixable.is_empty() || global.fixable.iter().any(|name| name == rule_name))
}

/// Where the fix loop spends its time, printed as `[FIX]` lines when
/// `RUMDL_PROFILE_RULES` is set, beside the `[RULE]` lines of the lint pass.
///
/// The fix loop re-parses the document and re-checks rules after every fix, so a
/// rule's cost there is its lint cost times the number of times it runs. The
/// report says how many contexts were built and, per rule, how often it was
/// checked, how often its `fix()` was called (whether or not it changed
/// anything), and the time both took together.
#[cfg(not(target_arch = "wasm32"))]
struct FixLoopProfile {
    contexts: std::cell::Cell<usize>,
    parse: std::cell::Cell<std::time::Duration>,
    rules: std::cell::RefCell<Vec<RuleFixProfile>>,
}

#[cfg(not(target_arch = "wasm32"))]
struct RuleFixProfile {
    name: &'static str,
    checks: usize,
    fix_calls: usize,
    elapsed: std::time::Duration,
}

#[cfg(not(target_arch = "wasm32"))]
impl FixLoopProfile {
    fn from_env() -> Option<Self> {
        std::env::var_os("RUMDL_PROFILE_RULES").map(|_| Self {
            contexts: std::cell::Cell::new(0),
            parse: std::cell::Cell::new(std::time::Duration::ZERO),
            rules: std::cell::RefCell::new(Vec::new()),
        })
    }

    fn record_context(&self, elapsed: std::time::Duration) {
        self.contexts.set(self.contexts.get() + 1);
        self.parse.set(self.parse.get() + elapsed);
    }

    fn with_rule(&self, name: &'static str, update: impl FnOnce(&mut RuleFixProfile)) {
        let mut rules = self.rules.borrow_mut();
        let index = rules.iter().position(|rule| rule.name == name).unwrap_or_else(|| {
            rules.push(RuleFixProfile {
                name,
                checks: 0,
                fix_calls: 0,
                elapsed: std::time::Duration::ZERO,
            });
            rules.len() - 1
        });
        update(&mut rules[index]);
    }

    /// Count one check of `name` and time it, and any fix that follows, until
    /// the returned guard drops.
    fn time<'a>(&'a self, name: &'static str) -> RuleFixTimer<'a> {
        self.with_rule(name, |rule| rule.checks += 1);
        RuleFixTimer {
            profile: self,
            name,
            start: std::time::Instant::now(),
        }
    }

    fn fix_attempted(&self, name: &'static str) {
        self.with_rule(name, |rule| rule.fix_calls += 1);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for FixLoopProfile {
    fn drop(&mut self) {
        eprintln!(
            "[FIX]  {:6} {:?} ({} contexts)",
            "parse",
            self.parse.get(),
            self.contexts.get()
        );
        for rule in self.rules.borrow().iter() {
            eprintln!(
                "[FIX]  {:6} {:?} ({} checks, {} fix calls)",
                rule.name, rule.elapsed, rule.checks, rule.fix_calls
            );
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct RuleFixTimer<'a> {
    profile: &'a FixLoopProfile,
    name: &'static str,
    start: std::time::Instant,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for RuleFixTimer<'_> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        self.profile.with_rule(self.name, |rule| rule.elapsed += elapsed);
    }
}

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
    /// Rules identified as participants in an oscillation cycle.
    /// Populated only when `converged == false` and a cycle was detected.
    /// Empty when the fix loop hit `max_iterations` without cycling.
    pub conflicting_rules: Vec<String>,
    /// Ordered rule sequence observed in the cycle.
    /// If non-empty, this can be rendered as a loop by appending the first rule
    /// at the end (e.g. `MD044 -> MD063 -> MD044`).
    pub conflict_cycle: Vec<String>,
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

        // MD005/MD077 (list indent and continuation indent) MUST run before:
        // - MD032 (blanks around lists) - MD005/MD077 fix nesting structure that MD032 relies on
        //   to correctly identify list block boundaries; running MD032 first on under-indented
        //   content causes spurious blank-line insertions inside list items
        dependencies.insert("MD005", vec!["MD032"]);
        dependencies.insert("MD077", vec!["MD032"]);

        Self { dependencies }
    }

    /// The given rules plus everything that transitively depends on them.
    fn downstream_of(&self, roots: &[&'static str]) -> HashSet<&'static str> {
        let mut closure: HashSet<&'static str> = HashSet::new();
        let mut pending: Vec<&'static str> = roots.to_vec();
        while let Some(name) = pending.pop() {
            if !closure.insert(name) {
                continue;
            }
            if let Some(dependents) = self.dependencies.get(name) {
                pending.extend(dependents.iter().copied());
            }
        }
        closure
    }

    /// Get the optimal order for running rules based on dependencies
    pub fn get_optimal_order<'a>(&self, rules: &'a [Box<dyn Rule>]) -> Vec<&'a dyn Rule> {
        // Build a map of rule names to rules for quick lookup
        let rule_map: HashMap<&str, &dyn Rule> = rules.iter().map(|r| (r.name(), r.as_ref())).collect();
        // Input position of each rule. Prerequisites are visited in this order so
        // that rules the dependency table leaves unordered keep their input order,
        // and so the result is the same on every run: the sets below are hashed,
        // and iterating them directly would let the hasher's per-process seed pick
        // the order of the whole rule set.
        let position: HashMap<&str, usize> = rules.iter().enumerate().map(|(i, r)| (r.name(), i)).collect();

        // Build reverse dependencies (rule -> rules it depends on)
        let mut reverse_deps: HashMap<&str, HashSet<&str>> = HashMap::new();
        for (prereq, dependents) in &self.dependencies {
            for dependent in dependents {
                reverse_deps.entry(dependent).or_default().insert(prereq);
            }
        }

        // A layout rule runs after every rule that is not downstream of it. Rules
        // downstream of a layout rule (its dependents, transitively) are excluded so
        // the declared edges MD013 -> MD009/MD012 stay acyclic.
        let downstream_of_layout = self.downstream_of(LAYOUT_RULES);
        for &layout_rule in LAYOUT_RULES {
            let prereqs = reverse_deps.entry(layout_rule).or_default();
            prereqs.extend(
                rule_map
                    .keys()
                    .copied()
                    .filter(|name| *name != layout_rule && !downstream_of_layout.contains(name)),
            );
        }

        // Perform topological sort
        let mut sorted = Vec::new();
        let mut visited: HashSet<&str> = HashSet::new();
        let mut visiting: HashSet<&str> = HashSet::new();

        fn visit<'a, 'b>(
            rule_name: &'b str,
            rule_map: &HashMap<&str, &'a dyn Rule>,
            position: &HashMap<&str, usize>,
            reverse_deps: &HashMap<&'b str, HashSet<&'b str>>,
            visited: &mut HashSet<&'b str>,
            visiting: &mut HashSet<&'b str>,
            sorted: &mut Vec<&'a dyn Rule>,
        ) where
            'a: 'b,
        {
            if visited.contains(rule_name) {
                return;
            }

            if visiting.contains(rule_name) {
                // Cycle detected, but we'll just skip it
                return;
            }

            visiting.insert(rule_name);

            // Visit dependencies first, in input order
            if let Some(deps) = reverse_deps.get(rule_name) {
                let mut deps: Vec<&'b str> = deps.iter().copied().filter(|dep| rule_map.contains_key(dep)).collect();
                deps.sort_by_key(|dep| position[dep]);
                for dep in deps {
                    visit(dep, rule_map, position, reverse_deps, visited, visiting, sorted);
                }
            }

            visiting.remove(rule_name);
            visited.insert(rule_name);

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
                &position,
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
    /// This implements a Ruff-inspired fix loop that re-parses the document after each
    /// fix, so a later rule sees what an earlier one produced (e.g., MD046 creating code
    /// blocks that MD040 needs to fix). After a fix the loop carries on with the next
    /// rule in dependency order, then wraps round to the rules before it, and stops once
    /// one full pass over every rule applies nothing.
    ///
    /// The `file_path` parameter is used to determine per-file flavor overrides. If provided,
    /// the flavor for creating LintContext will be resolved using `config.get_flavor_for_file()`.
    pub fn apply_fixes_iterative(
        &self,
        rules: &[Box<dyn Rule>],
        all_warnings: &[LintWarning], // Kept for API compatibility, but we re-check all rules
        content: &mut String,
        config: &Config,
        max_iterations: usize,
        file_path: Option<&std::path::Path>,
    ) -> Result<FixResult, String> {
        self.apply_fixes_iterative_with_paths(
            rules,
            all_warnings,
            content,
            config,
            max_iterations,
            crate::DocumentPaths::same(file_path),
        )
    }

    /// Apply fixes with separate paths for configuration matching and rule filesystem access.
    ///
    /// Native file adapters pass the same path for both. Virtual adapters can use a
    /// logical `config_path` for per-file flavor and ignores while leaving
    /// `source_file` unset so filesystem-dependent rules stay disabled.
    pub fn apply_fixes_iterative_with_paths(
        &self,
        rules: &[Box<dyn Rule>],
        _all_warnings: &[LintWarning],
        content: &mut String,
        config: &Config,
        max_iterations: usize,
        paths: crate::DocumentPaths<'_>,
    ) -> Result<FixResult, String> {
        if crate::merge_conflict::detect_for_rules(content, rules, config, paths.config_path).is_some() {
            return Ok(FixResult {
                rules_fixed: 0,
                iterations: 0,
                context_creations: 0,
                fixed_rule_names: HashSet::new(),
                converged: true,
                conflicting_rules: Vec::new(),
                conflict_cycle: Vec::new(),
            });
        }

        // Use the minimum of max_iterations parameter and MAX_ITERATIONS constant
        let max_iterations = max_iterations.min(MAX_ITERATIONS);

        // Get optimal rule order based on dependencies
        let ordered_rules = self.get_optimal_order(rules);

        let mut total_fixed = 0;
        let mut total_ctx_creations = 0;
        let mut iterations = 0;

        // History tracks (content_hash, rule_that_produced_this_state).
        // The initial entry has an empty rule name (no rule produced the initial content).
        let mut history: Vec<(u64, &str)> = vec![(hash_content(content), "")];

        // Track which rules actually applied fixes
        let mut fixed_rule_names: HashSet<&str> = HashSet::new();

        // Per-file-ignores are config-driven, per-file rule exclusions. The
        // coordinator is the single engine every fix path funnels through, so
        // resolving them here guarantees `fmt`/fix never rewrites a rule the file
        // has excluded - no caller can reintroduce issue #707 by forgetting to
        // pre-filter. Empty when no path is available (e.g. WASM without a path).
        let ignored_for_file: HashSet<String> = paths
            .config_path
            .map(|p| config.get_ignored_rules_for_file(p))
            .unwrap_or_default();

        // Where the next iteration resumes in `ordered_rules`. Restarting from the
        // first rule after every fix would re-check the rules before the one that
        // fixed on each iteration; they get the new content on the wrap instead.
        let mut resume_at = 0;

        #[cfg(not(target_arch = "wasm32"))]
        let profile = FixLoopProfile::from_env();

        // Ruff-style fix loop: keep applying fixes until content stabilizes
        while iterations < max_iterations {
            iterations += 1;

            // Create fresh context for this iteration
            // Use per-file flavor if file_path is provided, otherwise fall back to global flavor
            let flavor = paths
                .config_path
                .map_or_else(|| config.markdown_flavor(), |path| config.get_flavor_for_file(path));
            #[cfg(not(target_arch = "wasm32"))]
            let parse_start = profile.as_ref().map(|_| std::time::Instant::now());
            let ctx = crate::time_section!("fix: build context", {
                LintContext::new(content, flavor, paths.source_file.map(std::path::Path::to_path_buf))
            });
            #[cfg(not(target_arch = "wasm32"))]
            if let (Some(profile), Some(start)) = (profile.as_ref(), parse_start) {
                profile.record_context(start.elapsed());
            }
            total_ctx_creations += 1;

            // Inline `rumdl-configure-file` value overrides: when the document carries
            // inline rule-config overrides, recreate the affected rules from the merged
            // config so fixes honor them, matching the lint/diagnostics path. Without
            // this, fix() would run with the base config and could rewrite content the
            // configured rule considers valid.
            let recreated_rules: HashMap<String, Box<dyn Rule>> = {
                let inline_overrides = ctx.inline_config().get_all_rule_configs();
                if inline_overrides.is_empty() {
                    HashMap::new()
                } else {
                    let merged = config.merge_with_inline_config(ctx.inline_config());
                    inline_overrides
                        .keys()
                        .filter_map(|name| {
                            crate::rules::create_rule_by_name(name, &merged).map(|rule| (name.clone(), rule))
                        })
                        .collect()
                }
            };

            let mut any_fix_applied = false;
            // The rule that applied a fix this iteration (used for cycle reporting).
            let mut this_iter_rule: &str = "";

            // Check and fix each rule in dependency order
            let started_at = resume_at;
            for (index, rule) in ordered_rules.iter().enumerate().skip(started_at) {
                if !config_allows_fix(config, rule.name()) {
                    continue;
                }
                // Skip rules excluded for this file via [per-file-ignores].
                if ignored_for_file.contains(rule.name()) {
                    continue;
                }

                // Use the inline-config-recreated instance when present so checks and
                // fixes reflect inline `rumdl-configure-file` overrides; otherwise the
                // base rule. Rule identity (name) is unchanged either way.
                let effective_rule: &dyn Rule = recreated_rules.get(rule.name()).map_or(*rule, |r| r.as_ref());

                // Skip rules that indicate they should be skipped (opt-in rules, content-based skipping)
                if effective_rule.should_skip(&ctx) {
                    continue;
                }

                #[cfg(not(target_arch = "wasm32"))]
                let _rule_timer = profile.as_ref().map(|profile| profile.time(rule.name()));

                // Check if this rule has any current warnings
                let Ok(warnings) = crate::time_section!("fix: check rules", { effective_rule.check(&ctx) }) else {
                    continue;
                };

                if warnings.is_empty() {
                    continue;
                }

                // Filter warnings through inline config to respect disable comments
                let inline_config = ctx.inline_config();
                let filtered_warnings =
                    crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, inline_config, rule.name());

                if filtered_warnings.is_empty() {
                    continue;
                }

                // Decide whether to dispatch to rule.fix(). Two paths qualify:
                //   1. Any non-disabled warning carries an inline Fix (the
                //      common case — most rules attach per-warning edits).
                //   2. The rule advertises a fix capability via Rule::fix_capability().
                //      This is for rules whose fix() rewrites at the document
                //      level rather than producing per-warning edits (e.g.
                //      MD046 fence-style normalization).
                // A rule is skipped only when it has no inline fix AND advertises
                // no fix capability (Unfixable). Unfixable rules attach no inline
                // fixes, so in practice they are never dispatched to fix().
                let has_inline_fix = filtered_warnings.iter().any(|w| w.fix.is_some());
                let rule_advertises_fix = effective_rule.fix_capability() != FixCapability::Unfixable;
                if !has_inline_fix && !rule_advertises_fix {
                    continue;
                }

                // Apply fix
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(profile) = profile.as_ref() {
                    profile.fix_attempted(rule.name());
                }
                match crate::time_section!("fix: apply rule fixes", { effective_rule.fix(&ctx) }) {
                    Ok(fixed_content) => {
                        if fixed_content != *content {
                            *content = fixed_content;
                            total_fixed += 1;
                            any_fix_applied = true;
                            this_iter_rule = rule.name();
                            fixed_rule_names.insert(rule.name());
                            // The next rule sees the fixed content through a fresh
                            // context; after the last rule the loop wraps round.
                            resume_at = if index + 1 == ordered_rules.len() { 0 } else { index + 1 };
                            break;
                        }
                    }
                    Err(_) => {
                        // Error applying fix, continue to next rule
                        continue;
                    }
                }
            }

            // A clean scan that began part-way through the order has not seen the
            // rules before it on this content, so it wraps round to them.
            if !any_fix_applied && started_at > 0 {
                resume_at = 0;
                continue;
            }

            let current_hash = hash_content(content);

            // Check whether this content state has been seen before.
            if let Some(cycle_start) = history.iter().position(|(h, _)| *h == current_hash) {
                if cycle_start == history.len() - 1 {
                    // Content matches the last recorded state: nothing changed this iteration.
                    return Ok(FixResult {
                        rules_fixed: total_fixed,
                        iterations,
                        context_creations: total_ctx_creations,
                        fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
                        converged: true,
                        conflicting_rules: Vec::new(),
                        conflict_cycle: Vec::new(),
                    });
                } else {
                    // Content matches an older state: oscillation cycle detected.
                    // Collect the rules that participate in the cycle.
                    let conflict_cycle: Vec<String> = history[cycle_start + 1..]
                        .iter()
                        .map(|(_, r)| r.to_string())
                        .chain(std::iter::once(this_iter_rule.to_string()))
                        .filter(|r| !r.is_empty())
                        .collect();
                    let conflicting_rules: Vec<String> = history[cycle_start + 1..]
                        .iter()
                        .map(|(_, r)| *r)
                        .chain(std::iter::once(this_iter_rule))
                        .filter(|r| !r.is_empty())
                        .collect::<HashSet<&str>>()
                        .into_iter()
                        .map(std::string::ToString::to_string)
                        .collect();
                    return Ok(FixResult {
                        rules_fixed: total_fixed,
                        iterations,
                        context_creations: total_ctx_creations,
                        fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
                        converged: false,
                        conflicting_rules,
                        conflict_cycle,
                    });
                }
            }

            // New state - record it.
            history.push((current_hash, this_iter_rule));

            // If no fix was applied this iteration, content is stable.
            if !any_fix_applied {
                return Ok(FixResult {
                    rules_fixed: total_fixed,
                    iterations,
                    context_creations: total_ctx_creations,
                    fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
                    converged: true,
                    conflicting_rules: Vec::new(),
                    conflict_cycle: Vec::new(),
                });
            }
        }

        // Hit max iterations without detecting a cycle.
        Ok(FixResult {
            rules_fixed: total_fixed,
            iterations,
            context_creations: total_ctx_creations,
            fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
            converged: false,
            conflicting_rules: Vec::new(),
            conflict_cycle: Vec::new(),
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
                    fix: Some(Fix::new(0..0, String::new())),
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
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
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
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 10, None)
            .unwrap();

        // Should reach final state in one run (internally multiple iterations)
        assert_eq!(content, "Code: FENCE_LANG");
        assert_eq!(result.rules_fixed, 2);
        assert!(result.converged);
        assert!(result.iterations >= 2, "Should take at least 2 iterations for cascade");
    }

    #[test]
    fn a_rule_that_never_fires_is_checked_once_per_pass_not_once_per_fix() {
        static QUIET_CHECKS: AtomicUsize = AtomicUsize::new(0);
        let coordinator = FixCoordinator::new();

        // One rule with nothing to fix, then five that each fix once.
        let mut rules: Vec<Box<dyn Rule>> = vec![Box::new(ConditionalFixRule {
            name: "Quiet",
            check_fn: |_| {
                QUIET_CHECKS.fetch_add(1, Ordering::SeqCst);
                false
            },
            fix_fn: str::to_string,
        })];
        let fixers = [
            ConditionalFixRule {
                name: "FixA",
                check_fn: |c| c.contains('a'),
                fix_fn: |c| c.replace('a', "A"),
            },
            ConditionalFixRule {
                name: "FixB",
                check_fn: |c| c.contains('b'),
                fix_fn: |c| c.replace('b', "B"),
            },
            ConditionalFixRule {
                name: "FixC",
                check_fn: |c| c.contains('c'),
                fix_fn: |c| c.replace('c', "C"),
            },
            ConditionalFixRule {
                name: "FixD",
                check_fn: |c| c.contains('d'),
                fix_fn: |c| c.replace('d', "D"),
            },
            ConditionalFixRule {
                name: "FixE",
                check_fn: |c| c.contains('e'),
                fix_fn: |c| c.replace('e', "E"),
            },
        ];
        rules.extend(fixers.map(|rule| Box::new(rule) as Box<dyn Rule>));

        let mut content = "a b c d e".to_string();
        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &Config::default(), 100, None)
            .unwrap();

        assert_eq!(content, "A B C D E");
        assert!(result.converged);
        assert_eq!(result.rules_fixed, 5);
        // The pass that applied the five fixes, then the clean pass that proves
        // nothing is left. Restarting after each fix would check it six times.
        assert_eq!(QUIET_CHECKS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_fix_that_creates_work_for_an_earlier_rule_is_picked_up_on_the_wrap() {
        let coordinator = FixCoordinator::new();

        // The second rule's fix produces what the first rule fixes, and the
        // third has nothing to do, so the pass after the fix ends clean without
        // having looked at the first rule again.
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "Early_YToZ",
                check_fn: |content| content.contains('y'),
                fix_fn: |content| content.replace('y', "z"),
            }),
            Box::new(ConditionalFixRule {
                name: "Late_XToY",
                check_fn: |content| content.contains('x'),
                fix_fn: |content| content.replace('x', "y"),
            }),
            Box::new(ConditionalFixRule {
                name: "Tail_Quiet",
                check_fn: |_| false,
                fix_fn: str::to_string,
            }),
        ];

        let mut content = "x".to_string();
        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &Config::default(), 100, None)
            .unwrap();

        assert_eq!(content, "z");
        assert!(result.converged);
        assert_eq!(result.rules_fixed, 2);
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
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 10, None)
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
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
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
                name: "MD001",
                check_fn: |content| content.contains('A'),
                fix_fn: |content| content.replace('A', "X"),
            }),
            Box::new(ConditionalFixRule {
                name: "MD002",
                check_fn: |content| content.contains('B'),
                fix_fn: |content| content.replace('B', "Y"),
            }),
        ];

        let mut content = "AB".to_string();
        let mut config = Config::default();
        config.global.fixable = vec!["MD001".to_string()];

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
            .unwrap();

        assert_eq!(content, "XB"); // Only A->X, B unchanged
        assert_eq!(result.rules_fixed, 1);
    }

    /// Aliases in `unfixable` (e.g. `"heading-increment"`) must reach
    /// `apply_fixes_iterative` already canonicalised — the runtime invariant
    /// enforced by `Config::canonicalize_rule_lists` at every mutation
    /// boundary (`From<SourcedConfig> for Config`, LSP `apply_lsp_settings_*`,
    /// WASM `to_config_with_warnings`). The fix coordinator therefore matches
    /// against `Rule::name()` with plain string equality.
    #[test]
    fn test_unfixable_rules_resolved_from_alias() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![Box::new(ConditionalFixRule {
            name: "MD001",
            check_fn: |content| content.contains("BAD"),
            fix_fn: |content| content.replace("BAD", "GOOD"),
        })];

        let mut content = "BAD content".to_string();
        let mut config = Config::default();
        // Caller writes the alias…
        config.global.unfixable = vec!["heading-increment".to_string()];
        // …and the boundary canonicalises it to "MD001" before lint/fix sees it.
        config.canonicalize_rule_lists();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
            .unwrap();

        assert_eq!(content, "BAD content");
        assert_eq!(result.rules_fixed, 0);
        assert!(result.converged);
    }

    /// Counterpart to `test_unfixable_rules_resolved_from_alias` for the
    /// fixable allowlist. Same invariant: callers may write aliases, but the
    /// boundary canonicalises before the fix coordinator sees the config.
    #[test]
    fn test_fixable_allowlist_resolved_from_alias() {
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![Box::new(ConditionalFixRule {
            name: "MD001",
            check_fn: |content| content.contains("BAD"),
            fix_fn: |content| content.replace("BAD", "GOOD"),
        })];

        let mut content = "BAD content".to_string();
        let mut config = Config::default();
        config.global.fixable = vec!["heading-increment".to_string()];
        config.canonicalize_rule_lists();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
            .unwrap();

        assert_eq!(content, "GOOD content");
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
                    fix: Some(Fix::new(0..0, String::new())),
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
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
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
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
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
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 5, None)
            .unwrap();

        assert_eq!(content, "clean content");
        assert_eq!(result.rules_fixed, 0);
        assert!(result.converged);
    }

    #[test]
    fn test_oscillation_detection() {
        // Two rules that fight each other: Rule A changes "foo" → "bar", Rule B changes "bar" → "foo".
        // The fix loop should detect this as an oscillation cycle and stop early with
        // conflicting_rules populated rather than running all 100 iterations.
        let coordinator = FixCoordinator::new();

        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "RuleA",
                check_fn: |content| content.contains("foo"),
                fix_fn: |content| content.replace("foo", "bar"),
            }),
            Box::new(ConditionalFixRule {
                name: "RuleB",
                check_fn: |content| content.contains("bar"),
                fix_fn: |content| content.replace("bar", "foo"),
            }),
        ];

        let mut content = "foo".to_string();
        let config = Config::default();

        let result = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content, &config, 100, None)
            .unwrap();

        // Should detect the cycle quickly, not burn through all 100 iterations.
        assert!(!result.converged, "Should not converge in an oscillating pair");
        assert!(
            result.iterations < 10,
            "Cycle detection should stop well before max_iterations (got {})",
            result.iterations
        );

        // Both conflicting rules should be identified.
        let mut conflicting = result.conflicting_rules.clone();
        conflicting.sort();
        assert_eq!(
            conflicting,
            vec!["RuleA".to_string(), "RuleB".to_string()],
            "Both oscillating rules must be reported"
        );
        assert_eq!(
            result.conflict_cycle,
            vec!["RuleA".to_string(), "RuleB".to_string()],
            "Cycle should preserve the observed application order"
        );
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
                check_fn: |content| content.contains('A'),
                fix_fn: |content| content.replace('A', "B"),
            }),
            Box::new(ConditionalFixRule {
                name: "Rule2",
                check_fn: |content| content.contains('B') && !content.contains('C'),
                fix_fn: |content| content.replace('B', "BC"),
            }),
        ];

        let config = Config::default();

        // First run
        let mut content1 = "A".to_string();
        let result1 = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content1, &config, 10, None)
            .unwrap();

        // Second run on same final content
        let mut content2 = content1.clone();
        let result2 = coordinator
            .apply_fixes_iterative(&rules, &[], &mut content2, &config, 10, None)
            .unwrap();

        // Should be identical (idempotent)
        assert_eq!(content1, content2);
        assert_eq!(result2.rules_fixed, 0, "Second run should fix nothing");
        assert!(result1.converged);
        assert!(result2.converged);
    }

    #[test]
    fn test_apply_fixes_collapses_double_space_without_inline_override() {
        // Control: MD064 actually rewrites this content, so the override test below
        // is not vacuous - without an inline override the double space is collapsed.
        let mut content = String::from("`<svg>`.  Fortunately\n");
        let rules: Vec<Box<dyn Rule>> = vec![crate::rules::create_rule_by_name("MD064", &Config::default()).unwrap()];
        FixCoordinator::new()
            .apply_fixes_iterative(&rules, &[], &mut content, &Config::default(), 10, None)
            .unwrap();
        assert_eq!(
            content, "`<svg>`. Fortunately\n",
            "MD064 collapses the sentence double space when not overridden"
        );
    }

    #[test]
    fn test_per_file_ignores_skipped_even_with_unfiltered_rules() {
        // The coordinator is the single engine every fix path funnels through. Handed
        // the UNFILTERED rule set plus a file path, it must still skip any rule the
        // path excludes via [per-file-ignores] - so no caller can reintroduce #707 by
        // forgetting to pre-filter.
        let coordinator = FixCoordinator::new();
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "MD004",
                check_fn: |c| c.contains('*'),
                fix_fn: |c| c.replace('*', "-"),
            }),
            Box::new(ConditionalFixRule {
                name: "MD032",
                check_fn: |c| c.contains("PARENT"),
                fix_fn: |c| c.replace("PARENT", "parent"),
            }),
        ];

        let mut config = Config::default();
        config
            .per_file_ignores
            .insert("slides/**/*.md".to_string(), vec!["MD004".to_string()]);
        config.canonicalize_rule_lists();

        // Path matches the ignore glob: MD004 must be skipped, MD032 must still apply.
        let mut content = "* PARENT".to_string();
        let result = coordinator
            .apply_fixes_iterative(
                &rules,
                &[],
                &mut content,
                &config,
                10,
                Some(std::path::Path::new("slides/deck.md")),
            )
            .unwrap();
        assert_eq!(
            content, "* parent",
            "MD032 applied, MD004 (`*` -> `-`) skipped for slides/**"
        );
        assert!(result.fixed_rule_names.contains("MD032"));
        assert!(!result.fixed_rule_names.contains("MD004"));

        // Control: a path NOT matching the glob applies both rules, proving the skip
        // above is driven by per-file-ignores and not something else.
        let mut other = "* PARENT".to_string();
        coordinator
            .apply_fixes_iterative(
                &rules,
                &[],
                &mut other,
                &config,
                10,
                Some(std::path::Path::new("docs/other.md")),
            )
            .unwrap();
        assert_eq!(
            other, "- parent",
            "both rules apply when the path is not per-file-ignored"
        );
    }

    #[test]
    fn test_apply_fixes_honors_inline_configure_file_overrides() {
        // A document that relaxes a rule via an inline `rumdl-configure-file` override
        // must survive the fix coordinator unchanged: fixes have to honor inline value
        // overrides the same way lint/diagnostics do, otherwise "fix" rewrites content
        // the configured rule considers valid.
        let mut content = String::from(
            "<!-- rumdl-configure-file { \"MD064\": { \"allow-sentence-double-space\": true } } -->\n\n`<svg>`.  Fortunately\n",
        );
        let original = content.clone();
        let rules: Vec<Box<dyn Rule>> = vec![crate::rules::create_rule_by_name("MD064", &Config::default()).unwrap()];
        FixCoordinator::new()
            .apply_fixes_iterative(&rules, &[], &mut content, &Config::default(), 10, None)
            .unwrap();
        assert_eq!(
            content, original,
            "inline rumdl-configure-file override (allow-sentence-double-space) must prevent the MD064 fix"
        );
    }

    #[test]
    fn layout_rules_run_after_every_content_rewrite() {
        // Reflow decides wrap points from the inline content it sees, and its default
        // mode never revisits a line that is under the limit. Every rule that is not
        // downstream of MD013 must therefore be ordered before it, over the FULL rule
        // set, so a new inline rewrite (like MD054 in issue #819) is covered without
        // anyone remembering to list it.
        let rules = crate::rules::all_rules(&Config::default());
        let ordered = FixCoordinator::new().get_optimal_order(&rules);
        let names: Vec<&str> = ordered.iter().map(|r| r.name()).collect();
        assert_eq!(names.len(), rules.len(), "ordering must keep every rule exactly once");

        let md013 = names.iter().position(|&n| n == "MD013").unwrap();
        let after: Vec<&str> = names[md013 + 1..].to_vec();
        let mut expected_after = vec!["MD009", "MD012"];
        expected_after.sort_unstable();
        let mut actual_after = after.clone();
        actual_after.sort_unstable();
        assert_eq!(
            actual_after, expected_after,
            "only MD013's own dependents may follow it; order was {names:?}"
        );
    }

    #[test]
    fn link_style_rewrite_runs_before_reflow_regardless_of_input_order() {
        // Regression for issue #819: with MD054 after MD013, reflow wrapped on the
        // `[text](url)` width and the shortcut rewrite then left every line short.
        let coordinator = FixCoordinator::new();
        for input in [vec!["MD013", "MD054"], vec!["MD054", "MD013"]] {
            let rules: Vec<Box<dyn Rule>> = input
                .iter()
                .map(|name| crate::rules::create_rule_by_name(name, &Config::default()).unwrap())
                .collect();
            let names: Vec<&str> = coordinator.get_optimal_order(&rules).iter().map(|r| r.name()).collect();
            assert_eq!(names, vec!["MD054", "MD013"], "input order {input:?}");
        }
    }

    #[test]
    fn optimal_order_is_the_same_on_every_call() {
        // The prerequisite sets are hashed, and every HashMap gets its own seed, so
        // an ordering that iterated them directly would differ from one call (and
        // one process) to the next. That showed up as `rumdl fmt` producing two
        // different outputs for the same file. Rules the table leaves unordered
        // keep their input order instead.
        let rules = crate::rules::all_rules(&Config::default());
        let coordinator = FixCoordinator::new();
        let first: Vec<&str> = coordinator.get_optimal_order(&rules).iter().map(|r| r.name()).collect();
        for _ in 0..20 {
            let again: Vec<&str> = coordinator.get_optimal_order(&rules).iter().map(|r| r.name()).collect();
            assert_eq!(again, first);
        }
    }
}
