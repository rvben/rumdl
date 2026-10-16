use crate::config::Config;
use crate::lint_context::LintContext;
use crate::rule::{Fix, FixCapability, LintWarning, Rule};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

#[cfg(test)]
mod diagnostic_edits_tests;

mod cleanup_effects;

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

/// Fix plans tied to one immutable source revision and configured rule instance.
/// Rules absent from this map must still run in the fix pass.
pub(crate) type FixCheckResults = HashMap<RuleCheckKey, FixPlan>;

pub(crate) enum FixPlan {
    Unchanged,
    Edits(Vec<Fix>),
    Custom,
}

impl FixPlan {
    pub(crate) fn capture(rule: &dyn Rule, ctx: &LintContext<'_>, warnings: &[LintWarning]) -> Self {
        let mut enabled = warnings
            .iter()
            .filter(|warning| crate::utils::fix_utils::warning_is_enabled(warning, ctx.inline_config(), rule.name()))
            .peekable();
        if enabled.peek().is_none() {
            return Self::Unchanged;
        }
        if !rule.fixes_from_diagnostics() {
            return Self::Custom;
        }
        let edits: Vec<_> = enabled.filter_map(|warning| warning.fix.clone()).collect();
        if edits.is_empty() {
            Self::Unchanged
        } else {
            Self::Edits(edits)
        }
    }
}

/// Raw findings checked on one immutable document version. Never carried across
/// a rewrite; report filtering belongs to the lint driver, after these checks.
type RuleCheckKey = (std::any::TypeId, *const ());
pub(crate) type RawRuleChecks = HashMap<RuleCheckKey, Vec<LintWarning>>;

/// Identify the selected configured instance, including when callers supply two
/// instances with the same name. Keys live only for this invocation.
pub(crate) fn rule_check_key(rule: &dyn Rule) -> RuleCheckKey {
    // Zero-sized rules may share a data address; their types distinguish them.
    (rule.as_any().type_id(), std::ptr::from_ref(rule).cast::<()>())
}
type StableAnalysis<'a> = dyn FnMut(&LintContext<'_>, &mut RawRuleChecks) + 'a;

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
/// checked, how often diagnostic edits or its custom `fix()` were attempted
/// (whether or not anything changed), and the time both took together.
#[cfg(not(target_arch = "wasm32"))]
struct FixLoopProfile {
    contexts: std::cell::Cell<usize>,
    handoffs: std::cell::Cell<usize>,
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
            handoffs: std::cell::Cell::new(0),
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

        eprintln!("[FIX]  shared parser ({} handoffs)", self.handoffs.get());
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
    dependencies: Cow<'static, RuleDependencies>,
    cleanup_effects: bool,
}

type RuleDependencies = HashMap<&'static str, Vec<&'static str>>;

static DEFAULT_DEPENDENCIES: LazyLock<RuleDependencies> = LazyLock::new(FixCoordinator::build_dependencies);

struct CachedRuleOrder {
    names: Vec<&'static str>,
    positions: Vec<usize>,
}

thread_local! {
    // Keep one selection per worker. Only indices survive across documents,
    // never configured rule instances or document facts.
    static RULE_ORDER: RefCell<Option<CachedRuleOrder>> = const { RefCell::new(None) };
}

impl Default for FixCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl FixCoordinator {
    pub fn new() -> Self {
        Self {
            dependencies: Cow::Borrowed(&DEFAULT_DEPENDENCIES),
            cleanup_effects: false,
        }
    }

    /// Opt in only for the formatter, which does not expose per-revision analysis.
    pub fn for_formatter() -> Self {
        Self {
            cleanup_effects: true,
            ..Self::new()
        }
    }

