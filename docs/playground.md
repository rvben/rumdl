---
description: "Lint and fix Markdown in your browser with nothing installed, running the same engine as the CLI compiled to WebAssembly."
icon: lucide/play-circle
hide:
  - toc
---

# Playground

<p class="pg-intro">Paste Markdown, inspect every diagnostic, and apply safe fixes with the same rumdl engine that runs in the CLI.</p>

<p class="pg-privacy">
  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3z"/><path d="m9 12 2 2 4-4"/></svg>
  <span>Your Markdown stays in this browser. It is linted locally and is not uploaded. Interaction analytics records named actions only—never your text.</span>
</p>

<div id="rumdl-playground">

<div id="pg-status" class="pg-status pg-loading" role="status">
  <span class="pg-status__spinner" aria-hidden="true"></span>
  <span>Loading the rumdl engine…</span>
</div>

<div class="pg-toolbar" id="pg-toolbar" hidden>
  <div class="pg-toolbar-left">
    <span id="pg-version" class="pg-version"></span>
    <span id="pg-summary" class="pg-summary"></span>
  </div>
  <div class="pg-toolbar-right">
    <button id="pg-fix-btn" class="pg-btn pg-btn-primary" type="button" aria-keyshortcuts="Control+Enter Meta+Enter" disabled>Fix issues</button>
    <button id="pg-undo-btn" class="pg-btn pg-btn-quiet pg-btn-undo" type="button" disabled>Undo</button>
    <button id="pg-clear-btn" class="pg-btn pg-btn-quiet pg-btn-clear" type="button" disabled>Clear</button>
  </div>
</div>

<div class="pg-utility" id="pg-utility" hidden>
  <label class="pg-control-label" for="pg-example">
    <span>Example</span>
    <select id="pg-example" class="pg-select">
      <option value="">Choose a document…</option>
      <option value="common">Common issues</option>
      <option value="headings">Heading structure</option>
      <option value="links">Links and images</option>
      <option value="clean">Clean document</option>
    </select>
  </label>
  <button id="pg-config-toggle" class="pg-btn pg-btn-quiet" type="button" aria-expanded="false" aria-controls="pg-config-panel">Configure</button>
  <button id="pg-share-btn" class="pg-btn pg-btn-quiet" type="button">Copy share link</button>
  <span id="pg-share-status" class="pg-utility-status" role="status" aria-live="polite"></span>
</div>

<div id="pg-config-panel" class="pg-config" hidden>
  <form id="pg-config-form">
    <div class="pg-config-heading">
      <div>
        <h2>Playground configuration</h2>
        <p>Use the same flavor and global settings accepted by the WASM linter.</p>
      </div>
      <span id="pg-config-summary" class="pg-config-summary">Standard · 80 columns · all rules</span>
    </div>
    <div class="pg-config-fields">
      <label>
        <span>Markdown flavor</span>
        <select id="pg-flavor" class="pg-select">
          <option value="standard">Standard</option>
          <option value="mkdocs">MkDocs</option>
          <option value="mdx">MDX</option>
          <option value="myst">MyST</option>
          <option value="quarto">Quarto</option>
          <option value="obsidian">Obsidian</option>
          <option value="pandoc">Pandoc</option>
          <option value="hugo">Hugo</option>
          <option value="kramdown">Kramdown</option>
          <option value="azure_devops">Azure DevOps</option>
          <option value="mdg">MDG</option>
        </select>
      </label>
      <label>
        <span>Line length</span>
        <input id="pg-line-length" class="pg-input-control" type="number" min="20" max="1000" step="1" value="80" inputmode="numeric">
      </label>
      <label class="pg-config-rules">
        <span>Disable rules <small>optional, separated by commas</small></span>
        <input id="pg-disable-rules" class="pg-input-control" type="text" placeholder="MD013, MD041" autocomplete="off" spellcheck="false">
      </label>
    </div>
    <div class="pg-config-actions">
      <button class="pg-btn pg-btn-primary" type="submit">Apply configuration</button>
      <button id="pg-config-reset" class="pg-btn pg-btn-quiet" type="button">Reset defaults</button>
      <span id="pg-config-status" class="pg-config-status" role="status" aria-live="polite"></span>
    </div>
  </form>
</div>

