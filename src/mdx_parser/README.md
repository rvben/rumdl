# Bundled markdown-rs parser

Source: <https://github.com/wooorm/markdown-rs>, release `markdown` 1.0.0,
commit `1506572f9b406431402928f3a8b3df0b4ae2d8f5` (2025-04-23).
The Rust sources were copied from the crates.io release. The upstream MIT
copyright and license are preserved in [LICENSE](LICENSE).

This is a private rumdl module, bundled in its source package so all distribution
targets use the same parser fixes. Oxc still parses JavaScript; pulldown-cmark
remains rumdl's primary Markdown parser.

Integration adaptations:

- Sources use rumdl's rustfmt configuration.
- Absolute `crate::` paths point into `crate::mdx_parser`.
- The module uses the parent library's `alloc` import; crate-only attributes
  are removed, and upstream lint policy is isolated from rumdl's lint policy.
- The HTML renderer is compiled only for parser regression tests.
- Standalone-crate documentation examples are marked `ignore`; upstream inline
  unit tests remain enabled. Upstream optional `serde` and `log` features remain
  explicitly disabled with `cfg(any())`, as they were in rumdl's dependency
  configuration.

Parser fixes:

- Unclosed JSX in a link or image label returns `end-tag-mismatch` in both
  debug and release builds instead of asserting or accepting an invalid tree.
- JSX opened outside a link or image label cannot close inside its buffer;
  this returns `end-tag-mismatch` instead of panicking.
- ATX closing sequences and Setext underlines find their heading ancestor when
  unclosed JSX is above it on the compilation stack; closing the heading reports
  `end-tag-mismatch`.

## Incorporated upstream patches

The following parser patches are incorporated at the listed source revisions.
Dependent changes are applied once. Bindings, new crates, serializer changes,
and unrelated extension/API proposals are outside this parser snapshot.