    fn build_dependencies() -> RuleDependencies {
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

        dependencies
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
        // A private, modified graph is sorted directly, including after it has
        // been changed. The shared graph's order depends only on input names.
        let shared_graph = matches!(&self.dependencies, Cow::Borrowed(graph) if std::ptr::eq(*graph, LazyLock::force(&DEFAULT_DEPENDENCIES)));
        if shared_graph
            && let Some(ordered) = RULE_ORDER.with_borrow(|cache| {
                let cache = cache.as_ref()?;
                (cache.names.len() == rules.len()
                    && cache.names.iter().zip(rules).all(|(name, rule)| *name == rule.name()))
                .then(|| cache.positions.iter().map(|&index| rules[index].as_ref()).collect())
            })
        {
            return ordered;
        }
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
        for (prereq, dependents) in self.dependencies.iter() {
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

        if shared_graph {
            RULE_ORDER.with_borrow_mut(|cache| {
                *cache = Some(CachedRuleOrder {
                    names: rules.iter().map(|rule| rule.name()).collect(),
                    positions: sorted.iter().map(|rule| position[rule.name()]).collect(),
                });
            });
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
        // The fix loop works on analysis text; the mark is put back afterwards.
        let had_bom = crate::utils::bom::has_bom(content);
        if had_bom {
            content.remove(0);
        }
        let result = self.apply_fixes_with_analysis(rules, content, config, max_iterations, paths, None, None);
        if had_bom {
            content.insert(0, crate::utils::bom::BOM);
        }
        Ok(result)
    }

    /// Fix Markdown extracted from a document, such as a fenced block or doc
    /// comment. A leading U+FEFF belongs to this fragment's content and is never
    /// interpreted as a file encoding marker. Input uses LF line endings.
    pub fn apply_fixes_to_fragment(
        &self,
        rules: &[Box<dyn Rule>],
        content: &mut String,
        config: &Config,
        max_iterations: usize,
    ) -> FixResult {
        self.apply_fixes_with_analysis(
            rules,
            content,
            config,
            max_iterations,
            crate::DocumentPaths::same(None),
            None,
            None,
        )
    }

    /// Reuse checks only while their original source is still current. `content`
    /// is analysis text: its byte order mark, if any, was removed by the caller.
    /// The analysis belongs to this invocation, never to a later document version.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_fixes_with_analysis(
        &self,
        rules: &[Box<dyn Rule>],
        content: &mut String,
        config: &Config,
        max_iterations: usize,
        paths: crate::DocumentPaths<'_>,
        initial: Option<(&LintContext<'_>, &FixCheckResults)>,
        mut on_stable: Option<&mut StableAnalysis<'_>>,
    ) -> FixResult {
        // The caller may supply analysis only for this exact content version.
        let initial = initial.filter(|(ctx, _)| ctx.content == content);
        let conflict = initial.map_or_else(
            || crate::merge_conflict::detect_for_analysis(content, rules, config, paths.config_path),
            |(ctx, _)| crate::merge_conflict::detect_for_context(ctx, rules, config, paths.config_path),
        );
        if conflict.is_some() {
            return FixResult {
                rules_fixed: 0,
                iterations: 0,
                context_creations: 0,
                fixed_rule_names: HashSet::new(),
                converged: true,
                conflicting_rules: Vec::new(),
                conflict_cycle: Vec::new(),
            };
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

        let proof_policies: Vec<_> = ordered_rules
            .iter()
            .map(|rule| crate::format_proof::NativeFormatPolicy::validated(*rule))
            .collect();

        // Where the next iteration resumes in `ordered_rules`. A scan wraps
        // around on the same context until every rule has seen this content.
        // Only a rewrite invalidates the context and its lazily parsed caches.
        let mut resume_at = 0;

        let mut parse_handoff = None;

        #[cfg(not(target_arch = "wasm32"))]
        let profile = FixLoopProfile::from_env();

        // Ruff-style fix loop: keep applying fixes until content stabilizes
        while iterations < max_iterations {
            iterations += 1;

            // Reuse the initial analysis until a rewrite requires a fresh context.
            // Use per-file flavor if a config path is provided, otherwise the global flavor.
            let flavor = paths
                .config_path
                .map_or_else(|| config.markdown_flavor(), |path| config.get_flavor_for_file(path));

            let mut proof = if initial.is_none() && on_stable.is_none() {
                if parse_handoff.is_some() {
                    crate::format_proof::FormatProof::analyze_handoff(content, flavor, &mut parse_handoff)
                } else {
                    crate::format_proof::FormatProof::analyze(content, flavor)
                }
            } else {
                crate::format_proof::FormatProof::default()
            };

            let proof_skips: Vec<_> = proof_policies
                .iter()
                .map(|policy| policy.is_some_and(|policy| policy.proven(&proof)))
                .collect();

            let has_pending = ordered_rules.iter().enumerate().any(|(index, rule)| {
                config_allows_fix(config, rule.name())
                    && !ignored_for_file.contains(rule.name())
                    && rule.fix_capability() != FixCapability::Unfixable
                    && !proof_skips[index]
            });
            if !has_pending
                && proof.eligible()
                && initial.is_none()
                && on_stable.is_none()
                && !crate::inline_config::has_inline_config_markers(content)
            {
                return FixResult {
                    rules_fixed: total_fixed,
                    iterations,
                    context_creations: total_ctx_creations,
                    fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
                    converged: true,
                    conflicting_rules: Vec::new(),
                    conflict_cycle: Vec::new(),
                };
            }
            #[cfg(not(target_arch = "wasm32"))]
            let parse_start = profile.as_ref().map(|_| std::time::Instant::now());
            let reused = initial.filter(|_| iterations == 1);
            let mut fresh_ctx = None;
            let ctx = if let Some((ctx, _)) = reused {
                ctx
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                if parse_handoff.is_some()
                    && let Some(profile) = profile.as_ref()
                {
                    profile.handoffs.set(profile.handoffs.get() + 1);
                }
                let ctx = fresh_ctx.insert(crate::time_section!("fix: build context", {
                    LintContext::new_with_proof(
                        content,
                        flavor,
                        paths.source_file.map(std::path::Path::to_path_buf),
                        proof.take_parsed(),
                        parse_handoff.take(),
                    )
                }));
                #[cfg(not(target_arch = "wasm32"))]
                if let (Some(profile), Some(start)) = (profile.as_ref(), parse_start) {
                    profile.record_context(start.elapsed());
                }
                total_ctx_creations += 1;
                ctx
            };

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

            let mut current_checks = RawRuleChecks::new();
            let mut next_content = None;
            let mut any_fix_applied = false;
            // The rule that applied a fix this iteration (used for cycle reporting).
            let mut this_iter_rule: &str = "";

            // A configured no-op needs no syntax proof. Keep every diagnostic
            // consumer and document-level override on the normal check path.

            let configuration_only =
                initial.is_none() && on_stable.is_none() && ctx.inline_config().get_all_rule_configs().is_empty();

            let cleanup_enabled =
                self.cleanup_effects && initial.is_none() && on_stable.is_none() && cleanup_effects::eligible(ctx);

            let mut cleanup = cleanup_effects::CleanupBatch::default();

            // Continue in dependency order, wrapping to the earlier rules on
            // this same context. If nothing changes, each rule is checked once
            // and the document is stable; there is no reason to parse it again.
            let scan = ordered_rules
                .iter()
                .enumerate()
                .skip(resume_at)
                .chain(ordered_rules.iter().enumerate().take(resume_at));
            for (index, rule) in scan {
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

                // The lint pass reports warning-only rules. Rechecking them
                // after every rewrite cannot produce a CLI fix and can repeat
                // expensive work such as filesystem link validation. Read the
                // effective instance so inline settings can still enable fixes.
                if effective_rule.fix_capability() == FixCapability::Unfixable {
                    continue;
                }

                if configuration_only
                    && crate::format_proof::NativeFormatPolicy::validated(effective_rule)
                        .is_some_and(crate::format_proof::NativeFormatPolicy::configuration_only)
                {
                    continue;
                }
                // Proofs and negative checks belong to the old source too.
                // Cross a pending edit only for a built-in policy whose inputs
                // are preserved by every accepted cleanup effect.

                if !cleanup.is_empty() && !cleanup.can_check(index, effective_rule, ctx) {
                    break;
                }

                if proof_skips[index] && !recreated_rules.contains_key(rule.name()) {
                    continue;
                }
                if effective_rule.should_skip(ctx) {
                    continue;
                }

                #[cfg(not(target_arch = "wasm32"))]
                let _rule_timer = profile.as_ref().map(|profile| profile.time(rule.name()));

                // A diagnostic edit belongs to this exact source revision. A
                // rewrite discards every plan, including cached negative checks.
                let fresh_plan;
                let mut cleanup_edits = None;
                let mut prepared_native_fix = None;
                let mut checked_warnings = None;
                let plan = match reused.and_then(|(_, checks)| checks.get(&rule_check_key(*rule))) {
                    Some(plan) => plan,
                    None => {
                        let Ok(warnings) = crate::time_section!("fix: check rules", {
                            effective_rule.check_with_prepared_fix(ctx).map(|(warnings, prepared)| {
                                prepared_native_fix = prepared.map(Ok);
                                warnings
                            })
                        }) else {
                            if !cleanup.is_empty() {
                                break;
                            }
                            continue;
                        };
                        fresh_plan = FixPlan::capture(effective_rule, ctx, &warnings);

                        if cleanup_enabled {
                            cleanup_edits = cleanup_effects::capture_with_native(
                                effective_rule,
                                ctx,
                                &warnings,
                                &mut prepared_native_fix,
                            );
                        }
                        if on_stable.is_some() {
                            current_checks.insert(rule_check_key(*rule), warnings);
                        } else {
                            checked_warnings = Some(warnings);
                        }
                        &fresh_plan
                    }
                };
                if matches!(plan, FixPlan::Unchanged) {
                    continue;
                }

                if cleanup_enabled {
                    if let Some(edits) = cleanup_edits
                        && cleanup.try_push(ctx.content, index, effective_rule.name(), edits, &history)
                    {
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some(profile) = profile.as_ref() {
                            profile.fix_attempted(rule.name());
                        }
                        if cleanup.terminal() || cleanup.len() > max_iterations - iterations {
                            break;
                        }
                        continue;
                    }
                    if !cleanup.is_empty() {
                        break;
                    }
                }

                // Apply fix
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(profile) = profile.as_ref() {
                    profile.fix_attempted(rule.name());
                }
                let fixed = crate::time_section!("fix: apply rule fixes", {
                    match plan {
                        FixPlan::Edits(edits) => crate::utils::fix_utils::apply_edits(ctx.content, edits.iter())
                            .map(Cow::into_owned)
                            .map_err(crate::rule::LintError::FixFailed),
                        FixPlan::Custom => prepared_native_fix.unwrap_or_else(|| {
                            checked_warnings.as_deref().map_or_else(
                                || effective_rule.fix(ctx),
                                |warnings| effective_rule.fix_with_diagnostics(ctx, warnings),
                            )
                        }),
                        FixPlan::Unchanged => unreachable!(),
                    }
                });
                match fixed {
                    Ok(fixed_content) => {
                        if fixed_content != *content {
                            parse_handoff = ctx.take_parse_handoff(&fixed_content);
                            next_content = Some(fixed_content);
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

            if let Some(batch) = cleanup.finish() {
                let count = batch.stages.len();
                for &(hash, name, _) in batch.stages.iter().take(count - 1) {
                    history.push((hash, name));
                }
                for &(_, name, _) in &batch.stages {
                    fixed_rule_names.insert(name);
                }
                let &(_, name, index) = batch.stages.last().unwrap();
                this_iter_rule = name;
                resume_at = if index + 1 == ordered_rules.len() { 0 } else { index + 1 };
                total_fixed += count;
                iterations += count - 1;
                any_fix_applied = true;

                // A single-stage native guard may already have parsed this
                // exact output. Combined edits usually differ, so the
                // handoff's full byte comparison remains authoritative.
                parse_handoff = ctx.take_parse_handoff(&batch.content);
                next_content = Some(batch.content);
            }

            if !any_fix_applied && let Some(analyze) = &mut on_stable {
                analyze(ctx, &mut current_checks);
            }

            if let Some(fixed) = next_content {
                *content = fixed;
            }
            let current_hash = hash_content(content);

            // Check whether this content state has been seen before.
            if let Some(cycle_start) = history.iter().position(|(h, _)| *h == current_hash) {
                if cycle_start == history.len() - 1 {
                    // Content matches the last recorded state: nothing changed this iteration.
                    return FixResult {
                        rules_fixed: total_fixed,
                        iterations,
                        context_creations: total_ctx_creations,
                        fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
                        converged: true,
                        conflicting_rules: Vec::new(),
                        conflict_cycle: Vec::new(),
                    };
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
                    return FixResult {
                        rules_fixed: total_fixed,
                        iterations,
                        context_creations: total_ctx_creations,
                        fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
                        converged: false,
                        conflicting_rules,
                        conflict_cycle,
                    };
                }
            }

            // New state - record it.
            history.push((current_hash, this_iter_rule));

            // If no fix was applied this iteration, content is stable.
            if !any_fix_applied {
                return FixResult {
                    rules_fixed: total_fixed,
                    iterations,
                    context_creations: total_ctx_creations,
                    fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
                    converged: true,
                    conflicting_rules: Vec::new(),
                    conflict_cycle: Vec::new(),
                };
            }
        }

        // Hit max iterations without detecting a cycle.
        FixResult {
            rules_fixed: total_fixed,
            iterations,
            context_creations: total_ctx_creations,
            fixed_rule_names: fixed_rule_names.iter().map(std::string::ToString::to_string).collect(),
            converged: false,
            conflicting_rules: Vec::new(),
            conflict_cycle: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conflict_guard_discards_analysis_from_another_content_version() {
        let config = super::Config::default();
        let rules = crate::rules::all_rules(&config);
        let old = super::LintContext::new("# Safe\n", config.markdown_flavor(), None);
        let checks = super::FixCheckResults::new();
        let original = "<<<<<<< HEAD\ntext   \n";
        let mut content = original.to_string();
        let result = super::FixCoordinator::new().apply_fixes_with_analysis(
            &rules,
            &mut content,
            &config,
            100,
            crate::DocumentPaths::same(None),
            Some((&old, &checks)),
            None,
        );
        assert_eq!(content, original);
        assert_eq!(result.iterations, 0);
        assert_eq!(result.rules_fixed, 0);
    }

    use super::*;
    use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn forwarding_a_policy_does_not_certify_a_custom_rule() {
        #[derive(Clone)]
        struct Wrapper(crate::rules::MD013LineLength);
        impl Rule for Wrapper {
            fn name(&self) -> &'static str {
                "MD013"
            }
            fn description(&self) -> &'static str {
                "Custom rewrite"
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
                self.0.native_format_policy()
            }
            fn check(&self, ctx: &LintContext) -> LintResult {
                Ok((!ctx.content.contains("custom"))
                    .then(|| LintWarning {
                        rule_name: Some(self.name().into()),
                        message: self.description().into(),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 2,
                        severity: Severity::Warning,
                        fix: None,
                    })
                    .into_iter()
                    .collect())
            }
            fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
                Ok(if ctx.content.contains("custom") {
                    ctx.content.into()
                } else {
                    format!("custom {}", ctx.content)
                })
            }
        }
        let rule = Wrapper(crate::rules::MD013LineLength::default());
        assert!(rule.native_format_policy().is_some());
        assert!(crate::format_proof::NativeFormatPolicy::validated(&rule).is_none());
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(rule)];
        let config = Config::default();
        let (text, result) = crate::document_run::DocumentRun::new("plain\n", &rules, &config)
            .fix_for_formatter(10)
            .unwrap();
        assert_eq!(text, "custom plain\n");
        assert!(result.converged);
    }

    #[test]
    fn configured_noops_keep_flavor_diagnostics_and_inline_fixes() {
        #[derive(Clone)]
        struct CountNative {
            rule: Box<dyn Rule>,
            checks: std::sync::Arc<AtomicUsize>,
        }
        impl Rule for CountNative {
            fn name(&self) -> &'static str {
                self.rule.name()
            }
            fn description(&self) -> &'static str {
                self.rule.description()
            }
            fn check(&self, ctx: &LintContext) -> LintResult {
                self.checks.fetch_add(1, Ordering::Relaxed);
                self.rule.check(ctx)
            }
            fn fix(&self, ctx: &LintContext) -> Result<String, LintError> {
                self.rule.fix(ctx)
            }
            fn should_skip(&self, ctx: &LintContext) -> bool {
                self.rule.should_skip(ctx)
            }
            fn native_format_policy(&self) -> Option<crate::format_proof::NativeFormatPolicy> {
                self.rule
                    .native_format_policy()
                    .map(crate::format_proof::NativeFormatPolicy::for_test_owner::<Self>)
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
        for flavor in crate::format_proof::ALL_FLAVORS {
            let mut config = Config::default();
            config.global.flavor = flavor;
            let counters = [
                std::sync::Arc::new(AtomicUsize::new(0)),
                std::sync::Arc::new(AtomicUsize::new(0)),
            ];
            let rules: Vec<Box<dyn Rule>> = ["MD013", "MD036"]
                .into_iter()
                .zip(&counters)
                .map(|(name, checks)| {
                    Box::new(CountNative {
                        rule: crate::rules::create_rule_by_name(name, &config).unwrap(),
                        checks: checks.clone(),
                    }) as Box<dyn Rule>
                })
                .collect();
            let original = format!("*Heading*\n\n{}\n", "ordinary prose ".repeat(10));
            let mut content = original.clone();
            let coordinator = FixCoordinator::new();
            let result = coordinator.apply_fixes_with_analysis(
                &rules,
                &mut content,
                &config,
                10,
                crate::DocumentPaths::same(None),
                None,
                None,
            );
            assert!(result.converged);
            assert_eq!(content, original, "{flavor:?}");
            assert!(
                counters.iter().all(|count| count.load(Ordering::Relaxed) == 0),
                "{flavor:?}"
            );

            let mut warning_count = 0;
            let mut stable = |_: &LintContext, checks: &mut RawRuleChecks| {
                warning_count = checks.values().map(Vec::len).sum::<usize>();
            };
            coordinator.apply_fixes_with_analysis(
                &rules,
                &mut content,
                &config,
                10,
                crate::DocumentPaths::same(None),
                None,
                Some(&mut stable),
            );
            assert!(warning_count >= 2, "{flavor:?}: diagnostic consumer lost warnings");
            assert!(
                counters.iter().all(|count| count.load(Ordering::Relaxed) == 1),
                "{flavor:?}"
            );

            for (name, original) in [
                (
                    "MD036",
                    "<!-- rumdl-configure-file {\"MD036\":{\"fix\":true}} -->\n\n*Heading*\n",
                ),
                (
                    "MD013",
                    "<!-- rumdl-configure-file {\"MD013\":{\"reflow\":true,\"line_length\":12}} -->\n\nalpha beta gamma delta epsilon zeta\n",
                ),
            ] {
                let ctx = LintContext::new(original, flavor, None);
                let merged = config.merge_with_inline_config(ctx.inline_config());
                let native = crate::rules::create_rule_by_name(name, &merged).unwrap();
                let expected = native.fix(&ctx).unwrap();
                assert_ne!(
                    expected, original,
                    "{flavor:?} {name}: fixture must exercise a native edit"
                );
                let rules = vec![crate::rules::create_rule_by_name(name, &config).unwrap()];
                let mut content = original.to_owned();
                coordinator.apply_fixes_with_analysis(
                    &rules,
                    &mut content,
                    &config,
                    10,
                    crate::DocumentPaths::same(None),
                    None,
                    None,
                );
                assert_eq!(
                    content, expected,
                    "{flavor:?} {name}: inline override lost its native fix"
                );
            }
        }
    }

    #[derive(Clone)]
    struct WarningOnlyProbe;

    impl Rule for WarningOnlyProbe {
        fn name(&self) -> &'static str {
            "TEST_WARNING_ONLY"
        }
        fn description(&self) -> &'static str {
            "A diagnostic that cannot change content"
        }
        fn fix_capability(&self) -> FixCapability {
            FixCapability::Unfixable
        }
        fn check(&self, _ctx: &LintContext) -> LintResult {
            panic!("warning-only rules must not be rechecked in the fix loop")
        }
        fn fix(&self, _ctx: &LintContext) -> Result<String, LintError> {
            panic!("warning-only rules must not be fixed")
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn fix_loop_skips_warning_only_rules_without_losing_other_fixes() {
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(WarningOnlyProbe),
            Box::new(ConditionalFixRule {
                name: "TEST_FIX",
                check_fn: |s| s.contains("bad"),
                fix_fn: |s| s.replace("bad", "good"),
            }),
        ];
        let mut content = "bad\n".to_string();
        let result = FixCoordinator::new()
            .apply_fixes_iterative(&rules, &[], &mut content, &Config::default(), 10, None)
            .unwrap();
        assert!(result.converged);
        assert_eq!(content, "good\n");
    }

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
        // third has nothing to do. Wrapping around must find the earlier rule
        // without rebuilding the unchanged context or using up the fix budget.
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
            .apply_fixes_iterative(&rules, &[], &mut content, &Config::default(), 3, None)
            .unwrap();

        assert_eq!(content, "z");
        assert!(result.converged);
        assert_eq!(result.rules_fixed, 2);
        assert_eq!(result.context_creations, 3);
    }

    #[test]
    fn a_clean_wrap_reuses_the_context_and_does_not_repeat_the_tail() {
        static TAIL_CHECKS: AtomicUsize = AtomicUsize::new(0);
        TAIL_CHECKS.store(0, Ordering::SeqCst);
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(ConditionalFixRule {
                name: "Early_Quiet",
                check_fn: |_| false,
                fix_fn: str::to_string,
            }),
            Box::new(ConditionalFixRule {
                name: "Middle_Fix",
                check_fn: |content| content.contains('x'),
                fix_fn: |content| content.replace('x', "y"),
            }),
            Box::new(ConditionalFixRule {
                name: "Tail_Quiet",
                check_fn: |_| {
                    TAIL_CHECKS.fetch_add(1, Ordering::SeqCst);
                    false
                },
                fix_fn: str::to_string,
            }),
        ];
        let mut content = "x".to_string();
        let result = FixCoordinator::new()
            .apply_fixes_iterative(&rules, &[], &mut content, &Config::default(), 2, None)
            .unwrap();

        assert_eq!(content, "y");
        assert!(result.converged);
        assert_eq!(result.rules_fixed, 1);
        assert_eq!(result.context_creations, 2);
        assert_eq!(TAIL_CHECKS.load(Ordering::SeqCst), 1);
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
        coordinator.dependencies.to_mut().insert("RuleA", vec!["RuleB"]);
        coordinator.dependencies.to_mut().insert("RuleB", vec!["RuleC"]);
        coordinator.dependencies.to_mut().insert("RuleC", vec!["RuleA"]);

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

    #[test]
    fn cached_order_respects_the_selection_and_current_rule_instances() {
        let config = Config::default();
        let original = crate::rules::all_rules(&config);
        let coordinator = FixCoordinator::new();
        coordinator.get_optimal_order(&original);

        // The owned graph is the uncached reference, including when a caller
        // alternates selected rules or their input order between documents.
        let mut uncached = FixCoordinator::new();
        uncached.dependencies.to_mut();
        for selection in [
            original.iter().map(|rule| rule.name()).collect::<Vec<_>>(),
            vec!["MD013", "MD054", "MD009"],
            vec!["MD013", "MD054", "MD009"],
            vec!["MD009", "MD054", "MD013"],
            vec![],
            vec!["MD013"],
        ] {
            let current: Vec<Box<dyn Rule>> = selection
                .iter()
                .map(|name| crate::rules::create_rule_by_name(name, &config).unwrap())
                .collect();
            let ordered = coordinator.get_optimal_order(&current);
            let expected = uncached.get_optimal_order(&current);
            assert_eq!(
                ordered.iter().map(|rule| rule.name()).collect::<Vec<_>>(),
                expected.iter().map(|rule| rule.name()).collect::<Vec<_>>()
            );
            for rule in ordered {
                assert!(current.iter().any(|current| std::ptr::eq(rule, current.as_ref())));
            }
        }
    }
}
