// Formatting commands. Each is a pure function from editor state to a change,
// so it can be tested without a browser.
import { type ChangeSpec, EditorSelection, EditorState, type TransactionSpec } from "@codemirror/state";

const WORD = /[\p{L}\p{N}]/u;

export interface Style {
  /** Short delimiter such as `*`, if Typst has one. */
  mark?: string;
  /** Function form, such as `strong` for `#strong[..]`. */
  call?: string;
}

export const STYLES = {
  bold: { mark: "*", call: "strong" },
  italic: { mark: "_", call: "emph" },
  underline: { call: "underline" },
  strike: { call: "strike" },
  highlight: { call: "highlight" },
  subscript: { call: "sub" },
  superscript: { call: "super" },
  "inline-code": { mark: "`" },
} satisfies Record<string, Style>;

export type StyleName = keyof typeof STYLES;

/** Add the style around each selection, or remove it if it is already there. */
export function toggleStyle(state: EditorState, name: StyleName): TransactionSpec {
  const { mark, call }: Style = STYLES[name];
  const wrappers: [string, string][] = [];
  if (mark) wrappers.push([mark, mark]);
  if (call) wrappers.push([`#${call}[`, "]"]);

  return state.changeByRange((range) => {
    const { from, to } = range;
    const text = state.sliceDoc(from, to);
    for (const [open, close] of wrappers) {
      // The delimiters just outside the selection...
      if (
        from >= open.length &&
        state.sliceDoc(from - open.length, from) === open &&
        state.sliceDoc(to, to + close.length) === close
      ) {
        return {
          changes: [
            { from: from - open.length, to: from },
            { from: to, to: to + close.length },
          ],
          range: EditorSelection.range(from - open.length, to - open.length),
        };
      }
      // ...or selected along with the text.
      if (
        text.length >= open.length + close.length &&
        text.startsWith(open) &&
        text.endsWith(close)
      ) {
        const inner = text.slice(open.length, text.length - close.length);
        return {
          changes: { from, to, insert: inner },
          range: EditorSelection.range(from, from + inner.length),
        };
      }
    }

    // Leave surrounding spaces outside the delimiters.
    const lead = text.length - text.trimStart().length;
    const trail = text.trim() ? text.length - text.trimEnd().length : 0;
    const start = from + (text.trim() ? lead : 0);
    const end = to - trail;

    // Typst ignores `*` and `_` between letters, so use the function there.
    const before = state.sliceDoc(Math.max(0, start - 1), start);
    const after = state.sliceDoc(end, end + 1);
    const intraword = mark !== "`" && (WORD.test(before) || WORD.test(after));
    const [open, close] = (intraword && call ? wrappers[wrappers.length - 1] : wrappers[0]) as [
      string,
      string,
    ];
    return {
      changes: [
        { from: start, insert: open },
        { from: end, insert: close },
      ],
      range: EditorSelection.range(start + open.length, end + open.length),
    };
  });
}

/** The lines touched by any selection, each once. */
function selectedLines(state: EditorState) {
  const lines = new Map<number, ReturnType<EditorState["doc"]["line"]>>();
  for (const { from, to } of state.selection.ranges) {
    const first = state.doc.lineAt(from).number;
    // A selection ending at the very start of a line does not include it.
    const end = to > from && state.doc.lineAt(to).from === to ? to - 1 : to;
    for (let n = first; n <= state.doc.lineAt(end).number; n++) {
      lines.set(n, state.doc.line(n));
    }
  }
  return [...lines.values()];
}

const LIST_MARKER = /^(\s*)(?:[-+]\s+|\d+\.\s+|\/\s+)/;

export type ListKind = "bullet" | "numbered" | "term";
const MARKERS: Record<ListKind, { insert: string; matches: RegExp }> = {
  bullet: { insert: "- ", matches: /^\s*-\s/ },
  numbered: { insert: "+ ", matches: /^\s*(\+|\d+\.)\s/ },
  term: { insert: "/ ", matches: /^\s*\/\s/ },
};