<div id="pg-action-status" class="pg-action-status"></div>
<div id="pg-announcer" class="pg-visually-hidden" role="status" aria-live="polite" aria-atomic="true"></div>

<div class="pg-view-tabs" id="pg-view-tabs" role="tablist" aria-label="Playground view" hidden>
  <button id="pg-input-tab" class="pg-tab" type="button" role="tab" aria-selected="true" aria-controls="pg-input-panel">Input</button>
  <button id="pg-issues-tab" class="pg-tab" type="button" role="tab" aria-selected="false" aria-controls="pg-issues-panel">Issues <span id="pg-tab-count" class="pg-tab-count">0</span></button>
</div>

<div class="pg-panels" id="pg-panels" hidden>
  <section id="pg-input-panel" class="pg-panel" role="tabpanel" aria-labelledby="pg-input-tab pg-input-heading">
    <div class="pg-panel-header">
      <h2 id="pg-input-heading">Markdown input</h2>
      <span id="pg-char-count" class="pg-meta"></span>
    </div>
    <textarea id="pg-input" class="pg-editor" aria-labelledby="pg-input-heading" spellcheck="false" placeholder="Type or paste Markdown here…"></textarea>
  </section>
  <section id="pg-issues-panel" class="pg-panel" role="tabpanel" aria-labelledby="pg-issues-tab pg-issues-heading">
    <div class="pg-panel-header">
      <h2 id="pg-issues-heading">Issues</h2>
      <span id="pg-warning-count" class="pg-meta"></span>
    </div>
    <div id="pg-warnings" class="pg-results" aria-labelledby="pg-issues-heading"></div>
  </section>
</div>

</div>

<script type="module">
const WASM_URL = 'https://cdn.jsdelivr.net/npm/rumdl-wasm/rumdl_lib.js';
const MOBILE_QUERY = '(max-width: 60em)';
const SHARE_PREFIX = '#pg=';
const MAX_SHARE_URL_LENGTH = 16000;
const TRAIL = ' '.repeat(3);
const FLAVORS = new Set(['standard', 'mkdocs', 'mdx', 'pandoc', 'quarto', 'obsidian', 'kramdown', 'azure_devops', 'myst', 'hugo', 'mdg']);
const DEFAULT_CONFIG = Object.freeze({ flavor: 'standard', lineLength: 80, disable: [] });

const EXAMPLES = {
  common: `# My Document

## Introduction

This document has several common markdown issues.

###Missing space after hash

Some text with trailing spaces${TRAIL}
and more text.

http://example.com is a bare URL.

\`\`\`
code block without language
\`\`\`

![](image.png)
`,
  headings: `# Title

### Skipped level two

Content under level 3.

#### Another skip

## Back to level two

Content here.

# Second top-level heading
`,
  links: `# Links and Images

Click [here](http://example.com) to visit.

Or go to http://bare-url.com directly.

![missing alt text](photo.jpg)

[broken reference][ref]

[ref]: http://example.com
`,
  clean: `# Clean Document

## Introduction

This document has no linting issues.

## Code Example

\`\`\`rust
fn main() {
    println!("Hello, world!");
}
\`\`\`

## Links

Visit [rumdl](https://rumdl.dev) for more information.
`,
};

const statusEl = document.getElementById('pg-status');
const toolbarEl = document.getElementById('pg-toolbar');
const utilityEl = document.getElementById('pg-utility');
const panelsEl = document.getElementById('pg-panels');
const viewTabsEl = document.getElementById('pg-view-tabs');
const versionEl = document.getElementById('pg-version');
const summaryEl = document.getElementById('pg-summary');
const actionStatusEl = document.getElementById('pg-action-status');
const announcerEl = document.getElementById('pg-announcer');
const inputEl = document.getElementById('pg-input');
const warningsEl = document.getElementById('pg-warnings');
const warningCountEl = document.getElementById('pg-warning-count');
const charCountEl = document.getElementById('pg-char-count');
const tabCountEl = document.getElementById('pg-tab-count');
const fixBtn = document.getElementById('pg-fix-btn');
const undoBtn = document.getElementById('pg-undo-btn');
const clearBtn = document.getElementById('pg-clear-btn');
const exampleSelect = document.getElementById('pg-example');
const configToggle = document.getElementById('pg-config-toggle');
const configPanel = document.getElementById('pg-config-panel');
const configForm = document.getElementById('pg-config-form');
const configSummaryEl = document.getElementById('pg-config-summary');
const configStatusEl = document.getElementById('pg-config-status');
const configResetBtn = document.getElementById('pg-config-reset');
const flavorSelect = document.getElementById('pg-flavor');
const lineLengthInput = document.getElementById('pg-line-length');
const disableRulesInput = document.getElementById('pg-disable-rules');
const shareBtn = document.getElementById('pg-share-btn');
const shareStatusEl = document.getElementById('pg-share-status');
const inputTab = document.getElementById('pg-input-tab');
const issuesTab = document.getElementById('pg-issues-tab');
const inputPanel = document.getElementById('pg-input-panel');
const issuesPanel = document.getElementById('pg-issues-panel');
const mobileMedia = window.matchMedia(MOBILE_QUERY);

