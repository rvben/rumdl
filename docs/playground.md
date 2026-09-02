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
  <span>Your Markdown stays in this browser. It is linted locally, is not uploaded, and a draft is kept only in this tab until it closes. Interaction analytics records named actions only—never your text.</span>
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
    <div class="pg-command-group" role="group" aria-label="Editor actions">
      <button id="pg-undo-btn" class="pg-command-btn" type="button" disabled>Undo</button>
      <button id="pg-clear-btn" class="pg-command-btn" type="button" disabled>Clear</button>
      <button id="pg-focus-btn" class="pg-command-btn" type="button" aria-pressed="false">Focus</button>
    </div>
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
  <input id="pg-file-input" type="file" accept=".md,.markdown,.mdown,.mkd,.txt,text/markdown,text/plain" hidden>
  <button id="pg-file-btn" class="pg-utility-action" type="button">Open file</button>
  <button id="pg-share-btn" class="pg-utility-action" type="button">Copy share link</button>
  <span id="pg-share-status" class="pg-utility-status" role="status" aria-live="polite"></span>
</div>

<div id="pg-config-panel" class="pg-config" hidden>
  <form id="pg-config-form" novalidate>
    <div class="pg-config-heading">
      <div>
        <h2>Playground configuration</h2>
        <p>Use the same flavor and global settings accepted by the WASM linter.</p>
      </div>
      <span id="pg-config-summary" class="pg-config-summary">Standard · 80 columns · reflow on · all rules</span>
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
          <option value="gh-aw">GitHub Agentic Workflows (preview)</option>
        </select>
      </label>
      <label>
        <span>Line length</span>
        <input id="pg-line-length" class="pg-input-control" type="number" min="20" max="1000" step="1" value="80" inputmode="numeric" aria-describedby="pg-config-error">
      </label>
      <label class="pg-config-reflow">
        <input id="pg-reflow" type="checkbox" checked>
        <span>
          <strong>Reflow paragraphs</strong>
          <small>Wrap long prose when applying fixes</small>
        </span>
      </label>
      <label class="pg-config-rules">
        <span>Disable rules <small>optional, separated by commas</small></span>
        <input id="pg-disable-rules" class="pg-input-control" type="text" placeholder="MD013, MD041" autocomplete="off" spellcheck="false" aria-describedby="pg-config-error">
      </label>
    </div>
    <div class="pg-config-actions">
      <button class="pg-btn pg-btn-primary" type="submit">Apply configuration</button>
      <button id="pg-config-reset" class="pg-btn pg-btn-quiet" type="button">Reset defaults</button>
      <span id="pg-config-status" class="pg-config-status" role="status" aria-live="polite"></span>
    </div>
    <p id="pg-config-error" class="pg-config-error" role="alert" hidden></p>
  </form>
</div>

<div id="pg-action-status" class="pg-action-status"></div>
<div id="pg-announcer" class="pg-visually-hidden" role="status" aria-live="polite" aria-atomic="true"></div>

<div class="pg-workbench" id="pg-panels" hidden>
  <section id="pg-input-panel" class="pg-editor-panel" aria-labelledby="pg-input-heading">
    <div class="pg-panel-header">
      <div class="pg-editor-tab">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 3h7l4 4v14H7z"/><path d="M14 3v5h5"/></svg>
        <h2 id="pg-input-heading">Markdown</h2>
        <span id="pg-document-name" class="pg-document-name">Untitled.md</span>
      </div>
      <div class="pg-editor-meta">
        <button id="pg-active-config" class="pg-config-indicator" type="button" aria-expanded="false" aria-controls="pg-config-panel">Standard · 80 · Reflow</button>
        <span id="pg-char-count" class="pg-meta"></span>
      </div>
    </div>
    <div class="pg-editor-shell">
      <div class="pg-drop-overlay" aria-hidden="true">
        <svg viewBox="0 0 24 24"><path d="M12 16V4m0 0L7.5 8.5M12 4l4.5 4.5"/><path d="M5 14v5h14v-5"/></svg>
        <strong>Drop Markdown here</strong>
        <span>It opens locally in this browser.</span>
      </div>
      <div id="pg-editor" class="pg-editor"></div>
      <p id="pg-editor-help" class="pg-visually-hidden">Edit Markdown. Issues are underlined and listed in the Problems panel. Press Control or Command plus Enter to fix all automatically fixable issues.</p>
    </div>
  </section>
  <section id="pg-issues-panel" class="pg-problems" aria-labelledby="pg-issues-heading">
    <div id="pg-problems-resize" class="pg-problems-resize" role="separator" tabindex="0" aria-label="Resize Problems panel" aria-orientation="horizontal" aria-valuemin="112" aria-valuemax="360" aria-valuenow="224"></div>
    <div class="pg-problems-header">
      <button id="pg-problems-toggle" class="pg-problems-toggle" type="button" aria-expanded="true" aria-controls="pg-warnings">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8 10 4 4 4-4"/></svg>
        <span id="pg-issues-heading">Problems</span>
        <span id="pg-tab-count" class="pg-tab-count">0</span>
      </button>
      <div class="pg-issue-filters" role="group" aria-label="Filter issues">
        <button class="pg-issue-filter" type="button" data-issue-filter="all" aria-pressed="true">All <span id="pg-filter-all-count">0</span></button>
        <button class="pg-issue-filter" type="button" data-issue-filter="fixable" aria-pressed="false">Auto-fixable <span id="pg-filter-fixable-count">0</span></button>
        <button class="pg-issue-filter" type="button" data-issue-filter="manual" aria-pressed="false">Manual <span id="pg-filter-manual-count">0</span></button>
      </div>
      <span id="pg-warning-count" class="pg-visually-hidden"></span>
    </div>
    <div id="pg-warnings" class="pg-results" aria-labelledby="pg-issues-heading"></div>
  </section>
