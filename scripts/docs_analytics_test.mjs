#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const source = await readFile(new URL("../functions/api/events.js", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const { onRequestPost } = await import(moduleUrl);
const adoptionSource = await readFile(new URL("../functions/api/adoption.js", import.meta.url), "utf8");
const adoptionModuleUrl = `data:text/javascript;base64,${Buffer.from(adoptionSource).toString("base64")}`;
const { onRequestGet: getAdoptionSnapshot } = await import(adoptionModuleUrl);
const clientSource = await readFile(new URL("../docs/javascripts/rumdl.js", import.meta.url), "utf8");
const homepageSource = await readFile(new URL("../docs/index.md", import.meta.url), "utf8");
const playgroundSource = await readFile(new URL("../docs/playground.md", import.meta.url), "utf8");

const emitted = [];
const document = { readyState: "loading", addEventListener() {} };
class TestCustomEvent {
  constructor(type, init) {
    this.type = type;
    this.detail = init.detail;
  }
}
const window = {
  document,
  location: { hostname: "localhost" },
  dispatchEvent(event) { emitted.push(event.detail); },
};
vm.runInContext(clientSource, vm.createContext({
  Blob,
  CustomEvent: TestCustomEvent,
  document,
  fetch,
  navigator: {},
  window,
}));

assert.equal(
  window.rumdlAnalytics.track("cta_select", { action: "open_quickstart" }),
  false,
  "events with a missing required category must not leave the browser",
);
assert.deepEqual(emitted, []);