let linter = null;
let wasmModule = null;
let debounceTimer = null;
let warnings = [];
let undoState = null;
let currentView = 'input';
let currentExample = '';
let activeConfig = { ...DEFAULT_CONFIG, disable: [] };
const sharedState = readSharedState();

function announce(message, visible = false) {
  announcerEl.textContent = '';
  window.requestAnimationFrame(() => {
    announcerEl.textContent = message;
  });
  actionStatusEl.textContent = visible ? message : '';
}

function track(eventName, properties = {}) {
  window.rumdlAnalytics?.track(eventName, properties);
}

function disabledCountBucket(count) {
  if (count === 0) return '0';
  if (count === 1) return '1';
  if (count < 5) return '2_4';
  return '5_plus';
}

function lineLengthBucket(value) {
  if (value < 80) return 'under_80';
  if (value === 80) return '80';
  if (value <= 120) return '81_120';
  return 'over_120';
}

function normalizeConfig(value = {}) {
  const flavor = FLAVORS.has(value.flavor) ? value.flavor : DEFAULT_CONFIG.flavor;
  const parsedLineLength = Number(value.lineLength);
  const lineLength = Number.isInteger(parsedLineLength) && parsedLineLength >= 20 && parsedLineLength <= 1000
    ? parsedLineLength
    : DEFAULT_CONFIG.lineLength;
  const disable = Array.isArray(value.disable)
    ? [...new Set(value.disable.map((rule) => String(rule).toUpperCase()).filter((rule) => /^MD\d{3}$/.test(rule)))].slice(0, 100)
    : [];
  return { flavor, lineLength, disable };
}

function configFromForm() {
  const rawRules = disableRulesInput.value.trim();
  const tokens = rawRules ? rawRules.split(/[\s,]+/).filter(Boolean).map((rule) => rule.toUpperCase()) : [];
  const invalid = tokens.filter((rule) => !/^MD\d{3}$/.test(rule));
  if (invalid.length > 0) {
    throw new Error(`Use rule IDs such as MD013. Check: ${invalid.slice(0, 3).join(', ')}.`);
  }

  const lineLength = Number(lineLengthInput.value);
  if (!Number.isInteger(lineLength) || lineLength < 20 || lineLength > 1000) {
    throw new Error('Line length must be a whole number from 20 to 1000.');
  }

  return normalizeConfig({
    flavor: flavorSelect.value,
    lineLength,
    disable: tokens,
  });
}

function configOptions(config) {
  return {
    flavor: config.flavor,
    'line-length': config.lineLength,
    disable: config.disable,
  };
}

function configLabel(config) {
  const flavorName = flavorSelect.querySelector(`option[value="${config.flavor}"]`)?.textContent || config.flavor;
  const rules = config.disable.length === 0
    ? 'all rules'
    : `${config.disable.length} ${config.disable.length === 1 ? 'rule' : 'rules'} disabled`;
  return `${flavorName} · ${config.lineLength} columns · ${rules}`;
}

function syncConfigForm(config) {
  flavorSelect.value = config.flavor;
  lineLengthInput.value = String(config.lineLength);
  disableRulesInput.value = config.disable.join(', ');
  configSummaryEl.textContent = configLabel(config);
}

function createLinter(config) {
  const next = new wasmModule.Linter(configOptions(config));
  const configWarnings = JSON.parse(next.get_config_warnings());
  linter = next;
  activeConfig = normalizeConfig(config);
  syncConfigForm(activeConfig);
  return configWarnings;
}

