import assert from "node:assert/strict";
import { test } from "node:test";
import { EditorSelection, EditorState, type TransactionSpec } from "@codemirror/state";
import {
  citation,
  escapeMarkup,
  setHeading,
  setTextColor,
  symbolText,
  toggleList,
  toggleStyle,
  wrapBlock,
} from "./format.ts";

/** `|a|` marks a selection from the first bar to the second; one bar is a cursor. */
function state(marked: string): EditorState {
  const from = marked.indexOf("|");
  const rest = marked.slice(0, from) + marked.slice(from + 1);
  const second = rest.indexOf("|");
  const doc = second < 0 ? rest : rest.slice(0, second) + rest.slice(second + 1);
  return EditorState.create({
    doc,
    selection: EditorSelection.range(from, second < 0 ? from : second),
  });
}

function apply(marked: string, command: (s: EditorState) => TransactionSpec): string {
  const next = state(marked).update(command(state(marked))).state;
  const { from, to } = next.selection.main;
  const doc = next.doc.toString();
  return from === to
    ? `${doc.slice(0, from)}|${doc.slice(from)}`
    : `${doc.slice(0, from)}|${doc.slice(from, to)}|${doc.slice(to)}`;
}

test("bold wraps and unwraps", () => {
  const bold = (s: EditorState) => toggleStyle(s, "bold");
  assert.equal(apply("a |big| dog", bold), "a *|big|* dog");
  assert.equal(apply("a *|big|* dog", bold), "a |big| dog");
  assert.equal(apply("a |*big*| dog", bold), "a |big| dog");
  assert.equal(apply("a | dog", bold), "a *|* dog");
});

test("styles leave spaces outside and handle intraword selections", () => {
  assert.equal(apply("a| big |dog", (s) => toggleStyle(s, "italic")), "a _|big|_ dog");
  assert.equal(apply("in|tra|word", (s) => toggleStyle(s, "bold")), "in#strong[|tra|]word");
  assert.equal(apply("in#strong[|tra|]word", (s) => toggleStyle(s, "bold")), "in|tra|word");
  assert.equal(apply("x|2|", (s) => toggleStyle(s, "superscript")), "x#super[|2|]");
});

test("lists toggle and replace each other", () => {
  const bullets = (s: EditorState) => toggleList(s, "bullet");
  assert.equal(apply("|one\n\ntwo|", bullets), "- |one\n\n- two|");
  assert.equal(apply("- |one\n- two|", bullets), "|one\ntwo|");
  assert.equal(apply("  - on|e", (s) => toggleList(s, "numbered")), "  + on|e");
  assert.equal(apply("|", bullets), "- |");
  assert.equal(apply("ter|m", (s) => toggleList(s, "term")), "/ ter|m: ");
});

test("heading levels replace each other", () => {
  assert.equal(apply("Tit|le", (s) => setHeading(s, 2)), "== Tit|le");
  assert.equal(apply("== Tit|le", (s) => setHeading(s, 1)), "= Tit|le");
  assert.equal(apply("== Tit|le", (s) => setHeading(s, 0)), "Tit|le");
  assert.equal(apply("a=b|", (s) => setHeading(s, 0)), "a=b|");
});

test("blocks wrap whole lines with a long enough fence", () => {
  assert.equal(apply("a\nco|de\nb", (s) => wrapBlock(s, "code-block")), "a\n```\n|code|\n```\nb");
  assert.match(apply("|``` x|", (s) => wrapBlock(s, "code-block")), /^````\n/);
});

test("escapes literal text", () => {
  assert.equal(escapeMarkup("$5 #1 [a]"), "\\$5 \\#1 \\[a\\]");
  assert.equal(escapeMarkup("- 3"), "\\- 3");
  assert.equal(escapeMarkup("-3"), "-3");
  assert.equal(escapeMarkup("2. x"), "2\\. x");
  assert.equal(escapeMarkup("a//b"), "a\\//b");
});

test("citations and symbols stay separate from their neighbours", () => {
  assert.equal(citation("knuth84", " ", ""), "@knuth84");
  assert.equal(citation("knuth84", "x", "y"), " @knuth84 ");
  assert.equal(citation("a/b", " ", " "), '#cite(label("a/b"))');
  assert.equal(symbolText("alpha", "math", "x", "y"), " alpha ");
  assert.equal(symbolText("arrow.r", "markup", " ", "x"), "#sym.arrow.r;");
  assert.equal(symbolText("arrow.r", "markup", " ", ". "), "#sym.arrow.r");
  assert.equal(symbolText("pi", "code", "(", ")"), "sym.pi");
});

test("text color wraps, changes and comes off", () => {
  const red = (s: EditorState) => setTextColor(s, "red");
  const none = (s: EditorState) => setTextColor(s, null);
  assert.equal(apply("a| big |dog", red), "a #text(fill: red)[|big|] dog");
  assert.equal(apply("a | dog", red), "a #text(fill: red)[|] dog");
  assert.equal(
    apply('a #text(fill: red)[|big|] dog', (s) => setTextColor(s, 'rgb("#00ff00")')),
    'a #text(fill: rgb("#00ff00"))[|big|] dog',
  );
  assert.equal(apply('a |#text(fill: rgb("#00ff00"))[big]| dog', red), "a #text(fill: red)[|big|] dog");
  assert.equal(apply("a #text(fill: red)[|big|] dog", none), "a |big| dog");
  assert.equal(apply("|big|(1)", red), "#text(fill: red)[|big|];(1)");
  assert.equal(apply("#text(fill: red)[|big|];(1)", none), "|big|(1)");
  assert.equal(apply("a |big| dog", none), "a |big| dog");
  // Calls that set more than the color, and two separate calls, are left whole.
  assert.equal(
    apply("#text(size: 2em, fill: red)[|x|]", none),
    "#text(size: 2em, fill: red)[|x|]",
  );
  const two = "#text(fill: red)[a] #text(fill: blue)[b]";
  assert.equal(apply(`|${two}|`, red), `#text(fill: red)[|${two}|]`);
});
