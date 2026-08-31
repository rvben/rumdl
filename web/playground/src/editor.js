/*! CodeMirror 6 and its runtime dependencies are MIT licensed.
 *  See /javascripts/playground-editor.licenses.txt for notices. */

import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import {
  defaultKeymap,
  history,
  historyKeymap,
  invertedEffects,
  isolateHistory,
  undo,
  undoDepth,
} from '@codemirror/commands';
import {
  HighlightStyle,
  bracketMatching,
  syntaxHighlighting,
} from '@codemirror/language';
import { markdownLanguage } from '@codemirror/lang-markdown';
import { lintGutter, lintKeymap, setDiagnostics } from '@codemirror/lint';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import {
  Annotation,
  EditorState,
  StateEffect,
  StateField,
  Transaction,
} from '@codemirror/state';
import {
  EditorView,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  placeholder,
  rectangularSelection,
  scrollPastEnd,
} from '@codemirror/view';
import { tags } from '@lezer/highlight';
import initRumdlWasm, { Linter, get_version as getRumdlVersion } from 'rumdl-wasm';

const externalUpdate = Annotation.define();
const setDocumentMetadata = StateEffect.define();
let wasmReady;

export async function loadRumdl() {
  wasmReady ||= initRumdlWasm();
  await wasmReady;
  return { Linter, get_version: getRumdlVersion };
}

const rumdlHighlightStyle = HighlightStyle.define([
  { tag: tags.heading, color: 'var(--rm-accent)', fontWeight: '700' },
  { tag: [tags.link, tags.url], color: 'var(--rm-success)', textDecoration: 'underline' },
  { tag: tags.strong, color: 'var(--rm-ink)', fontWeight: '700' },
  { tag: tags.emphasis, color: 'var(--rm-ink)', fontStyle: 'italic' },
  { tag: [tags.monospace, tags.contentSeparator], color: 'var(--rm-warning)' },
  { tag: [tags.meta, tags.processingInstruction], color: 'var(--rm-muted)' },
  { tag: tags.comment, color: 'var(--rm-muted)', fontStyle: 'italic' },
]);

function clamp(value, minimum, maximum) {
  return Math.min(Math.max(value, minimum), maximum);
}

function offsetAt(doc, lineNumber, columnNumber) {
  const safeLineNumber = clamp(Number(lineNumber) || 1, 1, doc.lines);
  const line = doc.line(safeLineNumber);
  const characterIndex = Math.max(0, (Number(columnNumber) || 1) - 1);
  const prefix = Array.from(line.text).slice(0, characterIndex).join('');
  return Math.min(line.to, line.from + prefix.length);
}

function nextCharacterOffset(text, offset) {
  if (offset >= text.length) return offset;
  const codePoint = text.codePointAt(offset);
  return Math.min(text.length, offset + (codePoint > 0xffff ? 2 : 1));
}

function rangeForWarning(state, warning) {
  const from = offsetAt(state.doc, warning.line, warning.column);
  const endLine = Number(warning.end_line) || Number(warning.line) || 1;
  const endColumn = Number(warning.end_column) || (Number(warning.column) || 1) + 1;
  let to = offsetAt(state.doc, endLine, endColumn);

  if (to <= from) {
    to = nextCharacterOffset(state.doc.toString(), from);
  }

  return { from, to: Math.max(from, to) };
}

function diagnosticForWarning(state, warning, index, documentSnapshot, onFix) {
  const range = rangeForWarning(state, warning);
  const severity = ['error', 'warning', 'info'].includes(warning.severity)
    ? warning.severity
    : 'warning';

  return {
    ...range,
    severity,
    source: warning.rule_name || 'rumdl',
    message: warning.message,
    markClass: warning.fix == null ? 'cm-rumdl-manual' : 'cm-rumdl-fixable',
    actions: warning.fix == null
      ? undefined
      : [{
        name: 'Fix',
        apply() {
          onFix(index, documentSnapshot);
        },
      }],
  };
}