function applyConfiguration(config, announceResult = true) {
  try {
    const configWarnings = createLinter(config);
    lint();
    const message = configWarnings.length === 0
      ? `Applied ${configLabel(activeConfig)}.`
      : `Configuration applied with ${configWarnings.length} ${configWarnings.length === 1 ? 'warning' : 'warnings'}.`;
    configStatusEl.textContent = message;
    if (announceResult) announce(message, true);
    track('playground_config', {
      flavor: activeConfig.flavor,
      disabled: disabledCountBucket(activeConfig.disable.length),
      line_length: lineLengthBucket(activeConfig.lineLength),
    });
    return true;
  } catch (error) {
    const message = `Configuration was not applied. ${error.message}`;
    configStatusEl.textContent = message;
    announce(message, true);
    track('playground_error', { stage: 'config' });
    return false;
  }
}

function bytesToBase64Url(bytes) {
  let binary = '';
  for (let index = 0; index < bytes.length; index += 8192) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 8192));
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '');
}

function base64UrlToBytes(value) {
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/');
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=');
  const binary = atob(padded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function encodeSharedState(state) {
  return bytesToBase64Url(new TextEncoder().encode(JSON.stringify(state)));
}

function decodeSharedState(value) {
  const parsed = JSON.parse(new TextDecoder().decode(base64UrlToBytes(value)));
  if (parsed?.version !== 1 || typeof parsed.markdown !== 'string' || parsed.markdown.length > 100000) {
    throw new Error('Unsupported shared state.');
  }
  return { markdown: parsed.markdown, config: normalizeConfig(parsed.config) };
}

function readSharedState() {
  if (!window.location.hash.startsWith(SHARE_PREFIX)) return null;
  const encoded = window.location.hash.slice(SHARE_PREFIX.length);
  window.history.replaceState(null, '', `${window.location.pathname}${window.location.search}`);
  try {
    return decodeSharedState(encoded);
  } catch {
    return null;
  }
}

async function copyText(text) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const fallback = document.createElement('textarea');
  fallback.value = text;
  fallback.setAttribute('readonly', '');
  fallback.style.position = 'fixed';
  fallback.style.opacity = '0';
  document.body.appendChild(fallback);
  fallback.select();
  const copied = document.execCommand('copy');
  fallback.remove();
  if (!copied) throw new Error('Clipboard unavailable');
}

function setUndo(content, action, example = exampleSelect.value) {
  undoState = { content, action, example };
  undoBtn.disabled = false;
  undoBtn.title = `Undo ${action}`;
}

function clearUndo() {
  undoState = null;
  undoBtn.disabled = true;
  undoBtn.removeAttribute('title');
}

function updateCharCount() {
  const len = inputEl.value.length;
  charCountEl.textContent = `${len} ${len === 1 ? 'character' : 'characters'}`;
  clearBtn.disabled = len === 0;
}

function syncResponsiveView() {
  const mobile = mobileMedia.matches;
  viewTabsEl.hidden = !mobile;
  inputPanel.hidden = mobile && currentView !== 'input';
  issuesPanel.hidden = mobile && currentView !== 'issues';
}

function setView(view) {
  currentView = view;
  inputTab.setAttribute('aria-selected', String(view === 'input'));
  issuesTab.setAttribute('aria-selected', String(view === 'issues'));
  inputTab.tabIndex = view === 'input' ? 0 : -1;
  issuesTab.tabIndex = view === 'issues' ? 0 : -1;
  syncResponsiveView();
}

function escapeHtml(text) {
  const element = document.createElement('div');
  element.textContent = String(text);
  return element.innerHTML;
}

function renderWarnings(nextWarnings) {
  warnings = nextWarnings;
  const issueCount = warnings.length;
  const fixableCount = warnings.filter((warning) => warning.fix != null).length;

  const isEmpty = inputEl.value.length === 0;
  warningCountEl.textContent = isEmpty
    ? 'No input'
    : issueCount === 0
      ? 'No issues'
    : `${issueCount} ${issueCount === 1 ? 'issue' : 'issues'}`;
  tabCountEl.textContent = String(issueCount);
  summaryEl.textContent = isEmpty
    ? 'Empty'
    : issueCount === 0
      ? 'Clean'
    : `${issueCount} ${issueCount === 1 ? 'issue' : 'issues'} · ${fixableCount} fixable`;

  fixBtn.disabled = fixableCount === 0;
  fixBtn.textContent = fixableCount === 0
    ? 'No fixes available'
    : `Fix ${fixableCount} ${fixableCount === 1 ? 'issue' : 'issues'}`;
  fixBtn.title = fixableCount > 0
    ? `${fixBtn.textContent} (Ctrl/⌘ + Enter)`
    : 'No automatic fixes are available';

  if (isEmpty) {
    warningsEl.innerHTML = '<div class="pg-empty">Paste Markdown or load an example to start.</div>';
    return;
  }

  if (issueCount === 0) {
    warningsEl.innerHTML = '<div class="pg-empty pg-clean">No issues found. This document is clean.</div>';
    return;
  }

  warningsEl.innerHTML = warnings.map((warning, index) => {
    const hasFix = warning.fix != null;
    const rule = escapeHtml(warning.rule_name || 'unknown');
    const line = Number(warning.line) || 1;
    const column = Number(warning.column) || 1;
    const message = escapeHtml(warning.message);
    const openLabel = `Open ${rule} at line ${line}, column ${column} in the editor`;
    const fixLabel = `Fix ${rule} at line ${line}, column ${column}`;
    return `<div class="pg-warning">
      <span class="pg-warning-header">
        <span class="pg-rule-badge">${rule}</span>
        <span class="pg-location">Line ${line}:${column}</span>
        ${hasFix ? '<span class="pg-fix-badge">Auto-fix</span>' : ''}
      </span>
      <span class="pg-warning-message">${message}</span>
      <span class="pg-warning-actions">
        <button type="button" class="pg-warning-action" data-warning-open="${index}" aria-label="${escapeHtml(openLabel)}">Open in editor</button>
        ${hasFix ? `<button type="button" class="pg-warning-action pg-warning-fix" data-warning-fix="${index}" aria-label="${escapeHtml(fixLabel)}">Fix this issue</button>` : ''}
      </span>
    </div>`;
  }).join('');
}

function lint() {
  if (!linter) return;
  try {
    renderWarnings(JSON.parse(linter.check(inputEl.value)));
  } catch (error) {
    warnings = [];
    warningsEl.innerHTML = '<div class="pg-empty">The document could not be checked. Edit the input and try again.</div>';
    summaryEl.textContent = 'Check failed';
    warningCountEl.textContent = 'Unavailable';
    tabCountEl.textContent = '!';
    fixBtn.disabled = true;
    fixBtn.textContent = 'No fixes available';
    announce(`Linting failed: ${error.message}`, true);
    track('playground_error', { stage: 'lint' });
  }
}

function flattenFixes(fix) {
  return [fix, ...(fix.additional_edits || []).flatMap(flattenFixes)];
}

function applyWarningFix(content, fix) {
  const characters = Array.from(content);
  const edits = flattenFixes(fix)
    .map((edit) => ({
      start: Number(edit.range?.start),
      end: Number(edit.range?.end),
      replacement: String(edit.replacement ?? ''),
    }))
    .filter((edit) => Number.isInteger(edit.start) && Number.isInteger(edit.end) && edit.start >= 0 && edit.end >= edit.start)
    .sort((left, right) => right.start - left.start || right.end - left.end);

  for (const edit of edits) {
    characters.splice(edit.start, edit.end - edit.start, ...Array.from(edit.replacement));
  }
  return characters.join('');
}

function fixWarning(index) {
  const warning = warnings[index];
  if (!warning?.fix) return;
  const previous = inputEl.value;
  const fixed = applyWarningFix(previous, warning.fix);
  if (fixed === previous) {
    announce('That issue no longer has an applicable automatic fix.', true);
    track('playground_fix', { scope: 'single', outcome: 'unchanged' });
    return;
  }

  setUndo(previous, `fix for ${warning.rule_name || 'issue'}`);
  inputEl.value = fixed;
  exampleSelect.value = '';
  currentExample = '';
  updateCharCount();
  lint();
  const outcome = warnings.length === 0 ? 'clean' : 'remaining';
  announce(
    warnings.length === 0
      ? `Fixed ${warning.rule_name || 'the issue'}. The document is clean.`
      : `Fixed ${warning.rule_name || 'the issue'}. ${warnings.length} ${warnings.length === 1 ? 'issue remains' : 'issues remain'}.`,
    true,
  );
  track('playground_fix', { scope: 'single', outcome });
}

function jumpToWarning(index) {
  const warning = warnings[index];
  if (!warning) return;
  const line = Math.max(1, Number(warning.line) || 1);
  const column = Math.max(1, Number(warning.column) || 1);
  const lines = inputEl.value.split('\n');
  const lineStart = lines.slice(0, line - 1).reduce((total, value) => total + value.length + 1, 0);
  const start = Math.min(inputEl.value.length, lineStart + column - 1);

  setView('input');
  inputEl.focus();
  inputEl.setSelectionRange(start, Math.min(start + 1, inputEl.value.length));
  inputEl.scrollTop = Math.max(0, (line - 3) * 21);
  announce(`Moved to line ${line}, column ${column}.`);
}

async function main() {
  statusEl.className = 'pg-status pg-loading';
  statusEl.innerHTML = '<span class="pg-status__spinner" aria-hidden="true"></span><span>Loading the rumdl engine…</span>';
  try {
    const mod = await import(WASM_URL);
    await mod.default();

    wasmModule = mod;
    createLinter(sharedState?.config || DEFAULT_CONFIG);
    versionEl.textContent = `rumdl v${mod.get_version()}`;
    statusEl.hidden = true;
    toolbarEl.hidden = false;
    utilityEl.hidden = false;
    panelsEl.hidden = false;
    viewTabsEl.hidden = !mobileMedia.matches;

    inputEl.value = sharedState?.markdown ?? EXAMPLES.common;
    exampleSelect.value = sharedState ? '' : 'common';
    currentExample = sharedState ? '' : 'common';
    updateCharCount();
    lint();
    setView('input');
    announce(`Playground ready. ${warnings.length} issues found.`);
    track('playground_ready', { source: sharedState ? 'shared' : 'default' });
    if (sharedState) {
      shareStatusEl.textContent = 'Shared state loaded. The URL fragment has been removed from this tab.';
    }
  } catch (error) {
    statusEl.hidden = false;
    statusEl.className = 'pg-status pg-error';
    statusEl.innerHTML = `<span>rumdl could not load. Check your connection and try again.</span><button type="button" class="pg-btn" id="pg-retry-btn">Retry</button>`;
    document.getElementById('pg-retry-btn').addEventListener('click', main, { once: true });
    track('playground_error', { stage: 'load' });
  }
}

inputEl.addEventListener('input', () => {
  exampleSelect.value = '';
  currentExample = '';
  updateCharCount();
  clearUndo();
  actionStatusEl.textContent = '';
  shareStatusEl.textContent = '';
  window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => {
    lint();
    announce(`${warnings.length} ${warnings.length === 1 ? 'issue' : 'issues'} found.`);
  }, 300);
});

