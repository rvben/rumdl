# General Regex Ignore Surfaces

rumdl does not provide a general Vale-style `BlockIgnores` or `TokenIgnores`
configuration surface for making arbitrary regular-expression matches invisible
to every rule.

## Why this is out of scope

rumdl's rules operate on Markdown structure, not on an independent stream of
prose tokens. Filtering diagnostics whose byte ranges overlap a regex match
would not make structural rules ignore that content: line-length checks would
still count it, spacing rules would still see the surrounding lines, and link or
heading rules would still see the parsed nodes. The configuration would promise
more isolation than it could deliver.

Treating matched text as truly opaque cannot be implemented centrally either.
Each affected rule would need to understand the synthetic gap in its own view of
the document, including how that gap interacts with Markdown parsing. That is
domain-specific parser and rule work disguised as one global regex option.

The general mechanism would also overlap three existing, explicit ways to scope
linting: inline disable directives, per-file ignores, and Markdown flavors.
When a concrete template or extension syntax is misinterpreted, rumdl instead
models that syntax directly and teaches the affected rules to treat it correctly.
Hugo shortcode tags are one example of that approach.

Specific template-syntax bugs remain in scope when they include a reproducer and
identify behavior that differs from how the target Markdown system renders the
document. This decision rejects only the universal regex-ignore abstraction.

## Prior requests

- [#798](https://github.com/rvben/rumdl/issues/798) — add `BlockIgnores` and
  `TokenIgnores` settings similar to Vale