</div>

</div>

<!-- rumdl-disable -->
<template id="pg-script-template">
<script type="module">
import { createRumdlEditor, loadRumdl } from '../javascripts/playground-editor.js';

async function initializeRumdlPlayground() {
const playgroundRoot = document.getElementById('pg-panels');
if (!playgroundRoot || playgroundRoot.dataset.pgReady === 'true') return;
playgroundRoot.dataset.pgReady = 'true';

const SHARE_PREFIX = '#pg=';
const DRAFT_KEY = 'rumdl-playground-draft-v1';
const FOCUS_KEY = 'rumdl-playground-focus-v1';
const MAX_SHARE_URL_LENGTH = 16000;
const MAX_FILE_SIZE = 2 * 1024 * 1024;
const MIN_PROBLEMS_HEIGHT = 112;
const MAX_PROBLEMS_HEIGHT = 360;
const MARKDOWN_EXTENSIONS = ['.md', '.markdown', '.mdown', '.mkd', '.txt'];
const TRAIL = ' '.repeat(3);
const SAMPLE_IMAGE = '![]' + '(image.png)';
const SAMPLE_PHOTO = '![missing alt text]' + '(photo.jpg)';
const SAMPLE_BARE_URL = 'http://' + 'bare-url.com';
const FLAVORS = new Set(['standard', 'mkdocs', 'mdx', 'pandoc', 'quarto', 'obsidian', 'kramdown', 'azure_devops', 'myst', 'hugo', 'mdg', 'gh-aw']);
const DEFAULT_CONFIG = Object.freeze({ flavor: 'standard', lineLength: 80, reflow: true, disable: [] });

const EXAMPLES = {
  common: `# My Document

## Introduction

This document has several common markdown issues.

This intentionally long paragraph demonstrates how rumdl can reflow prose automatically while preserving Markdown structure when you apply its safe fixes.

###Missing space after hash

Some text with trailing spaces${TRAIL}
and more text.

http://example.com is a bare URL.

\`\`\`
code block without language
\`\`\`

${SAMPLE_IMAGE}
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

Or go to ${SAMPLE_BARE_URL} directly.

${SAMPLE_PHOTO}

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
const versionEl = document.getElementById('pg-version');
const summaryEl = document.getElementById('pg-summary');
const actionStatusEl = document.getElementById('pg-action-status');
const announcerEl = document.getElementById('pg-announcer');
const editorMountEl = document.getElementById('pg-editor');
const warningsEl = document.getElementById('pg-warnings');
const warningCountEl = document.getElementById('pg-warning-count');
const charCountEl = document.getElementById('pg-char-count');
const documentNameEl = document.getElementById('pg-document-name');
const editorShellEl = document.querySelector('.pg-editor-shell');
const tabCountEl = document.getElementById('pg-tab-count');
const fixBtn = document.getElementById('pg-fix-btn');
const undoBtn = document.getElementById('pg-undo-btn');
const clearBtn = document.getElementById('pg-clear-btn');
const focusBtn = document.getElementById('pg-focus-btn');
const activeConfigEl = document.getElementById('pg-active-config');
const exampleSelect = document.getElementById('pg-example');
const fileInput = document.getElementById('pg-file-input');
const fileBtn = document.getElementById('pg-file-btn');
const configPanel = document.getElementById('pg-config-panel');
const configForm = document.getElementById('pg-config-form');
const configSummaryEl = document.getElementById('pg-config-summary');
const configStatusEl = document.getElementById('pg-config-status');
const configErrorEl = document.getElementById('pg-config-error');
const configResetBtn = document.getElementById('pg-config-reset');
const flavorSelect = document.getElementById('pg-flavor');
const lineLengthInput = document.getElementById('pg-line-length');
const reflowInput = document.getElementById('pg-reflow');
const disableRulesInput = document.getElementById('pg-disable-rules');
const shareBtn = document.getElementById('pg-share-btn');
const shareStatusEl = document.getElementById('pg-share-status');
const issuesPanel = document.getElementById('pg-issues-panel');
const problemsToggle = document.getElementById('pg-problems-toggle');
const problemsResize = document.getElementById('pg-problems-resize');
const issueFilterButtons = [...document.querySelectorAll('[data-issue-filter]')];
const filterAllCountEl = document.getElementById('pg-filter-all-count');
const filterFixableCountEl = document.getElementById('pg-filter-fixable-count');
const filterManualCountEl = document.getElementById('pg-filter-manual-count');

let linter = null;
let wasmModule = null;
let editor = null;
let debounceTimer = null;
let warnings = [];
let warningsDocument = '';
let currentExample = '';
let currentFileName = '';
let dragDepth = 0;
let issueFilter = 'all';
let problemsExpanded = true;
let problemsHeight = 224;
let activeConfig = { ...DEFAULT_CONFIG, disable: [] };
let destroyed = false;
let focusMode = false;
const sharedResult = readSharedState();
const sharedState = sharedResult.state;
const draftState = sharedState ? null : readDraftState();

function announce(message, visible = false, tone = 'success') {
  announcerEl.textContent = '';
  window.requestAnimationFrame(() => {
    announcerEl.textContent = message;
  });
  actionStatusEl.dataset.tone = tone;
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
  const reflow = typeof value.reflow === 'boolean' ? value.reflow : DEFAULT_CONFIG.reflow;
  const disable = Array.isArray(value.disable)
    ? [...new Set(value.disable.map((rule) => String(rule).toUpperCase()).filter((rule) => /^MD\d{3}$/.test(rule)))].slice(0, 100)
    : [];
  return { flavor, lineLength, reflow, disable };
}

function configFromForm() {
  const rawRules = disableRulesInput.value.trim();
  const tokens = rawRules ? rawRules.split(/[\s,]+/).filter(Boolean).map((rule) => rule.toUpperCase()) : [];
  const invalid = tokens.filter((rule) => !/^MD\d{3}$/.test(rule));
  if (invalid.length > 0) {
    const error = new Error(`Use rule IDs such as MD013. Check: ${invalid.slice(0, 3).join(', ')}.`);
    error.field = disableRulesInput;
    throw error;
  }

  const lineLength = Number(lineLengthInput.value);
  if (!Number.isInteger(lineLength) || lineLength < 20 || lineLength > 1000) {
    const error = new Error('Line length must be a whole number from 20 to 1000.');
    error.field = lineLengthInput;
    throw error;
  }

  return normalizeConfig({
    flavor: flavorSelect.value,
    lineLength,
    reflow: reflowInput.checked,
    disable: tokens,
  });
}

function configOptions(config) {
  return {
    flavor: config.flavor,
    'line-length': config.lineLength,
    MD013: { reflow: config.reflow },
    disable: config.disable,
  };
}

function configLabel(config) {
  const flavorName = flavorSelect.querySelector(`option[value="${config.flavor}"]`)?.textContent || config.flavor;
  const rules = config.disable.length === 0
    ? 'all rules'
    : `${config.disable.length} ${config.disable.length === 1 ? 'rule' : 'rules'} disabled`;
  return `${flavorName} · ${config.lineLength} columns · reflow ${config.reflow ? 'on' : 'off'} · ${rules}`;
}

function compactConfigLabel(config) {
  const flavorName = flavorSelect.querySelector(`option[value="${config.flavor}"]`)?.textContent || config.flavor;
  const disabled = config.disable.length > 0 ? ` · ${config.disable.length} off` : '';
  return `${flavorName} · ${config.lineLength} · ${config.reflow ? 'Reflow' : 'No reflow'}${disabled}`;
}

function clearConfigError() {
  configErrorEl.hidden = true;
  configErrorEl.textContent = '';
  for (const field of [lineLengthInput, disableRulesInput]) {
    field.removeAttribute('aria-invalid');
  }
}

function showConfigError(error) {
  clearConfigError();
  const field = error.field instanceof HTMLElement ? error.field : null;
  configErrorEl.textContent = error.message;
  configErrorEl.hidden = false;
  field?.setAttribute('aria-invalid', 'true');
  field?.focus();
}

function syncConfigForm(config) {
  flavorSelect.value = config.flavor;
  lineLengthInput.value = String(config.lineLength);
  reflowInput.checked = config.reflow;
  disableRulesInput.value = config.disable.join(', ');
  configSummaryEl.textContent = configLabel(config);
  activeConfigEl.textContent = compactConfigLabel(config);
  activeConfigEl.title = `Configure: ${configLabel(config)}`;
}

function createLinter(config) {
  const next = new wasmModule.Linter(configOptions(config));
  let configWarnings;
  try {
    configWarnings = JSON.parse(next.get_config_warnings());
  } catch (error) {
    next.free?.();
    throw error;
  }
  const previous = linter;
  linter = next;
  previous?.free?.();
  activeConfig = normalizeConfig(config);
  syncConfigForm(activeConfig);
  return configWarnings;
}

function applyConfiguration(config, announceResult = true) {
  try {
    const configWarnings = createLinter(config);
    clearConfigError();
    lint();
    persistDraft();
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
    announce(message, true, 'error');
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
  if (!window.location.hash.startsWith(SHARE_PREFIX)) return { state: null, error: false };
  const encoded = window.location.hash.slice(SHARE_PREFIX.length);
  window.history.replaceState(null, '', `${window.location.pathname}${window.location.search}`);
  try {
    return { state: decodeSharedState(encoded), error: false };
  } catch {
    return { state: null, error: true };
  }
}

function readDraftState() {
  try {
    const parsed = JSON.parse(window.sessionStorage.getItem(DRAFT_KEY) || 'null');
    if (parsed?.version !== 1 || typeof parsed.markdown !== 'string' || parsed.markdown.length > MAX_FILE_SIZE) return null;
    const example = EXAMPLES[parsed.example] === parsed.markdown ? parsed.example : '';
    const fileName = typeof parsed.fileName === 'string' ? parsed.fileName.slice(0, 255) : '';
    return { markdown: parsed.markdown, config: normalizeConfig(parsed.config), example, fileName };
  } catch {
    return null;
  }
}

function persistDraft() {
  if (!editor || destroyed) return;
  try {
    window.sessionStorage.setItem(DRAFT_KEY, JSON.stringify({
      version: 1,
      markdown: getContent(),
      config: activeConfig,
      example: currentExample,
      fileName: currentFileName,
    }));
  } catch {
    // The playground remains fully usable when tab storage is unavailable.
  }
}

async function copyText(text) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Fall through for browsers that expose the API but deny permission.
    }
  }
  const previousFocus = document.activeElement;
  const fallback = document.createElement('textarea');
  fallback.value = text;
  fallback.setAttribute('readonly', '');
  fallback.style.position = 'fixed';
  fallback.style.opacity = '0';
  document.body.appendChild(fallback);
  fallback.focus();
  fallback.select();
  const copied = document.execCommand('copy');
  fallback.remove();
  previousFocus?.focus({ preventScroll: true });
  if (!copied) throw new Error('Clipboard unavailable');
}

function getContent() {
  return editor?.getValue() || '';
}

function documentMetadata() {
  return { example: currentExample, fileName: currentFileName };
}

function syncDocumentMetadata(metadata = {}) {
  currentExample = EXAMPLES[metadata.example] === getContent() ? metadata.example : '';
  currentFileName = typeof metadata.fileName === 'string' ? metadata.fileName : '';
  exampleSelect.value = currentExample;
}

function setContent(content, { example = currentExample, fileName = currentFileName, addToHistory = true } = {}) {
  currentExample = EXAMPLES[example] === content ? example : '';
  currentFileName = fileName;
  exampleSelect.value = currentExample;
  editor?.setValue(content, {
    addToHistory,
    metadata: documentMetadata(),
  });
}

function syncUndoState(historyState = {}) {
  undoBtn.disabled = !historyState.canUndo;
  undoBtn.title = historyState.canUndo ? 'Undo the last edit' : 'Nothing to undo';
  if (historyState.metadata) syncDocumentMetadata(historyState.metadata);
}

function updateCharCount() {
  const len = getContent().length;
  const count = `${len} ${len === 1 ? 'character' : 'characters'}`;
  const documentName = currentFileName || 'Untitled.md';
  documentNameEl.textContent = documentName;
  documentNameEl.title = documentName;
  charCountEl.textContent = count;
  clearBtn.disabled = len === 0 && !currentFileName;
}

function isMarkdownFile(file) {
  const name = file.name.toLowerCase();
  return MARKDOWN_EXTENSIONS.some((extension) => name.endsWith(extension))
    || file.type === 'text/markdown'
    || file.type === 'text/plain';
}

async function openMarkdownFile(file) {
  if (!file) return;
  if (!isMarkdownFile(file)) {
    announce('That file was not opened. Choose a Markdown or plain-text file.', true, 'error');
    return;
  }
  if (file.size > MAX_FILE_SIZE) {
    announce('That file is larger than 2 MiB. Choose a smaller document to keep the playground responsive.', true, 'error');
    return;
  }

  try {
    const content = await file.text();
    if (content.includes('\0')) {
      announce('That file appears to contain binary data. Choose a text-based Markdown file.', true, 'error');
      return;
    }

    setContent(content, { example: '', fileName: file.name });
    shareStatusEl.textContent = '';
    updateCharCount();
    lint();
    persistDraft();
    editor.resetScroll();
    const issueLabel = `${warnings.length} ${warnings.length === 1 ? 'issue' : 'issues'}`;
    announce(`Opened ${file.name}. ${issueLabel} found.`, true);
    editor.focus();
  } catch {
    announce('The file could not be read. Check that it is available, then try again.', true, 'error');
  }
}

function setProblemsHeight(nextHeight) {
  problemsHeight = Math.round(Math.min(Math.max(nextHeight, MIN_PROBLEMS_HEIGHT), MAX_PROBLEMS_HEIGHT));
  panelsEl.style.setProperty('--pg-problems-height', `${problemsHeight}px`);
  problemsResize.setAttribute('aria-valuenow', String(problemsHeight));
}

function setProblemsExpanded(expanded) {
  problemsExpanded = expanded;
  issuesPanel.classList.toggle('pg-problems--collapsed', !expanded);
  problemsToggle.setAttribute('aria-expanded', String(expanded));
  if (expanded) setProblemsHeight(problemsHeight);
}

function escapeHtml(text) {
  const element = document.createElement('div');
  element.textContent = String(text);
  return element.innerHTML;
}

function emptyState(kind, title, copy) {
  const icon = kind === 'clean'
    ? '<path d="m5 12 4 4L19 6"/>'
    : kind === 'filtered'
      ? '<path d="M4 7h10M18 7h2M4 17h2M10 17h10"/><circle cx="16" cy="7" r="2"/><circle cx="8" cy="17" r="2"/>'
      : '<path d="M7 3h7l4 4v14H7z"/><path d="M14 3v5h5M10 13h5M10 17h5"/>';
  return `<div class="pg-empty pg-empty--${kind}">
    <svg viewBox="0 0 24 24" aria-hidden="true">${icon}</svg>
    <strong>${escapeHtml(title)}</strong>
    <span>${escapeHtml(copy)}</span>
  </div>`;
}

function setIssueFilter(nextFilter) {
  issueFilter = nextFilter;
  for (const button of issueFilterButtons) {
    button.setAttribute('aria-pressed', String(button.dataset.issueFilter === issueFilter));
  }
  renderWarnings(warnings, warningsDocument);
}

function renderWarnings(nextWarnings, documentSnapshot = getContent()) {
  warnings = nextWarnings;
  warningsDocument = documentSnapshot;
  editor?.setWarnings(warnings, documentSnapshot);
  const issueCount = warnings.length;
  const fixableCount = warnings.filter((warning) => warning.fix != null).length;
  const manualCount = issueCount - fixableCount;
  const visibleWarnings = warnings
    .map((warning, index) => ({ warning, index }))
    .filter(({ warning }) => issueFilter === 'all'
      || (issueFilter === 'fixable' && warning.fix != null)
      || (issueFilter === 'manual' && warning.fix == null));

  filterAllCountEl.textContent = String(issueCount);
  filterFixableCountEl.textContent = String(fixableCount);
  filterManualCountEl.textContent = String(manualCount);

  const isEmpty = getContent().length === 0;
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
    : `${issueCount} ${issueCount === 1 ? 'issue' : 'issues'} · ${fixableCount} auto-fixable`;

  fixBtn.disabled = fixableCount === 0;
  fixBtn.textContent = fixableCount === 0
    ? 'No fixes available'
    : `Fix ${fixableCount} ${fixableCount === 1 ? 'issue' : 'issues'}`;
  fixBtn.title = fixableCount > 0
    ? `${fixBtn.textContent} (Ctrl/⌘ + Enter)`
    : 'No automatic fixes are available';

  if (isEmpty) {
    warningsEl.innerHTML = emptyState('empty', 'Ready when you are', 'Paste Markdown or load an example to start.');
    return;
  }

  if (issueCount === 0) {
    warningsEl.innerHTML = emptyState('clean', 'All clear', 'No issues found. This document is clean.');
    return;
  }

  if (visibleWarnings.length === 0) {
    const label = issueFilter === 'fixable' ? 'automatically fixable' : 'manual';
    warningsEl.innerHTML = emptyState('filtered', `No ${label} issues`, 'Choose another filter to inspect the remaining diagnostics.');
    return;
  }

  warningsEl.innerHTML = visibleWarnings.map(({ warning, index }) => {
    const hasFix = warning.fix != null;
    const rule = escapeHtml(warning.rule_name || 'unknown');
    const line = Number(warning.line) || 1;
    const column = Number(warning.column) || 1;
    const message = escapeHtml(warning.message);
    const fixLabel = `Fix ${rule} at line ${line}, column ${column}`;
    return `<div class="pg-warning" data-fixable="${hasFix}" data-warning-index="${index}">
      <span class="pg-warning-icon" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="M12 3 2.7 20h18.6L12 3Z"/><path d="M12 9v5M12 17.5v.01"/></svg></span>
      <a class="pg-rule-badge" href="../${rule.toLowerCase()}/" aria-label="Read documentation for ${rule}">${rule}</a>
      <button type="button" class="pg-warning-main" data-warning-open="${index}">
        <span class="pg-warning-message">${message}</span>
        <span class="pg-location">${line}:${column}</span>
        <span class="pg-visually-hidden"> — show in editor</span>
      </button>
      ${hasFix ? `<button type="button" class="pg-warning-fix" data-warning-fix="${index}" aria-label="${escapeHtml(fixLabel)}">Fix</button>` : ''}
    </div>`;
  }).join('');
}

function lint() {
  if (!linter) return;
  try {
    const documentSnapshot = getContent();
    renderWarnings(JSON.parse(linter.check(documentSnapshot)), documentSnapshot);
  } catch (error) {
    warnings = [];
    warningsDocument = '';
    filterAllCountEl.textContent = '0';
    filterFixableCountEl.textContent = '0';
    filterManualCountEl.textContent = '0';
    editor?.setWarnings([], '');
    warningsEl.innerHTML = emptyState('filtered', 'Check interrupted', 'Edit the input to run the check again.');
    summaryEl.textContent = 'Check failed';
    warningCountEl.textContent = 'Unavailable';
    tabCountEl.textContent = '!';
    fixBtn.disabled = true;
    fixBtn.textContent = 'No fixes available';
    announce(`Linting failed: ${error.message}`, true, 'error');
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

function fixWarning(index, expectedDocument = warningsDocument) {
  if (expectedDocument !== getContent()) {
    lint();
    announce('The document changed, so diagnostics were refreshed. Choose Fix again.', true, 'error');
    return;
  }
  const warning = warnings[index];
  if (!warning?.fix) return;
  const previous = getContent();
  const fixed = applyWarningFix(previous, warning.fix);
  if (fixed === previous) {
    announce('That issue no longer has an applicable automatic fix.', true);
    track('playground_fix', { scope: 'single', outcome: 'unchanged' });
    return;
  }

  setContent(fixed, { example: '', fileName: currentFileName });
  updateCharCount();
  lint();
  persistDraft();
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
  document.querySelectorAll('.pg-warning').forEach((row) => {
    row.classList.toggle('pg-warning--active', row.dataset.warningIndex === String(index));
  });
  editor.revealWarning(warning);
  announce(`Showing line ${line}, column ${column} in the editor.`);
}

function setCheckingState() {
  warnings = [];
  warningsDocument = '';
  editor?.setWarnings([], '');
  filterAllCountEl.textContent = '0';
  filterFixableCountEl.textContent = '0';
  filterManualCountEl.textContent = '0';
  tabCountEl.textContent = '…';
  summaryEl.textContent = 'Checking…';
  warningCountEl.textContent = 'Checking';
  fixBtn.disabled = true;
  fixBtn.textContent = 'Checking…';
  fixBtn.title = 'Diagnostics are being refreshed';
  warningsEl.innerHTML = emptyState('filtered', 'Checking changes', 'Diagnostics update as you type.');
}

function handleEditorChange(_content, metadata) {
  syncDocumentMetadata(metadata);
  updateCharCount();
  syncUndoState({ canUndo: editor?.canUndo(), metadata: documentMetadata() });
  actionStatusEl.textContent = '';
  shareStatusEl.textContent = '';
  window.clearTimeout(debounceTimer);
  setCheckingState();
  persistDraft();
  debounceTimer = window.setTimeout(() => {
    lint();
    announce(`${warnings.length} ${warnings.length === 1 ? 'issue' : 'issues'} found.`);
  }, 300);
}

function setConfigOpen(open) {
  configPanel.hidden = !open;
  activeConfigEl.setAttribute('aria-expanded', String(open));
  if (open) flavorSelect.focus();
}

function setFocusMode(enabled, persist = true) {
  focusMode = enabled;
  document.getElementById('rumdl-playground')?.classList.toggle('pg-focus-mode', enabled);
  document.body.classList.toggle('pg-focus-active', enabled);
  focusBtn.setAttribute('aria-pressed', String(enabled));
  focusBtn.textContent = enabled ? 'Exit focus' : 'Focus';
  if (persist) {
    try {
      window.localStorage.setItem(FOCUS_KEY, enabled ? 'true' : 'false');
    } catch {
      // Focus mode still works when preference storage is unavailable.
    }
  }
  if (enabled) editor?.focus();
}

function preferredFocusMode() {
  try {
    return window.localStorage.getItem(FOCUS_KEY) === 'true';
  } catch {
    return false;
  }
}

function handleDocumentKeydown(event) {
  if (event.key === 'Escape' && focusMode) {
    setFocusMode(false);
    focusBtn.focus();
  }
}

function destroyPlayground() {
  if (destroyed) return;
  persistDraft();
  destroyed = true;
  window.clearTimeout(debounceTimer);
  editor?.destroy();
  editor = null;
  linter?.free?.();
  linter = null;
  document.removeEventListener('keydown', handleDocumentKeydown);
  document.body.classList.remove('pg-focus-active');
  if (window.rumdlPlaygroundSession?.root === playgroundRoot) delete window.rumdlPlaygroundSession;
}

window.rumdlPlaygroundSession = { root: playgroundRoot, destroy: destroyPlayground };

async function main() {
  statusEl.className = 'pg-status pg-loading';
  statusEl.innerHTML = '<span class="pg-status__spinner" aria-hidden="true"></span><span>Loading the rumdl engine…</span>';
  try {
    const mod = await loadRumdl();
    if (destroyed || !playgroundRoot.isConnected) return;

    wasmModule = mod;
    const initialState = sharedState || draftState;
    createLinter(initialState?.config || DEFAULT_CONFIG);
    const initialContent = initialState?.markdown ?? EXAMPLES.common;
    currentExample = initialState?.example || (initialState ? '' : 'common');
    currentFileName = initialState?.fileName || '';
    if (!editor) {
      editor = createRumdlEditor({
        parent: editorMountEl,
        doc: initialContent,
        metadata: documentMetadata(),
        onChange: handleEditorChange,
        onFix: fixWarning,
        onHistoryChange: syncUndoState,
        onFixAll: () => {
          if (!fixBtn.disabled) fixBtn.click();
        },
      });
    } else {
      setContent(initialContent);
    }
    versionEl.textContent = `rumdl v${mod.get_version()}`;
    statusEl.hidden = true;
    toolbarEl.hidden = false;
    utilityEl.hidden = false;
    panelsEl.hidden = false;

    exampleSelect.value = currentExample;
    updateCharCount();
    syncUndoState({ canUndo: editor.canUndo(), metadata: documentMetadata() });
    lint();
    setProblemsExpanded(true);
    setFocusMode(preferredFocusMode(), false);
    announce(`Playground ready. ${warnings.length} issues found.`);
    track('playground_ready', { source: sharedState ? 'shared' : draftState ? 'draft' : 'default' });
    if (sharedState) {
      shareStatusEl.textContent = 'Shared state loaded. The URL fragment has been removed from this tab.';
    } else if (sharedResult.error) {
      shareStatusEl.textContent = 'This share link was invalid, so your tab draft or the default example was loaded.';
      announce(shareStatusEl.textContent, false, 'error');
    } else if (draftState) {
      shareStatusEl.textContent = 'Restored your draft from this tab.';
      announce(shareStatusEl.textContent);
    }
  } catch (error) {
    if (destroyed || !playgroundRoot.isConnected) return;
    statusEl.hidden = false;
    statusEl.className = 'pg-status pg-error';
    statusEl.innerHTML = `<span>rumdl could not load. Check your connection and try again.</span><button type="button" class="pg-btn" id="pg-retry-btn">Retry</button>`;
    document.getElementById('pg-retry-btn').addEventListener('click', main, { once: true });
    track('playground_error', { stage: 'load' });
  }
}

fileBtn.addEventListener('click', () => fileInput.click());

fileInput.addEventListener('change', async () => {
  await openMarkdownFile(fileInput.files?.[0]);
  fileInput.value = '';
});

editorShellEl.addEventListener('dragenter', (event) => {
  if (!Array.from(event.dataTransfer?.types || []).includes('Files')) return;
  event.preventDefault();
  dragDepth += 1;
  editorShellEl.classList.add('pg-editor-shell--drop-target');
});

editorShellEl.addEventListener('dragover', (event) => {
  if (!Array.from(event.dataTransfer?.types || []).includes('Files')) return;
  event.preventDefault();
  event.dataTransfer.dropEffect = 'copy';
});

editorShellEl.addEventListener('dragleave', () => {
  dragDepth = Math.max(0, dragDepth - 1);
  if (dragDepth === 0) editorShellEl.classList.remove('pg-editor-shell--drop-target');
});

editorShellEl.addEventListener('drop', async (event) => {
  event.preventDefault();
  dragDepth = 0;
  editorShellEl.classList.remove('pg-editor-shell--drop-target');
  const files = [...(event.dataTransfer?.files || [])];
  if (files.length !== 1) {
    announce('Drop one Markdown file at a time.', true, 'error');
    return;
  }
  await openMarkdownFile(files[0]);
});

fixBtn.addEventListener('click', () => {
  if (!linter) return;
  const fixableCount = warnings.filter((warning) => warning.fix != null).length;
  if (fixableCount === 0) return;

  const previous = getContent();
  const fixed = linter.fix(previous);
  if (fixed === previous) {
    announce('No automatic changes were available.', true);
    track('playground_fix', { scope: 'all', outcome: 'unchanged' });
    return;
  }

  setContent(fixed, { example: '', fileName: currentFileName });
  updateCharCount();
  lint();
  persistDraft();
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
  if (!editor?.undo()) return;
  window.clearTimeout(debounceTimer);
  updateCharCount();
  lint();
  persistDraft();
  announce('Undid the last edit.', true);
});

clearBtn.addEventListener('click', () => {
  if (getContent().length === 0 && !currentFileName) return;
  setContent('', { example: '', fileName: '' });
  updateCharCount();
  lint();
  persistDraft();
  announce('Editor cleared. Undo is available.', true);
  editor.focus();
});

exampleSelect.addEventListener('change', () => {
  const key = exampleSelect.value;
  if (!key || !EXAMPLES[key]) return;
  setContent(EXAMPLES[key], { example: key, fileName: '' });
  updateCharCount();
  lint();
  persistDraft();
  announce(`Loaded ${exampleSelect.options[exampleSelect.selectedIndex].text}. ${warnings.length} ${warnings.length === 1 ? 'issue' : 'issues'} found.`, true);
  track('playground_example', { example: key });
});

warningsEl.addEventListener('click', (event) => {
  const openButton = event.target.closest('[data-warning-open]');
  const fixButton = event.target.closest('[data-warning-fix]');
  if (openButton) jumpToWarning(Number(openButton.dataset.warningOpen));
  if (fixButton) fixWarning(Number(fixButton.dataset.warningFix));
});

for (const button of issueFilterButtons) {
  button.addEventListener('click', () => setIssueFilter(button.dataset.issueFilter));
}

problemsToggle.addEventListener('click', () => {
  setProblemsExpanded(!problemsExpanded);
  announce(`Problems panel ${problemsExpanded ? 'expanded' : 'collapsed'}.`);
});

let resizeStartY = 0;
let resizeStartHeight = 0;

problemsResize.addEventListener('pointerdown', (event) => {
  if (!problemsExpanded) return;
  resizeStartY = event.clientY;
  resizeStartHeight = problemsHeight;
  problemsResize.setPointerCapture(event.pointerId);
  panelsEl.classList.add('pg-workbench--resizing');
});

problemsResize.addEventListener('pointermove', (event) => {
  if (!problemsResize.hasPointerCapture(event.pointerId)) return;
  setProblemsHeight(resizeStartHeight + resizeStartY - event.clientY);
});

problemsResize.addEventListener('pointerup', (event) => {
  if (problemsResize.hasPointerCapture(event.pointerId)) problemsResize.releasePointerCapture(event.pointerId);
  panelsEl.classList.remove('pg-workbench--resizing');
});

problemsResize.addEventListener('pointercancel', () => {
  panelsEl.classList.remove('pg-workbench--resizing');
});

problemsResize.addEventListener('keydown', (event) => {
  const step = event.shiftKey ? 48 : 16;
  if (event.key === 'ArrowUp') setProblemsHeight(problemsHeight + step);
  else if (event.key === 'ArrowDown') setProblemsHeight(problemsHeight - step);
  else if (event.key === 'Home') setProblemsHeight(MIN_PROBLEMS_HEIGHT);
  else if (event.key === 'End') setProblemsHeight(MAX_PROBLEMS_HEIGHT);
  else return;
  event.preventDefault();
});

activeConfigEl.addEventListener('click', () => {
  if (focusMode) setFocusMode(false);
  setConfigOpen(configPanel.hidden);
});

focusBtn.addEventListener('click', () => setFocusMode(!focusMode));

document.addEventListener('keydown', handleDocumentKeydown);

configForm.addEventListener('submit', (event) => {
  event.preventDefault();
  try {
    applyConfiguration(configFromForm());
  } catch (error) {
    showConfigError(error);
    const message = `Configuration was not applied. ${error.message}`;
    configStatusEl.textContent = message;
    announce(message, true, 'error');
    track('playground_error', { stage: 'config' });
  }
});

configResetBtn.addEventListener('click', () => {
  clearConfigError();
  syncConfigForm(DEFAULT_CONFIG);
  applyConfiguration(DEFAULT_CONFIG);
});

for (const field of [lineLengthInput, disableRulesInput]) {
  field.addEventListener('input', clearConfigError);
}

shareBtn.addEventListener('click', async () => {
  shareStatusEl.textContent = '';
  try {
    const encoded = encodeSharedState({
      version: 1,
      markdown: getContent(),
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

main();
}

initializeRumdlPlayground();
</script>
</template>
<!-- rumdl-enable -->
