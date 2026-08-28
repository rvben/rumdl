# rumdl.dev Pages Functions

## Adoption events

`api/events.js` accepts a small, server-side allowlist of product actions. It
never accepts Markdown, URLs, referrers, user agents, account identifiers, or
free-form event properties. Requests are same-origin, capped at 1 KiB, and
return `204` without setting cookies.

To record events in production, add a Cloudflare Pages Analytics Engine binding
to the `rumdl-dev` project:

- Variable name: `RUMDL_ANALYTICS`
- Dataset: `rumdl_web_events`

The endpoint intentionally remains a no-op when that binding is absent, so
preview deployments and local builds do not fail or send analytics.

Run `make docs-analytics` to verify schema rejection and the privacy boundary.