inputEl.addEventListener('keydown', (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter' && !fixBtn.disabled) {
    event.preventDefault();
    fixBtn.click();
  }
});

fixBtn.addEventListener('click', () => {
  if (!linter) return;
  const fixableCount = warnings.filter((warning) => warning.fix != null).length;
  if (fixableCount === 0) return;

  const previous = inputEl.value;
  const fixed = linter.fix(previous);
  if (fixed === previous) {
    announce('No automatic changes were available.', true);
    track('playground_fix', { scope: 'all', outcome: 'unchanged' });
    return;
  }

  setUndo(previous, 'automatic fixes');
  inputEl.value = fixed;
  updateCharCount();
  lint();
  const remaining = warnings.length;
  announce(
    remaining === 0
      ? `Fixed ${fixableCount} ${fixableCount === 1 ? 'issue' : 'issues'}. The document is clean.`
      : `Fixed ${fixableCount} ${fixableCount === 1 ? 'issue' : 'issues'}. ${remaining} ${remaining === 1 ? 'issue needs' : 'issues need'} a manual edit.`,
    true,
  );
  track('playground_fix', { scope: 'all', outcome: remaining === 0 ? 'clean' : 'remaining' });
});

undoBtn.addEventListener('click', () => {
  if (!undoState) return;
  const { content, action, example } = undoState;
  inputEl.value = content;
  exampleSelect.value = example;
  currentExample = example;
  updateCharCount();
  clearUndo();
  lint();
  announce(`Undid ${action}.`, true);
});

