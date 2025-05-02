# Plan: Rule Parity and Improvement with markdownlint

## Objective
Achieve full rule parity, consistency, and feature completeness with [markdownlint](https://marketplace.visualstudio.com/items/?itemName=DavidAnson.vscode-markdownlint) in the rumdl project, while improving maintainability, documentation, and user experience.

---

## 1. Rule Coverage Audit
- [x] Review all rules implemented in rumdl (`src/rules/`) and compare with the [markdownlint Rules List](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md).
- [x] Identify any missing rules or rules with incomplete/ambiguous coverage.
- [x] Document any intentional differences or custom rules.

## 2. Duplicate and Fragmented Rule Consolidation
- [x] Identify rules with multiple implementations (e.g., MD011, MD029).
- [x] Consolidate into a single, canonical implementation per rule.
- [x] Archive or remove deprecated/old versions to avoid confusion.
- [x] Ensure all rule files follow the `md{number}_{description}.rs` naming pattern.
- [x] Rename all rule files to match the markdownlint naming convention for clarity and consistency.

## 3. Documentation Improvements
- [x] Ensure every rule has a corresponding documentation file in `docs/` (e.g., `md001.md`).
- [x] Update documentation to follow the required structure:
    - Purpose and rationale
    - Configuration options
    - Valid/invalid/fixed examples (with linter disabled for examples)
    - Special cases and edge cases
    - Related rules and cross-references
- [x] Document any differences from markdownlint, if intentional.

## 4. Test Coverage
- [~] Ensure each rule has a comprehensive test file in `tests/rules/`. (Most rules covered; verify edge cases)
- [~] Tests should cover valid, invalid, edge, and fix cases. (Ongoing)
- [~] Remove any test patterns that do not reflect real-world Markdown usage. (Ongoing)

## 5. Configuration and Auto-fix Support
- [~] Review each rule for configuration options (e.g., preferred list marker, heading style, etc.). (Partial)
- [~] Ensure configuration is documented and tested. (Partial)
- [~] Ensure all auto-fixable rules have robust `fix()` implementations. (Partial)
- [~] Document which rules are auto-fixable. (Partial)

## 6. Performance and Code Quality
- [~] Continue optimizing regex usage and string allocations. (Ongoing)
- [~] Minimize duplicate logic by using shared utilities where possible. (Ongoing)
- [~] Maintain consistent code style and idiomatic Rust. (Ongoing)

## 7. User Experience Enhancements
- [ ] Add a summary command or output listing enabled rules, their configuration, and auto-fix status. (Planned)
- [ ] Consider a plugin system for custom rules in the future. (Planned)

## 8. Action Items Table
| Task                     | Owner | Status         | Notes                                                      |
|--------------------------|-------|----------------|------------------------------------------------------------|
| Rule audit               |       | In Progress    | Nearly complete; minor edge case review may remain         |
| Consolidate duplicates   |       | Complete       | All major duplicates handled                               |
| Update docs              |       | Complete       | All rules have docs                                        |
| Improve tests            |       | In Progress    | Most rules have tests; verify coverage if not done         |
| Config/auto-fix review   |       | In Progress    | Some rules support this; full audit may be needed          |
| Performance/code quality |       | In Progress    | Ongoing improvements                                       |
| UX enhancements          |       | Planned        | Not started or in early stages                             |

---

## References
- [markdownlint VSCode Extension](https://marketplace.visualstudio.com/items/?itemName=DavidAnson.vscode-markdownlint)
- [markdownlint Rules Documentation](https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md)

---

*Last updated: 2024-06-10* 