/** Make the selected lines list items, or plain lines if they all are. */
export function toggleList(state: EditorState, kind: ListKind): TransactionSpec {
  const { insert, matches } = MARKERS[kind];
  const all = selectedLines(state);
  const lines = all.length > 1 ? all.filter((l) => l.text.trim()) : all;
  const remove = lines.every((l) => matches.test(l.text));
  const changes: ChangeSpec[] = lines.map((line) => {
    const old = LIST_MARKER.exec(line.text);
    const indent = old?.[1] ?? /^\s*/.exec(line.text)![0];
    const length = old ? old[0].length : indent.length;
    // A term list item reads `/ term: description`.
    const colon = kind === "term" && !remove && !line.text.includes(":") ? ": " : "";
    const added: ChangeSpec[] = [
      { from: line.from, to: line.from + length, insert: indent + (remove ? "" : insert) },
    ];
    if (colon) added.push({ from: line.to, insert: colon });
    return added;
  });
  return withCursorAfterInserts(state, changes);
}

/** Set the heading level of the selected lines; 0 makes them plain text. */
export function setHeading(state: EditorState, level: number): TransactionSpec {
  const changes = selectedLines(state).map((line) => {
    const old = /^=+(\s+|$)/.exec(line.text)?.[0].length ?? 0;
    return { from: line.from, to: line.from + old, insert: level ? `${"=".repeat(level)} ` : "" };
  });
  return withCursorAfterInserts(state, changes);
}

/** Apply changes, keeping a cursor that sits at an insertion point after it. */
function withCursorAfterInserts(state: EditorState, specs: ChangeSpec[]): TransactionSpec {
  const changes = state.changes(specs);
  return { changes, selection: state.selection.map(changes, 1) };
}

/** Wrap whole lines in a raw block or a block quote. */
export function wrapBlock(state: EditorState, kind: "code-block" | "quote-block"): TransactionSpec {
  const { from, to } = state.selection.main;
  const first = state.doc.lineAt(from);
  const last = state.doc.lineAt(to > from && state.doc.lineAt(to).from === to ? to - 1 : to);
  const text = state.sliceDoc(first.from, last.to);
  let open: string, close: string;
  if (kind === "code-block") {
    const longest = Math.max(0, ...[...text.matchAll(/`+/g)].map((m) => m[0].length));
    open = "`".repeat(Math.max(3, longest + 1));
    close = open;
  } else {
    open = "#quote(block: true)[";
    close = "]";
  }
  return {
    changes: [
      { from: first.from, insert: `${open}\n` },
      { from: last.to, insert: `\n${close}` },
    ],
    selection: EditorSelection.range(
      first.from + open.length + 1,
      last.to + open.length + 1,
    ),
  };
}

/** Make literal text safe to place in markup. */
export function escapeMarkup(text: string): string {
  return text
    .replace(/[\\#$*_`<@[\]~]/g, "\\$&")
    .replace(/\/(?=[/*])/g, "\\/")
    .replace(/^(\s*)([=\-+/])(?=\s|$)/, "$1\\$2")
    .replace(/^(\s*\d+)\.(?=\s|$)/, "$1\\.");
}

/** Text that cites `key`, given the characters around the insertion point. */
export function citation(key: string, before: string, after: string): string {
  const plain = /^[\p{L}\p{N}_-]+(?:[.:][\p{L}\p{N}_-]+)*$/u.test(key);
  const lead = WORD.test(before) ? " " : "";
  if (!plain) return `${lead}#cite(label(${JSON.stringify(key)}))`;
  // A following letter would become part of the key, and `[` a supplement.
  const trail = /[\p{L}\p{N}_\-[]/u.test(after) ? " " : "";
  return `${lead}@${key}${trail}`;
}

/** How to write a symbol where the cursor is. */
export function symbolText(name: string, mode: string, before: string, after: string): string {
  if (mode === "math") {
    const lead = WORD.test(before) ? " " : "";
    const trail = WORD.test(after) ? " " : "";
    return `${lead}${name}${trail}`;
  }
  if (mode === "code") return `sym.${name}`;
  // `;` stops following text from reading as part of the name.
  const joins = /^(?:[\p{L}\p{N}_([]|\.\p{L})/u.test(after);
  return `#sym.${name}${joins ? ";" : ""}`;
}
