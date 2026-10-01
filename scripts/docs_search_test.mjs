import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const root = new URL('../', import.meta.url);
const load = async path => import(`data:text/javascript;base64,${Buffer.from(await readFile(new URL(path, root))).toString('base64')}`);
const { onRequestPost } = await load('functions/api/docs-search.js');
const { normalizeCloudflare, markdownText } = await load('docs/javascripts/docs-search-core.js');
const site = (await readFile(new URL('zensical.toml', root), 'utf8')).includes('https://rumdl.dev/') ? 'rumdl' : 'shinyhub';
const origin = `https://${site}.dev`;
const query = 'example question <private-placeholder>';
const points = [];
let providerCalls = 0;
let provider;
function context(body, headers = {}, hostname = `${site}.dev`) {
  return { request: new Request(`https://${hostname}/api/docs-search`, { method: 'POST',
    headers: { origin: `https://${hostname}`, 'content-type': 'application/json', ...headers },
    body: typeof body === 'string' ? body : JSON.stringify(body) }),
    env: { DOCS_SEARCH_ANALYTICS: { writeDataPoint(point) { points.push(point); } } },
    fetch: async (url, options) => {
      providerCalls++;
      assert.match(url, /^https:\/\/[a-f0-9-]+\.search\.ai\.cloudflare\.com\/search$/);
      assert.equal(options.headers['user-agent'], 'docs-search/1.0');
      assert.equal(options.headers.authorization, undefined);
      assert.equal(options.redirect, 'manual');
      assert.deepEqual(JSON.parse(options.body), { messages: [{ role: 'user', content: query }] });
      return provider();
    } };
}
provider = () => new Response(JSON.stringify({ chunks: [{ text: 'Example', item: { key: `${origin}/guide/` } }] }));
let response = await onRequestPost(context({ query }));
assert.equal(response.status, 200);
assert.equal(response.headers.get('x-docs-search-analytics'), 'enabled');
assert.equal(response.headers.get('cache-control'), 'no-store');
assert.deepEqual(points.map(point => point.blobs), [['attempt',''],['complete','success']]);
assert.equal(points[1].doubles[2], 1);
assert.equal(JSON.stringify(points).includes(query), false);
assert.equal(JSON.stringify(points).includes(origin), false);

for (const [body, headers, status] of [
  [{ query }, { origin: 'https://other.invalid' }, 403],
  [{ query }, { origin: '' }, 403],
  [{ query }, { 'content-type': 'text/plain' }, 415],
  [{ query }, { 'content-length': '5000' }, 413],
  [' '.repeat(5000), {}, 413],
  ['{', {}, 400], [[], {}, 422],
  [{ query: '' }, {}, 422], [{ query: 'q'.repeat(501) }, {}, 422],
  [{ query, user: 'private' }, {}, 422],
  [{ event: 'click', rank: 11, method: 'ai' }, {}, 422],
  [{ event: 'open', query }, {}, 422],
  [{ event: 'constructor' }, {}, 422],
  [{ event: 'keyword', milliseconds: -1, matches: 1 }, {}, 422],
]) assert.equal((await onRequestPost(context(body, headers))).status, status);
assert.equal(providerCalls, 1, 'rejected requests never reach the provider');

for (const body of [{ event: 'open' }, { event: 'click', rank: 2, method: 'ai' },
  { event: 'keyword', milliseconds: 6, matches: 4 }]) {
  assert.equal((await onRequestPost(context(body))).status, 204);
}
assert.equal(providerCalls, 1, 'measurement does not dispatch paid queries');
const before = points.length;
response = await onRequestPost(context({ query }, {}, 'preview.pages.dev'));
assert.equal(response.status, 200);
assert.equal(points.length, before, 'preview traffic is not production usage');

provider = () => new Response('{}', { status: 429 });
assert.equal((await onRequestPost(context({ query }))).status, 429);
assert.equal(points.at(-1).blobs[1], 'limited');
provider = () => new Response(JSON.stringify({ error: query }));
response = await onRequestPost(context({ query }));
assert.equal(response.status, 502);
assert.equal((await response.text()).includes(query), false, 'upstream errors are not echoed');
provider = () => { throw new DOMException('Timeout', 'TimeoutError'); };
assert.equal((await onRequestPost(context({ query }))).status, 504);
provider = () => new Response('x'.repeat(2 * 1024 * 1024 + 1));
assert.equal((await onRequestPost(context({ query }))).status, 502);

const index = { items: [{ location: 'guide/', title: 'Guide', path: ['Guide'] },
  { location: 'guide/#setting', title: 'Setting', path: ['Guide','Setting'] }] };
const results = normalizeCloudflare({ chunks: [
  { text: '## Setting [¶](#setting)\nUse `--name <slug>`.', item: { key: `${origin}/guide/` } },
  { text: 'Bad', item: { key: 'https://other.invalid/guide/' } },
  { text: 'Bad', item: { key: `${origin}/missing/` } },
] }, origin, index);
assert.equal(results.length, 1);
assert.equal(results[0].location, 'guide/#setting');
assert.match(results[0].text, /--name <slug>/);
assert.equal(markdownText('**Text** with [link](https://example.com)'), 'Text with link');
console.log(`${site}: documentation search contract, privacy, provider errors and result links passed`);
