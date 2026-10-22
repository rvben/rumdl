// Retire the proxy too: already-open pages must not dispatch provider queries.
export function onRequest() {
  return new Response(JSON.stringify({ error: 'AI Search has been retired. Use the documentation keyword search.' }), {
    status: 410,
    headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' },
  });
}