function request(body, options = {}) {
  return new Request("https://rumdl.dev/api/events", {
    method: "POST",
    headers: {
      "content-type": options.contentType || "application/json",
      origin: options.origin || "https://rumdl.dev",
    },
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
}

const writes = [];
const dataset = { writeDataPoint: (point) => writes.push(point) };

const adoptionResponse = await getAdoptionSnapshot({
  env: {
    ADOPTION_SNAPSHOT_TOKEN: "snapshot-secret",
    CLOUDFLARE_ACCOUNT_ID: "0123456789abcdef0123456789abcdef",
    CLOUDFLARE_ANALYTICS_TOKEN: "test-token",
    RUMDL_ANALYTICS_DATASET: "rumdl_web_events",
  },
  request: new Request("https://rumdl.dev/api/adoption", {
    headers: { authorization: "Bearer snapshot-secret" },
  }),
  fetch: async () => new Response(JSON.stringify({
    data: [
      { day: "2026-08-28", event: "playground_ready", dimension1: "default", dimension2: "", dimension3: "", events: 3 },
    ],
  }), { status: 200, headers: { "content-type": "application/json" } }),
  today: "2026-08-28",
});
assert.equal(adoptionResponse.status, 200);
assert.equal(adoptionResponse.headers.get("cache-control"), "no-store");
assert.deepEqual(await adoptionResponse.json(), {
  schema_version: 1,
  generated_at: "2026-08-28T00:00:00.000Z",
  period: { from: "2026-08-01", to: "2026-08-28" },
  total_actions: 3,
  active_days: 1,
  daily: Array.from({ length: 28 }, (_, index) => ({
    date: `2026-08-${String(index + 1).padStart(2, "0")}`,
    count: index === 27 ? 3 : 0,
  })),
  signals: [
    { key: "recorded_actions", label: "Recorded actions", current: 3, previous: 0, note: "All privacy-preserving product actions" },
    { key: "playground_starts", label: "Playground starts", current: 3, previous: 0, note: "Playground sessions that loaded successfully" },
    { key: "playground_depth", label: "Playground depth", current: 0, previous: 0, note: "Examples, fixes, configuration, and sharing actions" },
    { key: "quickstart_opens", label: "Quickstart opens", current: 0, previous: 0, note: "Selections of the Quickstart path" },
    { key: "playground_errors", label: "Playground errors", current: 0, previous: 0, note: "Load, lint, configuration, or sharing failures" },
  ],
});

const privateAdoptionResponse = await getAdoptionSnapshot({
  env: {
    ADOPTION_SNAPSHOT_TOKEN: "snapshot-secret",
    CLOUDFLARE_ACCOUNT_ID: "0123456789abcdef0123456789abcdef",
    CLOUDFLARE_ANALYTICS_TOKEN: "test-token",
  },
  request: new Request("https://rumdl.dev/api/adoption"),
  fetch: async () => {
    throw new Error("unauthorized requests must not reach Analytics Engine");
  },
  today: "2026-08-28",
});
assert.equal(privateAdoptionResponse.status, 401);
assert.equal(privateAdoptionResponse.headers.get("cache-control"), "no-store");

let unauthorizedFetches = 0;
const unconfiguredAdoptionResponse = await getAdoptionSnapshot({
  env: {},
  request: new Request("https://rumdl.dev/api/adoption"),
  fetch: async () => { unauthorizedFetches += 1; },
  today: "2026-08-28",
});
assert.equal(unconfiguredAdoptionResponse.status, 401);
assert.equal(unauthorizedFetches, 0, "unauthorized requests must not reveal configuration or query Analytics Engine");

const incompleteAdoptionResponse = await getAdoptionSnapshot({
  env: { ADOPTION_SNAPSHOT_TOKEN: "snapshot-secret" },
  request: new Request("https://rumdl.dev/api/adoption", {
    headers: { authorization: "Bearer snapshot-secret" },
  }),
  fetch: async () => { throw new Error("incomplete configuration must fail before fetch"); },
  today: "2026-08-28",
});
assert.equal(incompleteAdoptionResponse.status, 503);
assert.deepEqual(await incompleteAdoptionResponse.json(), { error: "Adoption snapshot is not configured" });

const malformedAdoptionResponse = await getAdoptionSnapshot({
  env: {
    ADOPTION_SNAPSHOT_TOKEN: "snapshot-secret",
    CLOUDFLARE_ACCOUNT_ID: "0123456789abcdef0123456789abcdef",
    CLOUDFLARE_ANALYTICS_TOKEN: "test-token",
  },
  request: new Request("https://rumdl.dev/api/adoption", {
    headers: { authorization: "Bearer snapshot-secret" },
  }),
  fetch: async () => new Response(JSON.stringify({
    data: [
      { day: "2026-08-28", event: "private_event", dimension1: "secret", events: 1 },
      { day: "2026-08-28", event: "playground_ready", dimension1: "default", dimension2: "", dimension3: "", events: 2 },
    ],
  }), { status: 200 }),
  today: "2026-08-28",
});
assert.equal(malformedAdoptionResponse.status, 200);
const filteredAdoptionSnapshot = await malformedAdoptionResponse.json();
assert.equal(filteredAdoptionSnapshot.total_actions, 2);
assert.equal(filteredAdoptionSnapshot.active_days, 1);

const upstreamAdoptionResponse = await getAdoptionSnapshot({
  env: {
    ADOPTION_SNAPSHOT_TOKEN: "snapshot-secret",
    CLOUDFLARE_ACCOUNT_ID: "0123456789abcdef0123456789abcdef",
    CLOUDFLARE_ANALYTICS_TOKEN: "test-token",
  },
  request: new Request("https://rumdl.dev/api/adoption", {
    headers: { authorization: "Bearer snapshot-secret" },
  }),
  fetch: async () => new Response("provider secret detail", { status: 403 }),
  today: "2026-08-28",
});
assert.equal(upstreamAdoptionResponse.status, 503);
const upstreamBody = await upstreamAdoptionResponse.text();
assert.equal(upstreamBody.includes("provider secret detail"), false);
assert.equal(upstreamAdoptionResponse.headers.get("cache-control"), "no-store");

const productActions = [
  ["Quickstart open", "cta_select", { action: "open_quickstart", location: "hero" }, ["open_quickstart", "hero", ""]],
  ["command copy", "command_copy", { command: "uvx_check", result: "success" }, ["uvx_check", "success", ""]],
  ["playground start", "playground_ready", { source: "default" }, ["default", "", ""]],
  ["playground example", "playground_example", { example: "common" }, ["common", "", ""]],
  ["playground fix", "playground_fix", { scope: "single", outcome: "clean" }, ["single", "clean", ""]],
  ["playground configuration", "playground_config", { flavor: "standard", disabled: "0", line_length: "under_80" }, ["standard", "0", "under_80"]],
  ["playground share", "playground_share", { result: "success" }, ["success", "", ""]],
  ["playground error", "playground_error", { stage: "lint" }, ["lint", "", ""]],
];

for (const [label, event, properties, dimensions] of productActions) {
  assert.equal(window.rumdlAnalytics.track(event, properties), true, `${label} should pass the browser contract`);
  const payload = emitted.pop();
  const contractWrites = [];
  const result = await onRequestPost({
    request: request(payload),
    env: { RUMDL_ANALYTICS: { writeDataPoint: (point) => contractWrites.push(point) } },
  });
  assert.equal(result.status, 204, `${label} should pass the edge contract`);
  assert.deepEqual(contractWrites, [{
    indexes: [event],
    blobs: [event, ...dimensions],
    doubles: [1],
  }], `${label} should preserve only its fixed aggregate dimensions`);
}

assert.match(homepageSource, /hide:\s+[\s\S]*?- navigation/);
assert.match(homepageSource, /class="rm-home-nav"[\s\S]*?>Documentation<[\s\S]*?>Playground<[\s\S]*?>Installation</);
assert.match(homepageSource, /class="rm-terminal-shot"[\s\S]*?src="images\/homepage-terminal\.png"/);
assert.doesNotMatch(homepageSource, /Captured from an actual/);
assert.doesNotMatch(homepageSource, /class="rm-proof"|class="rm-terminal"/);
assert.match(homepageSource, /class="rm-install rm-install--primary"[\s\S]*?data-rm-copy="uvx rumdl check \."[^>]+data-rm-command="uvx_check"/);
assert.match(homepageSource, /data-rm-event="cta_select"[^>]+data-rm-action="open_quickstart"[^>]+data-rm-location="hero"[^>]*>Quickstart</);
assert.doesNotMatch(homepageSource, /trial|60-second/i);
assert.match(homepageSource, /class="rm-hero__alternatives"[\s\S]*?data-rm-action="open_playground"[\s\S]*?data-rm-action="install"/);
assert.match(homepageSource, /class="rm-next__primary"[\s\S]*?data-rm-copy="uvx rumdl check \."/);
assert.match(clientSource, /const idleLabel = button\.textContent;/);
assert.doesNotMatch(clientSource, /button\.textContent = "Copy"/);
assert.match(playgroundSource, /track\('playground_ready', \{ source: sharedState \? 'shared' : 'default' \}\)/);
assert.match(playgroundSource, /track\('playground_example', \{ example: key \}\)/);
assert.match(playgroundSource, /track\('playground_fix', \{ scope: 'single', outcome \}\)/);
assert.match(playgroundSource, /track\('playground_config', \{\s+flavor: activeConfig\.flavor,\s+disabled: disabledCountBucket\(activeConfig\.disable\.length\),\s+line_length: lineLengthBucket\(activeConfig\.lineLength\),/);
assert.match(playgroundSource, /track\('playground_share', \{ result: 'success' \}\)/);
assert.match(playgroundSource, /track\('playground_error', \{ stage: 'lint' \}\)/);

const accepted = await onRequestPost({
  request: request({
    event: "playground_fix",
    properties: { scope: "single", outcome: "clean", markdown: "private text" },
  }),
  env: { RUMDL_ANALYTICS: dataset },
});
assert.equal(accepted.status, 204);
assert.equal(accepted.headers.get("x-rumdl-analytics"), "accepted");
assert.deepEqual(writes, [{
  indexes: ["playground_fix"],
  blobs: ["playground_fix", "single", "clean", ""],
  doubles: [1],
}]);
assert.equal(JSON.stringify(writes).includes("private text"), false);

const disabled = await onRequestPost({
  request: request({ event: "playground_ready", properties: { source: "default" } }),
  env: {},
});
assert.equal(disabled.status, 204);
assert.equal(disabled.headers.get("x-rumdl-analytics"), "binding-disabled");

const rejectedEvent = await onRequestPost({
  request: request({ event: "markdown_content", properties: { value: "private" } }),
  env: { RUMDL_ANALYTICS: dataset },
});
assert.equal(rejectedEvent.status, 422);

const rejectedIncompleteEvent = await onRequestPost({
  request: request({ event: "cta_select", properties: { action: "open_quickstart" } }),
  env: { RUMDL_ANALYTICS: dataset },
});
assert.equal(rejectedIncompleteEvent.status, 422);

const rejectedOrigin = await onRequestPost({
  request: request({ event: "playground_ready", properties: { source: "default" } }, { origin: "https://example.com" }),
  env: { RUMDL_ANALYTICS: dataset },
});
assert.equal(rejectedOrigin.status, 403);

const rejectedSize = await onRequestPost({
  request: request(`{"event":"playground_ready","properties":{"source":"default"},"padding":"${"x".repeat(1100)}"}`),
  env: { RUMDL_ANALYTICS: dataset },
});
assert.equal(rejectedSize.status, 413);

console.log("docs analytics test passed");
