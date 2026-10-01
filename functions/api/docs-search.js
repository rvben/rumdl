const SITE = "rumdl";
const ENDPOINT = "https://edb2912a-11a7-4c16-8017-7a013ba2948f.search.ai.cloudflare.com/search";
const MAX_BODY_BYTES = 4096;
const HEADERS = { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' };

function json(value, status = 200, analytics = false) {
  return new Response(JSON.stringify(value), { status,
    headers: { ...HEADERS, 'x-docs-search-analytics': analytics ? 'enabled' : 'disabled' } });
}

async function boundedText(body, limit) {
  if (!body) return '';
  const reader = body.getReader();
  const parts = [];
  let bytes = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > limit) { await reader.cancel(); throw new Error('size'); }
      parts.push(value);
    }
  } finally { reader.releaseLock(); }
  const result = new Uint8Array(bytes);
  let offset = 0;
  for (const part of parts) { result.set(part, offset); offset += part.length; }
  return new TextDecoder().decode(result);
}

function analytics(context) {
  const dataset = context.env.DOCS_SEARCH_ANALYTICS;
  const enabled = new URL(context.request.url).hostname === `${SITE}.dev`
    && typeof dataset?.writeDataPoint === 'function';
  return { enabled, record(event, outcome = '', milliseconds = 0, matches = 0, rank = 0) {
    if (!enabled) return;
    try {
      // No query, URL, IP, referrer, user agent, cookie, or visitor identifier.
      dataset.writeDataPoint({ indexes: [SITE], blobs: [event, outcome],
        doubles: [1, milliseconds, matches, rank] });
    } catch { /* Measurement must not break documentation search. */ }
  } };
}

function validNumber(value, maximum) {
  return Number.isInteger(value) && value >= 0 && value <= maximum;
}

export async function onRequestPost(context) {
  const { request } = context;
  const origin = new URL(request.url).origin;
  if (request.headers.get('origin') !== origin) return json({ error: 'Invalid request origin.' }, 403);
  if (!(request.headers.get('content-type') || '').startsWith('application/json')) {
    return json({ error: 'Expected JSON.' }, 415);
  }
  if (Number(request.headers.get('content-length') || 0) > MAX_BODY_BYTES) {
    return json({ error: 'Request is too large.' }, 413);
  }
  let payload;
  try { payload = JSON.parse(await boundedText(request.body, MAX_BODY_BYTES)); }
  catch (error) { return json({ error: 'Invalid request.' }, error.message === 'size' ? 413 : 400); }
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return json({ error: 'Invalid request.' }, 422);
  const meter = analytics(context);

  if ('event' in payload) {
    const schemas = {
      open: { keys: ['event'], valid: () => true },
      click: { keys: ['event', 'rank', 'method'], valid: () => validNumber(payload.rank, 10)
        && payload.rank > 0 && ['ai', 'keyword'].includes(payload.method) },
      keyword: { keys: ['event', 'milliseconds', 'matches'], valid: () =>
        validNumber(payload.milliseconds, 60000) && validNumber(payload.matches, 10) },
    };
    const schema = Object.hasOwn(schemas, payload.event) ? schemas[payload.event] : null;
    if (!schema || Object.keys(payload).some(key => !schema.keys.includes(key)) || !schema.valid()) {
      return json({ error: 'Invalid event.' }, 422);
    }
    meter.record(payload.event, payload.method || '', payload.milliseconds || 0,
      payload.matches || 0, payload.rank || 0);
    return new Response(null, { status: 204, headers: { 'cache-control': 'no-store',
      'x-docs-search-analytics': meter.enabled ? 'enabled' : 'disabled' } });
  }

  if (Object.keys(payload).length !== 1 || typeof payload.query !== 'string'
    || !payload.query.trim() || payload.query.length > 500) return json({ error: 'Enter a question of up to 500 characters.' }, 422);
  const started = Date.now();
  meter.record('attempt');
  try {
    const response = await (context.fetch || fetch)(ENDPOINT, {
      method: 'POST', headers: { 'content-type': 'application/json', 'user-agent': 'docs-search/1.0' },
      body: JSON.stringify({ messages: [{ role: 'user', content: payload.query.trim() }] }),
      signal: AbortSignal.timeout(12000), redirect: 'manual',
    });
    if (!response.ok) {
      meter.record('complete', response.status === 429 ? 'limited' : 'error', Date.now() - started);
      return json({ error: response.status === 429
        ? 'AI Search is busy. Try keyword search or search again shortly.'
        : 'AI Search is unavailable. Try keyword search.' }, response.status === 429 ? 429 : 502, meter.enabled);
    }
    const result = JSON.parse(await boundedText(response.body, 2 * 1024 * 1024));
    const chunks = result?.result?.chunks ?? result?.chunks;
    if (result?.success === false || result?.error || !Array.isArray(chunks)) throw new Error('provider');
    meter.record('complete', chunks.length ? 'success' : 'empty', Date.now() - started, Math.min(chunks.length, 10));
    return json(result, 200, meter.enabled);
  } catch (error) {
    const timeout = error.name === 'TimeoutError' || error.name === 'AbortError';
    meter.record('complete', timeout ? 'timeout' : 'error', Date.now() - started);
    return json({ error: timeout ? 'AI Search took too long. Try keyword search.'
      : 'AI Search is unavailable. Try keyword search.' }, timeout ? 504 : 502, meter.enabled);
  }
}