clearBtn.addEventListener('click', () => {
  if (inputEl.value.length === 0) return;
  setUndo(inputEl.value, 'clear');
  inputEl.value = '';
  exampleSelect.value = '';
  currentExample = '';
  updateCharCount();
  lint();
  announce('Editor cleared. Undo is available.', true);
  inputEl.focus();
});

exampleSelect.addEventListener('change', () => {
  const key = exampleSelect.value;
  if (!key || !EXAMPLES[key]) return;
  if (inputEl.value !== EXAMPLES[key]) setUndo(inputEl.value, 'example change', currentExample);
  inputEl.value = EXAMPLES[key];
  currentExample = key;
  updateCharCount();
  lint();
  announce(`Loaded ${exampleSelect.options[exampleSelect.selectedIndex].text}. ${warnings.length} ${warnings.length === 1 ? 'issue' : 'issues'} found.`, true);
  track('playground_example', { example: key });
});

warningsEl.addEventListener('click', (event) => {
  const openButton = event.target.closest('[data-warning-open]');
  const fixButton = event.target.closest('[data-warning-fix]');
  if (openButton) jumpToWarning(Number(openButton.dataset.warningOpen));
  if (fixButton) fixWarning(Number(fixButton.dataset.warningFix));
});

inputTab.addEventListener('click', () => setView('input'));
issuesTab.addEventListener('click', () => setView('issues'));
viewTabsEl.addEventListener('keydown', (event) => {
  const tabs = [inputTab, issuesTab];
  const currentIndex = tabs.indexOf(document.activeElement);
  if (currentIndex === -1) return;

  let nextIndex = currentIndex;
  if (event.key === 'ArrowRight') nextIndex = (currentIndex + 1) % tabs.length;
  else if (event.key === 'ArrowLeft') nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
  else if (event.key === 'Home') nextIndex = 0;
  else if (event.key === 'End') nextIndex = tabs.length - 1;
  else return;

  event.preventDefault();
  const nextTab = tabs[nextIndex];
  setView(nextTab === inputTab ? 'input' : 'issues');
  nextTab.focus();
});

