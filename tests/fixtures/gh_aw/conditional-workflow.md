---
on:
  schedule:
    - cron: "0 8 * * 1"
permissions:
  contents: read
---

{{#if github.event.repository.archived == false}}
{{#runtime-import ./weekly-guidance.md}}

# Review repository maintenance

Summarize stale maintenance work without changing repository contents.

{{/if}}
