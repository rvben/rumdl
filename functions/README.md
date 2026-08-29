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

Run `make docs-analytics` to verify the browser-to-edge contract for Quickstart
opens, command copies, playground starts, examples, fixes, configuration,
sharing, and error stages as well as schema rejection and the privacy boundary.

## Private adoption snapshot

`api/adoption.js` turns the product-owned event contract into a small aggregate
snapshot for an authorized private consumer. It returns exactly 28 contiguous
UTC daily totals and five labeled current-versus-previous signals. Raw events,
product dimensions, credentials, request bodies, and analytics-provider errors
are never returned. Every response uses `Cache-Control: no-store`.

Configure these runtime values in the Cloudflare Pages project:

- `ADOPTION_SNAPSHOT_TOKEN`: a strong shared bearer secret;
- `CLOUDFLARE_ACCOUNT_ID`: the account that owns the Analytics Engine dataset;
- `CLOUDFLARE_ANALYTICS_TOKEN`: an Account Analytics Read token; and
- `RUMDL_ANALYTICS_DATASET`: optional dataset name, defaulting to
  `rumdl_web_events`.

The bearer secret and Analytics Read token are secrets and do not belong in
`wrangler.toml` or committed environment files. The snapshot route rejects
unauthorized requests before contacting Analytics Engine and fails closed when
configuration or upstream data is invalid.
