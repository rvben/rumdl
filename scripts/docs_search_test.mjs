import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const root = new URL('../', import.meta.url);
const load = async path => import(`data:text/javascript;base64,${Buffer.from(await readFile(new URL(path, root))).toString('base64')}`);
const { onRequest } = await load('functions/api/docs-search.js');
const { normalizeCloudflare, markdownText } = await load('docs/javascripts/docs-search-core.js');
const site = (await readFile(new URL('zensical.toml', root), 'utf8')).includes('https://rumdl.dev/') ? 'rumdl' : 'shinyhub';
const origin = `https://${site}.dev`;
const fail = () => { assert.fail('retired search must not contact a provider or record analytics'); };
for (const method of ['GET', 'POST', 'OPTIONS']) {
  const response = await onRequest({
    request: new Request(`${origin}/api/docs-search`, { method,
      ...(method === 'POST' ? { body: JSON.stringify({ query: 'MD013' }) } : {}) }),
    env: { DOCS_SEARCH_ANALYTICS: { writeDataPoint: fail } },
    fetch: fail,
  });
  assert.equal(response.status, 410);
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.match((await response.json()).error, /keyword search/);
}

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
console.log(`${site}: retired search blocks provider calls; retained result helpers passed`);
