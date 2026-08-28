# rumdl.dev Pages Functions

## Adoption events

`api/events.js` accepts a small, server-side allowlist of product actions. Every
required category must match the fixed contract in both the browser and the
edge function; incomplete events are discarded before transmission and
rejected at the endpoint. It never accepts Markdown, URLs, referrers, user
agents, account identifiers, or free-form event properties. Requests are
same-origin, capped at 1 KiB, and return `204` without setting cookies.

To record events in production, add a Cloudflare Pages Analytics Engine binding
to the `rumdl-dev` project:

- Variable name: `RUMDL_ANALYTICS`
- Dataset: `rumdl_web_events`

The endpoint intentionally remains a no-op when that binding is absent, so
preview deployments and local builds do not fail or send analytics.

Run `make docs-analytics` to verify the browser-to-edge contract for repository
trials, command copies, playground starts, examples, fixes, configuration,
sharing, and error stages as well as schema rejection and the privacy boundary.
