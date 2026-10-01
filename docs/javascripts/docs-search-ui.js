import { NativeSearch, cloudflareSearch } from './docs-search-core.js';

const site = {"site": "rumdl", "title": "rumdl", "origin": "https://rumdl.dev", "connected": true};
const config = JSON.parse(document.querySelector('#__config').textContent);
const native = new NativeSearch(new URL(config.search, location.href),
  new URL(config.base + '/search.json', location.href));
const examples = site.site === 'shinyhub'
  ? ['Keep an idle app running', 'Sign in with our existing identity provider', 'Deploy several apps together']
  : ['MD013', 'Ignore long lines in code blocks', 'Use my markdownlint configuration'];

function node(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}

const button = node('button', 'ds-trigger', 'Ask docs');
button.type = 'button';
button.setAttribute('aria-haspopup', 'dialog');
const header = document.querySelector('.md-header__inner');
header.append(button);
const dialog = node('dialog', 'ds-dialog');
dialog.setAttribute('aria-labelledby', 'ds-title');
const top = node('div', 'ds-top');
const title = node('h2', '', `Search ${site.title} documentation`);
title.id = 'ds-title';
const close = node('button', 'ds-close', 'Close');
close.type = 'button';
top.append(title, close);
const introduction = node('p', 'ds-intro', 'Find a guide, a setting, or an answer in the documentation.');
const form = node('form', 'ds-form');
form.setAttribute('role', 'search');
const label = node('label', 'ds-label', 'What would you like to find?');
label.htmlFor = 'ds-query';
const inputRow = node('div', 'ds-input-row');
const input = node('input', 'ds-input');
input.id = 'ds-query'; input.type = 'search'; input.maxLength = 500;
input.autocomplete = 'off'; input.placeholder = 'Search by topic or ask a question…';
const submit = node('button', 'ds-submit', 'Search');
submit.type = 'submit';
inputRow.append(input, submit); form.append(label, inputRow);
const modes = node('fieldset', 'ds-modes');
modes.append(node('legend', 'ds-label', 'Search method'));
for (const [value, text] of [['keyword', 'Keyword'], ['ai', 'AI Search']]) {
  const label = node('label', 'ds-mode');
  const radio = node('input'); radio.type = 'radio'; radio.name = 'ds-method'; radio.value = value;
  radio.checked = value === 'ai'; radio.disabled = value === 'ai' && !site.connected;
  radio.addEventListener('change', () => { if (input.value.trim()) run(); });
  label.append(radio, node('span', '', text)); modes.append(label);
}
const connection = node('p', 'ds-connection', site.connected
  ? 'AI Search finds documentation for your question. Keyword search works well for exact names.'
  : 'Local preview: keyword search works. AI Search needs an index before relevance can be compared.');
connection.id = 'ds-connection'; modes.setAttribute('aria-describedby', connection.id);
const suggestions = node('div', 'ds-suggestions');
suggestions.setAttribute('aria-label', 'Example searches');
for (const query of examples) {
  const example = node('button', 'ds-example', query); example.type = 'button';
  example.addEventListener('click', () => { input.value = query; run(); input.focus(); });
  suggestions.append(example);
}
const status = node('p', 'ds-status', 'Try a topic, rule name, or question.');
status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
const results = node('ol', 'ds-results'); results.setAttribute('aria-label', 'Search results');
const footer = node('div', 'ds-footer');
const original = node('button', 'ds-original', 'Open existing search'); original.type = 'button';
footer.append(original);
dialog.append(top, introduction, form, modes, connection, suggestions, status, results, footer);
document.body.append(dialog);

let revision = 0, controller;
function cancel() {
  const pending = results.hasAttribute('aria-busy');
  revision++; controller?.abort(); results.removeAttribute('aria-busy');
  if (pending) status.textContent = 'Search cancelled. Press Enter or Search to try again.';
}
function open() { dialog.showModal(); input.focus(); dialog.scrollTop = 0; track("open"); }
button.addEventListener('click', open);
close.addEventListener('click', () => dialog.close());
dialog.addEventListener('close', () => { cancel(); button.focus(); });
dialog.addEventListener('click', event => { if (event.target === dialog) {
  const bounds = dialog.getBoundingClientRect();
  if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) dialog.close();
} });
original.addEventListener('click', () => {
  dialog.close();
  const existing = document.querySelector('[data-md-toggle="search"]');
  existing?.click();
});
document.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === 'k') {
    event.preventDefault(); if (!dialog.open) open();
  }
});
input.addEventListener('input', () => {
  cancel(); results.replaceChildren();
  status.textContent = input.value.trim() ? 'Press Enter or Search to see results.' : 'Try a topic, rule name, or question.';
});
form.addEventListener('submit', event => { event.preventDefault(); run(); });

async function run() {
  const query = input.value.trim();
  cancel(); const current = revision;
  results.replaceChildren();
  if (!query) { status.textContent = 'Enter a topic or question to search.'; input.focus(); return; }
  controller = new AbortController();
  const method = modes.querySelector('input:checked').value;
  status.textContent = 'Searching the documentation…'; results.setAttribute('aria-busy', 'true');
  const started = performance.now();
  try {
    await native.initialize();
    const matches = method === 'ai'
      ? await cloudflareSearch(query, site.origin, native.index, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]))
      : await native.search(query);
    if (current !== revision) return;
    const milliseconds = Math.round(performance.now() - started);
    if (method === 'keyword') track('keyword', { milliseconds, matches: matches.length });
    status.textContent = matches.length ? `${matches.length} matching sections · ${milliseconds} ms`
      : 'No matching documentation. Try a shorter topic or an exact setting name.';
    for (const [rank, match] of matches.entries()) {
      const row = node('li', 'ds-result');
      const link = node('a', 'ds-result-link', match.title);
      link.href = '/' + match.location;
      link.addEventListener('click', () => { track('click', { rank: rank + 1, method }); dialog.close(); });
      const path = node('p', 'ds-path', match.path.join(' / '));
      const excerpt = node('p', 'ds-excerpt', match.text.slice(0, 350) + (match.text.length > 350 ? '…' : ''));
      row.append(link, path, excerpt); results.append(row);
    }
  } catch (error) {
    if (current !== revision || error.name === 'AbortError') return;
    status.textContent = error.name === 'TimeoutError'
      ? 'AI Search took too long. Try keyword search.' : error.message;
  } finally {
    if (current === revision) results.removeAttribute('aria-busy');
  }
}

// Instant navigation keeps header chrome but may replace it between versions.
window.document$?.subscribe(() => {
  const currentHeader = document.querySelector('.md-header__inner');
  if (currentHeader && !currentHeader.contains(button)) currentHeader.append(button);
});


// Store fixed categories and numeric aggregates only; never the question text.
function track(event, properties = {}) {
  if (location.hostname !== new URL(site.origin).hostname) return;
  fetch('/api/docs-search', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    credentials: 'same-origin', keepalive: true,
    body: JSON.stringify({ event, ...properties }),
  }).catch(() => {});
}
