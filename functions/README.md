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

## Weekly report

`.github/workflows/adoption-report.yml` queries aggregate weighted counts every
Monday and publishes a private GitHub Actions summary plus a 30-day HTML
artifact. It requires a separate `CLOUDFLARE_ANALYTICS_TOKEN` repository secret
with only `Account Analytics: Read` permission. The report never writes data
back to Cloudflare or commits generated analytics to the repository.

Run `make docs-analytics-report` to render the reporting surface with synthetic
fixture data and no network access.
