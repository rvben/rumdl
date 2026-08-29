const EVENT_SCHEMA = Object.freeze({
  cta_select: {
    action: ["open_quickstart", "compare_markdownlint", "open_playground", "install"],
    location: ["hero", "next"],
  },
  command_copy: {
    command: ["uvx_check"],
    result: ["success", "failure"],
  },
  playground_ready: { source: ["default", "shared"] },
  playground_example: { example: ["common", "headings", "links", "clean"] },
  playground_fix: {
    scope: ["single", "all"],
    outcome: ["clean", "remaining", "unchanged"],
  },
  playground_config: {
    flavor: ["standard", "mkdocs", "mdx", "pandoc", "quarto", "obsidian", "kramdown", "azure_devops", "myst", "hugo", "mdg"],
    disabled: ["0", "1", "2_4", "5_plus"],
    line_length: ["under_80", "80", "81_120", "over_120"],
  },
  playground_share: { result: ["success", "failure", "too_large"] },
  playground_error: { stage: ["load", "lint", "config", "share"] },
});

const RESPONSE_HEADERS = {
  "cache-control": "no-store",
  "content-type": "text/plain; charset=utf-8",
};

function response(status = 204, analytics = "accepted") {
  return new Response(null, {
    status,
    headers: { ...RESPONSE_HEADERS, "x-rumdl-analytics": analytics },
  });
}

function validatePayload(payload) {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const schema = EVENT_SCHEMA[payload.event];
  if (!schema || !payload.properties || typeof payload.properties !== "object") return null;

  const properties = {};
  for (const [key, allowed] of Object.entries(schema)) {
    const value = String(payload.properties[key] ?? "");
    if (!allowed.includes(value)) return null;
    properties[key] = value;
  }
  return { event: payload.event, properties };
}

export async function onRequestPost(context) {
  const { request } = context;
  const url = new URL(request.url);
  const origin = request.headers.get("origin");
  if (origin && origin !== url.origin) return response(403, "rejected-origin");

  const contentType = request.headers.get("content-type") || "";
  if (!contentType.startsWith("application/json")) return response(415, "rejected-type");

  const declaredLength = Number(request.headers.get("content-length") || 0);
  if (declaredLength > 1024) return response(413, "rejected-size");

  let text;
  try {
    text = await request.text();
  } catch {
    return response(400, "rejected-body");
  }
  if (text.length > 1024) return response(413, "rejected-size");

  let event;
  try {
    event = validatePayload(JSON.parse(text));
  } catch {
    return response(400, "rejected-json");
  }
  if (!event) return response(422, "rejected-schema");

  const dataset = context.env.RUMDL_ANALYTICS;
  if (!dataset || typeof dataset.writeDataPoint !== "function") {
    return response(204, "binding-disabled");
  }

  const values = Object.values(event.properties);
  dataset.writeDataPoint({
    indexes: [event.event],
    blobs: [event.event, values[0] || "", values[1] || "", values[2] || ""],
    doubles: [1],
  });
  return response();
}
