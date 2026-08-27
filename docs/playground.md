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
  Your Markdown stays in this browser. It is linted locally and is not uploaded.
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
    <button id="pg-fix-btn" class="pg-btn pg-btn-primary" type="button" aria-keyshortcuts="Control+Enter Meta+Enter" disabled>Fix issues</button>
    <button id="pg-undo-btn" class="pg-btn pg-btn-quiet pg-btn-undo" type="button" disabled>Undo</button>
    <button id="pg-clear-btn" class="pg-btn pg-btn-quiet pg-btn-clear" type="button" disabled>Clear</button>
  </div>
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
const TRAIL = ' '.repeat(3);

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
const inputTab = document.getElementById('pg-input-tab');
const issuesTab = document.getElementById('pg-issues-tab');
const inputPanel = document.getElementById('pg-input-panel');
const issuesPanel = document.getElementById('pg-issues-panel');
const mobileMedia = window.matchMedia(MOBILE_QUERY);

let linter = null;
let debounceTimer = null;
let warnings = [];
let undoState = null;
let currentView = 'input';
let currentExample = '';

function announce(message, visible = false) {
  announcerEl.textContent = '';
  window.requestAnimationFrame(() => {
    announcerEl.textContent = message;
  });
  actionStatusEl.textContent = visible ? message : '';
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
    const label = `${rule}, line ${line}, column ${column}: ${warning.message}. Go to issue in editor.`;
    return `<button type="button" class="pg-warning" data-warning-index="${index}" aria-label="${escapeHtml(label)}">
      <span class="pg-warning-header">
        <span class="pg-rule-badge">${rule}</span>
        <span class="pg-location">Line ${line}:${column}</span>
        ${hasFix ? '<span class="pg-fix-badge">Auto-fix</span>' : ''}
      </span>
      <span class="pg-warning-message">${message}</span>
      <span class="pg-warning-hint">Open in editor</span>
    </button>`;
  }).join('');
}

function lint() {
  if (!linter) return;
  try {
    renderWarnings(JSON.parse(linter.check(inputEl.value)));
  } catch (error) {
    warnings = [];
    warningsEl.innerHTML = '<div class="pg-empty">The document could not be checked. Edit the input and try again.</div>';
    fixBtn.disabled = true;
    announce(`Linting failed: ${error.message}`, true);
  }
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

    linter = new mod.Linter({});
    versionEl.textContent = `rumdl v${mod.get_version()}`;
    statusEl.hidden = true;
    toolbarEl.hidden = false;
    panelsEl.hidden = false;
    viewTabsEl.hidden = !mobileMedia.matches;

    inputEl.value = EXAMPLES.common;
    exampleSelect.value = 'common';
    currentExample = 'common';
    updateCharCount();
    lint();
    setView('input');
    announce(`Playground ready. ${warnings.length} issues found.`);
  } catch (error) {
    statusEl.hidden = false;
    statusEl.className = 'pg-status pg-error';
    statusEl.innerHTML = `<span>rumdl could not load. Check your connection and try again.</span><button type="button" class="pg-btn" id="pg-retry-btn">Retry</button>`;
    document.getElementById('pg-retry-btn').addEventListener('click', main, { once: true });
  }
}

inputEl.addEventListener('input', () => {
  exampleSelect.value = '';
  currentExample = '';
  updateCharCount();
  clearUndo();
  actionStatusEl.textContent = '';
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
});

warningsEl.addEventListener('click', (event) => {
  const button = event.target.closest('[data-warning-index]');
  if (button) jumpToWarning(Number(button.dataset.warningIndex));
});

inputTab.addEventListener('click', () => setView('input'));
issuesTab.addEventListener('click', () => setView('issues'));
mobileMedia.addEventListener('change', syncResponsiveView);

main();
</script>
