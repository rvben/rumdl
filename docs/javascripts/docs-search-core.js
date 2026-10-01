// Search results are checked against the generated documentation index.
export function plainText(value) {
  return String(value ?? '').replace(/<[^>]*>/g, ' ').replace(/&(amp|lt|gt|quot|apos|nbsp);/g,
    (_, name) => ({ amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' })[name])
    .replace(/&#(x[0-9a-f]+|\d+);/gi, (_, number) => {
      const code = number[0].toLowerCase() === 'x' ? parseInt(number.slice(1), 16) : Number(number);
      return code <= 0x10ffff ? String.fromCodePoint(code) : '';
    }).replace(/\s+/g, ' ').trim();
}

export function markdownText(value) {
  const text = String(value ?? '')
    .replace(/<\/?(?:p|div|span|script|img|a|br|pre|code|strong|em|h[1-6])\b[^>]*>/gi, ' ')
    // CLI placeholders in Markdown are text, not HTML elements.
    .replace(/<([^<>\n]+)>/g, '&lt;$1&gt;')
    .replace(/\[(?:¶)?\]\([^)]*\)/g, '')
    .replace(/!?\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/^\s*#{1,6}\s+/gm, '')
    .replace(/^\s*(?:[-*+]\s+|---+\s*$)/gm, '')
    .replace(/(?:\*\*|__|`+)/g, '');
  return plainText(text);
}

export function documentLocation(value, origin, index) {
  if (typeof value !== 'string' || /[\u0000-\u001f]/.test(value)) return null;
  try {
    const url = new URL(value, origin + '/');
    if (url.origin !== origin || url.username || url.password) return null;
    const path = url.pathname.replace(/^\//, '');
    const page = index.items.find(item => item.location.split('#')[0] === path);
    if (!page) return null;
    const exact = index.items.find(item => item.location === path + url.hash);
    return { location: exact ? path + url.hash : path, page, section: exact ?? page };
  } catch { return null; }
}

export function normalizeCloudflare(payload, origin, index, { deduplicate = true } = {}) {
  if (payload?.success === false || payload?.error) throw new Error('AI Search could not complete this search.');
  const chunks = payload?.result?.chunks ?? payload?.chunks;
  if (!Array.isArray(chunks)) throw new Error('AI Search returned an unexpected response.');
  const results = [], seen = new Set();
  for (const chunk of chunks) {
    const metadata = chunk.item?.metadata ?? {};
    const key = metadata.url ?? chunk.item?.key;
    const anchor = String(chunk.text ?? '').match(/^#{1,6}\s[^\n]*?\[¶\]\((#[^\s)]+)/)?.[1];
    const document = documentLocation(anchor && typeof key === 'string' ? key.split('#')[0] + anchor : key, origin, index);
    if (!document || (deduplicate && seen.has(document.location))) continue;
    seen.add(document.location);
    results.push({ location: document.location,
      title: plainText(document.section.title),
      path: (document.section.path ?? [document.section.title]).map(plainText),
      text: markdownText(chunk.text), score: chunk.score });
  }
  return results.slice(0, 10);
}

export function normalizeNative(payload, index) {
  return (payload.items ?? []).flatMap(item => {
    const section = index.items[item.id];
    if (!section) return [];
    return [{ location: section.location, title: plainText(section.title),
      path: (section.path ?? [section.title]).map(plainText), text: plainText(section.text) }];
  }).slice(0, 10);
}

export class NativeSearch {
  constructor(workerURL, indexURL) {
    this.workerURL = workerURL;
    this.indexURL = indexURL;
    this.queue = Promise.resolve();
  }
  async initialize() {
    if (!this.initializing) {
      this.initializing = (async () => {
        const response = await fetch(this.indexURL);
        if (!response.ok) throw new Error('The documentation index could not be loaded.');
        this.index = await response.json();
        this.worker = new Worker(this.workerURL);
        await this.exchange({ type: 0, data: this.index }, 1);
      })().catch(error => {
        this.worker?.terminate();
        this.initializing = null;
        throw error;
      });
    }
    return this.initializing;
  }
  exchange(message, responseType) {
    return new Promise((resolve, reject) => {
      const worker = this.worker;
      const cleanup = () => {
        clearTimeout(timer);
        worker.removeEventListener('message', receive);
        worker.removeEventListener('error', fail);
      };
      const receive = event => {
        if (event.data.type === responseType) { cleanup(); resolve(event.data.data); }
      };
      const fail = () => {
        cleanup(); worker.terminate(); this.initializing = null;
        reject(new Error('Keyword search is unavailable. Please try again.'));
      };
      const timer = setTimeout(fail, 10_000);
      worker.addEventListener('message', receive);
      worker.addEventListener('error', fail);
      worker.postMessage(message);
    });
  }
  search(query) {
    const task = this.queue.then(async () => {
      await this.initialize();
      const payload = await this.exchange({ type: 2, data: { input: query } }, 3);
      return normalizeNative(payload, this.index);
    });
    this.queue = task.catch(() => {});
    return task;
  }
}

export async function cloudflareSearch(query, origin, index, signal, options) {
  const response = await fetch('/api/docs-search', { method: 'POST',
    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ query }), signal });
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error ?? 'AI Search is unavailable. Try keyword search.');
  return normalizeCloudflare(payload, origin, index, options);
}