configToggle.addEventListener('click', () => {
  const willOpen = configPanel.hidden;
  configPanel.hidden = !willOpen;
  configToggle.setAttribute('aria-expanded', String(willOpen));
  if (willOpen) flavorSelect.focus();
});

configForm.addEventListener('submit', (event) => {
  event.preventDefault();
  try {
    applyConfiguration(configFromForm());
  } catch (error) {
    const message = `Configuration was not applied. ${error.message}`;
    configStatusEl.textContent = message;
    announce(message, true);
    track('playground_error', { stage: 'config' });
  }
});

configResetBtn.addEventListener('click', () => {
  syncConfigForm(DEFAULT_CONFIG);
  applyConfiguration(DEFAULT_CONFIG);
});

shareBtn.addEventListener('click', async () => {
  shareStatusEl.textContent = '';
  try {
    const encoded = encodeSharedState({
      version: 1,
      markdown: inputEl.value,
      config: activeConfig,
    });
    const url = new URL(window.location.href);
    url.hash = `${SHARE_PREFIX.slice(1)}${encoded}`;
    if (url.href.length > MAX_SHARE_URL_LENGTH) {
      shareStatusEl.textContent = 'This document is too large for a reliable share URL. Shorten it and try again.';
      announce(shareStatusEl.textContent);
      track('playground_share', { result: 'too_large' });
      return;
    }
    await copyText(url.href);
    shareStatusEl.textContent = 'Link copied. Anyone with it can read this Markdown and configuration.';
    announce(shareStatusEl.textContent);
    track('playground_share', { result: 'success' });
  } catch {
    shareStatusEl.textContent = 'The share link could not be copied. Check clipboard permissions and try again.';
    announce(shareStatusEl.textContent);
    track('playground_share', { result: 'failure' });
    track('playground_error', { stage: 'share' });
  }
});

mobileMedia.addEventListener('change', syncResponsiveView);

main();
</script>
