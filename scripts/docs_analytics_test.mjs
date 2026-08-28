#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const source = await readFile(new URL("../functions/api/events.js", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const { onRequestPost } = await import(moduleUrl);
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
  window.rumdlAnalytics.track("cta_select", { action: "repository_trial" }),
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

const productActions = [
  ["repository trial", "cta_select", { action: "repository_trial", location: "hero" }, ["repository_trial", "hero", ""]],
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

assert.match(homepageSource, /data-rm-event="cta_select"[^>]+data-rm-action="repository_trial"[^>]+data-rm-location="hero"/);
assert.match(homepageSource, /data-rm-copy="uvx rumdl check \."[^>]+data-rm-command="uvx_check"/);
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
  request: request({ event: "cta_select", properties: { action: "repository_trial" } }),
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
