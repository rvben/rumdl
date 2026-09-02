const SNAPSHOT_DAYS = 28;

const EVENT_SCHEMA = Object.freeze({
  cta_select: {
    dimensions: [["open_quickstart", "compare_markdownlint", "open_playground", "install"], ["hero", "next"], [""]],
  },
  command_copy: {
    dimensions: [["uvx_check"], ["success", "failure"], [""]],
  },
  playground_ready: {
    dimensions: [["default", "shared"], [""], [""]],
  },
  playground_example: {
    dimensions: [["common", "headings", "links", "clean"], [""], [""]],
  },
  playground_fix: {
    dimensions: [["single", "all"], ["clean", "remaining", "unchanged"], [""]],
  },
  playground_config: {
    dimensions: [
      ["standard", "mkdocs", "mdx", "pandoc", "quarto", "obsidian", "kramdown", "azure_devops", "myst", "hugo", "mdg", "gh-aw"],
      ["0", "1", "2_4", "5_plus"],
      ["under_80", "80", "81_120", "over_120"],
    ],
  },
  playground_share: {
    dimensions: [["success", "failure", "too_large"], [""], [""]],
  },
  playground_error: {
    dimensions: [["load", "lint", "config", "share"], [""], [""]],
  },
});

const SIGNALS = Object.freeze([
  {
    key: "recorded_actions",
    label: "Recorded actions",
    note: "All privacy-preserving product actions",
    matches: () => true,
  },
  {
    key: "playground_starts",
    label: "Playground starts",
    note: "Playground sessions that loaded successfully",
    matches: row => row.event === "playground_ready",
  },
  {
    key: "playground_depth",
    label: "Playground depth",
    note: "Examples, fixes, configuration, and sharing actions",
    matches: row => ["playground_example", "playground_fix", "playground_config", "playground_share"].includes(row.event),
  },
  {
    key: "quickstart_opens",
    label: "Quickstart opens",
    note: "Selections of the Quickstart path",
    matches: row => row.event === "cta_select" && row.dimension1 === "open_quickstart",
  },
  {
    key: "playground_errors",
    label: "Playground errors",
    note: "Load, lint, configuration, or sharing failures",
    matches: row => row.event === "playground_error",
  },
]);

const JSON_HEADERS = {
  "cache-control": "no-store",
  "content-type": "application/json; charset=utf-8",
};

function json(body, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });
}

function sameSecret(left, right) {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return difference === 0;
}

function utcDate(value) {
  return new Date(`${value}T00:00:00.000Z`);
}

function dateString(value) {
  return value.toISOString().slice(0, 10);
}

function shiftDate(value, days) {
  const shifted = new Date(value);
  shifted.setUTCDate(shifted.getUTCDate() + days);
  return shifted;
}

function validIdentifier(value) {
  return typeof value === "string" && /^[A-Za-z_][A-Za-z0-9_]*$/.test(value);
}

function normalizeRows(data, from, to) {
  if (!Array.isArray(data)) throw new Error("Analytics Engine returned an invalid result");
  return data.flatMap(row => {
    const schema = EVENT_SCHEMA[row?.event];
    const dimensions = [row?.dimension1 ?? "", row?.dimension2 ?? "", row?.dimension3 ?? ""];
    const count = Number(row?.events);
    if (
      !schema
      || !/^\d{4}-\d{2}-\d{2}$/.test(row?.day ?? "")
      || row.day < from
      || row.day > to
      || !Number.isSafeInteger(count)
      || count < 0
      || !schema.dimensions.every((allowed, index) => allowed.includes(dimensions[index]))
    ) {
      return [];
    }
    return [{
      day: row.day,
      event: row.event,
      dimension1: dimensions[0],
      dimension2: dimensions[1],
      dimension3: dimensions[2],
      count,
    }];
  });
}

function buildSnapshot(rows, today) {
  const to = dateString(today);
  const from = dateString(shiftDate(today, -(SNAPSHOT_DAYS - 1)));
  const currentFrom = dateString(shiftDate(today, -6));
  const previousFrom = dateString(shiftDate(today, -13));
  const daily = new Map();
  for (let offset = SNAPSHOT_DAYS - 1; offset >= 0; offset -= 1) {
    daily.set(dateString(shiftDate(today, -offset)), 0);
  }
  for (const row of rows) daily.set(row.day, (daily.get(row.day) || 0) + row.count);
  const totalActions = [...daily.values()].reduce((sum, count) => sum + count, 0);
  const signals = SIGNALS.map(signal => ({
    key: signal.key,
    label: signal.label,
    current: rows
      .filter(row => row.day >= currentFrom && signal.matches(row))
      .reduce((sum, row) => sum + row.count, 0),
    previous: rows
      .filter(row => row.day >= previousFrom && row.day < currentFrom && signal.matches(row))
      .reduce((sum, row) => sum + row.count, 0),
    note: signal.note,
  }));
  return {
    schema_version: 1,
    generated_at: today.toISOString(),
    period: { from, to },
    total_actions: totalActions,
    active_days: [...daily.values()].filter(count => count > 0).length,
    daily: [...daily.entries()].map(([date, count]) => ({ date, count })),
    signals,
  };
}

function aggregateSql(dataset) {
  return `SELECT
  formatDateTime(toStartOfDay(timestamp), '%Y-%m-%d', 'Etc/UTC') AS day,
  index1 AS event,
  blob2 AS dimension1,
  blob3 AS dimension2,
  blob4 AS dimension3,
  sum(_sample_interval) AS events
FROM ${dataset}
WHERE timestamp >= toStartOfDay(now()) - INTERVAL '27' DAY
  AND timestamp < toStartOfDay(now()) + INTERVAL '1' DAY
GROUP BY day, event, dimension1, dimension2, dimension3
ORDER BY day ASC, event ASC`;
}

export async function onRequestGet(context) {
  const snapshotToken = String(context.env.ADOPTION_SNAPSHOT_TOKEN || "").trim();
  const accountId = String(context.env.CLOUDFLARE_ACCOUNT_ID || "").trim();
  const token = String(context.env.CLOUDFLARE_ANALYTICS_TOKEN || "").trim();
  const dataset = String(context.env.RUMDL_ANALYTICS_DATASET || "rumdl_web_events").trim();
  const authorization = context.request.headers.get("authorization") || "";
  if (!snapshotToken || !sameSecret(authorization, `Bearer ${snapshotToken}`)) {
    return json({ error: "Unauthorized" }, 401);
  }
  if (!/^[A-Fa-f0-9]{32}$/.test(accountId) || !token || !validIdentifier(dataset)) {
    return json({ error: "Adoption snapshot is not configured" }, 503);
  }

  const today = context.today ? utcDate(context.today) : new Date();
  const request = context.fetch || fetch;
  try {
    const response = await request(
      `https://api.cloudflare.com/client/v4/accounts/${accountId}/analytics_engine/sql`,
      {
        method: "POST",
        headers: {
          authorization: `Bearer ${token}`,
          "content-type": "text/plain; charset=utf-8",
        },
        body: aggregateSql(dataset),
      },
    );
    if (!response.ok) return json({ error: "Adoption snapshot is temporarily unavailable" }, 503);
    const payload = await response.json();
    const to = dateString(today);
    const from = dateString(shiftDate(today, -(SNAPSHOT_DAYS - 1)));
    return json(buildSnapshot(normalizeRows(payload.data, from, to), today));
  } catch {
    return json({ error: "Adoption snapshot is temporarily unavailable" }, 503);
  }
}
