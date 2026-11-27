// The CodeMirror source editor.
import { closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentOnInput, indentUnit } from "@codemirror/language";
import { lintGutter, setDiagnostics } from "@codemirror/lint";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, Range, StateEffect, StateField } from "@codemirror/state";
import {
  Decoration,
  DecorationSet,
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import type { Diagnostic } from "./backend";

const setHighlights = StateEffect.define<DecorationSet>();

// Highlighting arrives asynchronously from the Typst parser. Until it does,
// the previous decorations move along with the edits.
const highlights = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, transaction) {
    value = value.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(setHighlights)) value = effect.value;
    }
    return value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

const wrapping = new Compartment();

export interface EditorHooks {
  onChange: () => void;
  onCursor: (line: number, column: number) => void;
}

export class Editor {
  readonly view: EditorView;
  private wrap: boolean;
  private readonly hooks: EditorHooks;
  private marks = new Map<string, Decoration>();

  constructor(parent: HTMLElement, wrap: boolean, hooks: EditorHooks) {
    this.wrap = wrap;
    this.hooks = hooks;
    this.view = new EditorView({ parent, state: this.createState("") });
  }

  private createState(doc: string): EditorState {
    return EditorState.create({
      doc,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        history(),
        drawSelection(),
        indentOnInput(),
        indentUnit.of("  "),
        bracketMatching(),
        closeBrackets(),
        highlightSelectionMatches(),
        search({ top: true }),
        lintGutter(),
        highlights,
        wrapping.of(this.wrap ? EditorView.lineWrapping : []),
        EditorView.contentAttributes.of({ spellcheck: "false", "aria-label": "Typst source" }),
        keymap.of([
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...searchKeymap,
          ...historyKeymap,
          ...completionKeymap,
          indentWithTab,
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) this.hooks.onChange();
          if (update.docChanged || update.selectionSet) this.reportCursor(update.state);
        }),
      ],
    });
  }

  private reportCursor(state: EditorState) {
    const head = state.selection.main.head;
    const line = state.doc.lineAt(head);
    this.hooks.onCursor(line.number, head - line.from + 1);
  }

  get text(): string {
    return this.view.state.doc.toString();
  }

  /** Show another document with a fresh undo history. */
  load(text: string) {
    this.view.setState(this.createState(text));
    this.reportCursor(this.view.state);
    this.view.focus();
  }

  setWrap(wrap: boolean) {
    this.wrap = wrap;
    this.view.dispatch({
      effects: wrapping.reconfigure(wrap ? EditorView.lineWrapping : []),
    });
  }

  /** Apply `[from, to, tag]` triples; `classes` maps tags to CSS classes. */
  applyHighlights(spans: number[], classes: string[]) {
    const length = this.view.state.doc.length;
    const ranges: Range<Decoration>[] = [];
    for (let i = 0; i + 2 < spans.length; i += 3) {
      const from = spans[i];
      const to = Math.min(spans[i + 1], length);
      if (from >= to) continue;
      const name = classes[spans[i + 2]];
      let mark = this.marks.get(name);
      if (!mark) this.marks.set(name, (mark = Decoration.mark({ class: name })));
      ranges.push(mark.range(from, to));
    }
    this.view.dispatch({ effects: setHighlights.of(Decoration.set(ranges, true)) });
  }

  /** Underline problems that belong to this document. */
  showDiagnostics(diagnostics: Diagnostic[]) {
    const length = this.view.state.doc.length;
    const found = diagnostics.flatMap((d) => {
      if (d.from === null || d.to === null) return [];
      const from = Math.min(d.from, length);
      return [
        {
          from,
          to: Math.max(from, Math.min(d.to, length)),
          severity: d.severity,
          message: [d.message, ...d.hints.map((h) => `Hint: ${h}`)].join("\n"),
        },
      ];
    });
    found.sort((a, b) => a.from - b.from);
    this.view.dispatch(setDiagnostics(this.view.state, found));
  }

  jumpTo(offset: number) {
    const anchor = Math.min(offset, this.view.state.doc.length);
    this.view.dispatch({
      selection: { anchor },
      effects: EditorView.scrollIntoView(anchor, { y: "center" }),
    });
    this.view.focus();
  }
}