export function createRumdlEditor({
  parent,
  doc = '',
  metadata = {},
  onChange,
  onFix,
  onFixAll,
  onHistoryChange,
}) {
  const documentMetadata = StateField.define({
    create() {
      return metadata;
    },
    update(value, transaction) {
      for (const effect of transaction.effects) {
        if (effect.is(setDocumentMetadata)) value = effect.value;
      }
      return value;
    },
  });

  const state = EditorState.create({
    doc,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      documentMetadata,
      invertedEffects.of((transaction) => {
        const effects = [];
        for (const effect of transaction.effects) {
          if (effect.is(setDocumentMetadata)) {
            effects.push(setDocumentMetadata.of(transaction.startState.field(documentMetadata)));
          }
        }
        return effects;
      }),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      EditorState.tabSize.of(2),
      bracketMatching(),
      closeBrackets(),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      EditorView.lineWrapping,
      scrollPastEnd(),
      placeholder('Type or paste Markdown here…'),
      markdownLanguage,
      syntaxHighlighting(rumdlHighlightStyle),
      lintGutter({ hoverTime: 180 }),
      keymap.of([
        {
          key: 'Mod-Enter',
          preventDefault: true,
          run() {
            onFixAll();
            return true;
          },
        },
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...lintKeymap,
      ]),
      EditorView.contentAttributes.of({
        'aria-label': 'Markdown editor',
        'aria-describedby': 'pg-editor-help',
        autocapitalize: 'off',
        autocomplete: 'off',
        spellcheck: 'false',
      }),
      EditorView.updateListener.of((update) => {
        onHistoryChange?.({
          canUndo: undoDepth(update.state) > 0,
          metadata: update.state.field(documentMetadata),
        });
        if (update.docChanged) {
          const isExternal = update.transactions.some((transaction) => transaction.annotation(externalUpdate));
          if (!isExternal) {
            onChange(update.state.doc.toString(), update.state.field(documentMetadata));
          }
        }
      }),
    ],
  });

  const view = new EditorView({ state, parent });

  return {
    focus() {
      view.focus();
    },

    getValue() {
      return view.state.doc.toString();
    },

    setValue(value, options = {}) {
      const nextValue = String(value);
      const currentValue = view.state.doc.toString();
      const nextMetadata = options.metadata ?? view.state.field(documentMetadata);
      const metadataChanged = nextMetadata !== view.state.field(documentMetadata);
      if (nextValue === currentValue && !metadataChanged) return;

      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: nextValue },
        effects: setDocumentMetadata.of(nextMetadata),
        annotations: [
          externalUpdate.of(true),
          Transaction.addToHistory.of(options.addToHistory !== false),
          isolateHistory.of('full'),
        ],
      });
    },

    setMetadata(nextMetadata) {
      view.dispatch({
        effects: setDocumentMetadata.of(nextMetadata),
        annotations: Transaction.addToHistory.of(false),
      });
    },

    canUndo() {
      return undoDepth(view.state) > 0;
    },

    undo() {
      return undo(view);
    },

    resetScroll() {
      view.scrollDOM.scrollTop = 0;
      view.scrollDOM.scrollLeft = 0;
    },

    setWarnings(warnings, documentSnapshot = view.state.doc.toString()) {
      const diagnostics = warnings.map((warning, index) => (
        diagnosticForWarning(view.state, warning, index, documentSnapshot, onFix)
      ));
      view.dispatch(setDiagnostics(view.state, diagnostics));
    },

    revealWarning(warning) {
      const range = rangeForWarning(view.state, warning);
      view.dispatch({
        selection: { anchor: range.from, head: range.to },
        effects: EditorView.scrollIntoView(range.from, { y: 'center' }),
      });
      view.focus();
    },

    destroy() {
      view.destroy();
    },
  };
}
