#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile(new URL("../functions/api/events.js", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const { onRequestPost } = await import(moduleUrl);

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