| PR                                                     | Source revision | Change                                                                    |
| ------------------------------------------------------ | --------------- | ------------------------------------------------------------------------- |
| [#206](https://github.com/wooorm/markdown-rs/pull/206) | `aa28ef46f5c9`  | test: panic on `<![C` followed by empty `&#;`                             |
| [#207](https://github.com/wooorm/markdown-rs/pull/207) | `d2efc4814ce8`  | test: more link-shape variants of `subtokenize.rs:149` panic (GH-79)      |
| [#210](https://github.com/wooorm/markdown-rs/pull/210) | `79d3a45e94d9`  | test: more setext-underline variants of `to_html.rs:197` panic (GH-22)    |
| [#216](https://github.com/wooorm/markdown-rs/pull/216) | `1b4eeafaf903`  | Improve source location lookup performance                                |
| [#219](https://github.com/wooorm/markdown-rs/pull/219) | `a1129c332d8a`  | fix: do not panic on MDX setext heading with unclosed JSX                 |
| [#221](https://github.com/wooorm/markdown-rs/pull/221) | `e40411e6e285`  | Remove unused extern crate std in gfm_tagfilter.rs                        |
| [#231](https://github.com/wooorm/markdown-rs/pull/231) | `5a23439ea34c`  | Improve performance of Unicode punctuation lookup                         |
| [#232](https://github.com/wooorm/markdown-rs/pull/232) | `b2c4009e30ab`  | Box the error payload of `State`                                          |
| [#233](https://github.com/wooorm/markdown-rs/pull/233) | `e9764f4159a1`  | Improve performance of the edit map                                       |
| [#234](https://github.com/wooorm/markdown-rs/pull/234) | `0aad229426ae`  | Improve performance of emphasis matching                                  |
| [#235](https://github.com/wooorm/markdown-rs/pull/235) | `e6531213d930`  | Improve performance by scanning to the next special character             |
| [#236](https://github.com/wooorm/markdown-rs/pull/236) | `20eb59d08ebf`  | Fix attention flanking next to other markers                              |
| [#237](https://github.com/wooorm/markdown-rs/pull/237) | `38c3268a75b1`  | Fix the rule of 3 to count whole delimiter runs                           |
| [#238](https://github.com/wooorm/markdown-rs/pull/238) | `455b9609fd24`  | Improve performance of `to_mdast` on deep trees                           |
| [#239](https://github.com/wooorm/markdown-rs/pull/239) | `e3c498a34a6e`  | Improve performance of labels that can't be defined                       |
| [#240](https://github.com/wooorm/markdown-rs/pull/240) | `128d3a2136d1`  | Improve performance of escaped pipes in table code                        |
| [#241](https://github.com/wooorm/markdown-rs/pull/241) | `efbd4037f011`  | Make `Node::to_string` iterative                                          |
| [#242](https://github.com/wooorm/markdown-rs/pull/242) | `e2c2e70fda5b`  | Improve performance of container continuation                             |
| [#243](https://github.com/wooorm/markdown-rs/pull/243) | `d195d2a707bf`  | Improve performance of labels in image alt text                           |
| [#244](https://github.com/wooorm/markdown-rs/pull/244) | `cf1c34b5be53`  | Improve performance of definition lookups                                 |
| [#245](https://github.com/wooorm/markdown-rs/pull/245) | `702e5c5f7c70`  | Improve performance of marking link starts inactive                       |
| [#246](https://github.com/wooorm/markdown-rs/pull/246) | `a80d8754e078`  | Improve performance of unclosed inline HTML                               |
| [#247](https://github.com/wooorm/markdown-rs/pull/247) | `9141ce07fe90`  | Improve performance of unclosed code spans                                |
| [#248](https://github.com/wooorm/markdown-rs/pull/248) | `5ea813b3c5ee`  | Improve performance of failed autolink literals                           |
| [#249](https://github.com/wooorm/markdown-rs/pull/249) | `4e0aa6b04210`  | Build the MDX line index only for errors                                  |
| [#250](https://github.com/wooorm/markdown-rs/pull/250) | `ed81c8903d35`  | Improve performance of collecting MDX expressions                         |
| [#251](https://github.com/wooorm/markdown-rs/pull/251) | `296d34872fae`  | Improve performance of failed MDX flow expressions                        |
| [#252](https://github.com/wooorm/markdown-rs/pull/252) | `9a099b56abbc`  | Add an opt-in budget for reading MDX expressions and ESM                  |
| [#253](https://github.com/wooorm/markdown-rs/pull/253) | `872b559ad670`  | Add an optional `memchr` feature for finding line ends                    |
| [#256](https://github.com/wooorm/markdown-rs/pull/256) | `20f8e42c5021`  | Forbid unsafe code in all crates                                          |
| [#257](https://github.com/wooorm/markdown-rs/pull/257) | `4f0844225673`  | Fix link titles in parentheses to not contain an unescaped `(`            |
| [#263](https://github.com/wooorm/markdown-rs/pull/263) | `bfd2781d5074`  | Fix a panic on email addresses in image titles with GFM                   |
| [#264](https://github.com/wooorm/markdown-rs/pull/264) | `81886dc0cc4b`  | Fix tables and HTML after a table head row and a lazy line                |
| [#265](https://github.com/wooorm/markdown-rs/pull/265) | `e26c525bd713`  | Fix a panic on a character reference at the end of an MDX attribute value |
| [#266](https://github.com/wooorm/markdown-rs/pull/266) | `836b60fbe24e`  | Fix label matching for labels without leading whitespace                  |
| [#267](https://github.com/wooorm/markdown-rs/pull/267) | `ccc6ed01ea1c`  | Fix lists after an item that ends in unclosed code                        |
| [#268](https://github.com/wooorm/markdown-rs/pull/268) | `663f4ad6278d`  | Fix an extra line ending in unclosed code at the end of a container       |
| [#269](https://github.com/wooorm/markdown-rs/pull/269) | `0471e9bcb23c`  | End lists before trailing blank lines in mdast                            |

## Integration details

- #233's append-log edit map supersedes the alternatives in #197 and #217
  and the earlier local BTreeMap implementation.
- #237 uses whole delimiter-run lengths with #234's delimiter stack. Rejected
  opener bounds are clamped to the surviving stack after a match. Its whole-run
  regression replaces #234's assertion of the old rule-of-three bug.
- #243 retains lazy identifier normalization for resource labels.
- #219 is adapted to #238's open-node stack. The malformed-label fixes also
  check that stack before resuming a buffer or closing a JSX element.
- #267 removes #242's coalescing of adjacent item ends. Items after an unclosed
  fence merge correctly; #268 determines their rendered code line endings.
- #269 intentionally excludes trailing blank lines from list, item, and footnote
  ranges. An unclosed code/math child is bounded by its container when only a
  line ending crosses the boundary, fixing the regression in
  [#205](https://github.com/wooorm/markdown-rs/pull/205).
- Native MDX link facts bypass redundant primary-parser image-label extraction,
  while preserving unresolved references and malformed-document fallback.
- Rumdl obtains exact JSX attribute-expression ranges from parser events,
  preserving closing braces after indentation, tabs, and container prefixes.
- The optional memchr scanner is always enabled through rumdl's existing
  dependency. No new Cargo feature is exposed.
- The optional MDX parse budget defaults to disabled, as proposed upstream.
  Rumdl does not impose a new parsing or document-size limit.

`pending_regressions.rs` retains regression assertions from the applicable PRs.
The MDX collection and budget test modules retain upstream callback tests.
`tests.rs` covers JSX boundaries, unterminated attribute references, container
ranges, and differential checks against the dev-only upstream crate. Those
comparisons correct only the documented upstream container-range bug; other
AST fields, positions, and errors are compared exactly.

Retain the license and patch provenance when updating this snapshot.
