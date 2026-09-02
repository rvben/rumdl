---
on: workflow_dispatch
permissions:
  contents: read
---

{{#runtime-import shared/experiment-guidance.md}}

# Compare experiment variants

{{#if experiments.output_format == "brief"}}
Produce a brief report.
{{#elseif experiments.output_format == "detailed"}}
Produce a detailed report.
{{#else}}
Produce the default report.
{{#endif}}
