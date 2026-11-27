// Everything that adds to or rewrites the document: formatting, visual
// editors, snippets, symbols, citations and Markdown pasting.
import { snippet } from "@codemirror/autocomplete";
import { toggleComment } from "@codemirror/commands";
import type { EditorState, TransactionSpec } from "@codemirror/state";
import type { KeyBinding } from "@codemirror/view";
import { open } from "@tauri-apps/plugin-dialog";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { backend, type Construct, type ConstructType, type SymbolEntry } from "./backend";
import { button, el, field, input, openDialog, select, type DialogHandle } from "./dialog";
import type { Editor } from "./editor";
import {
  citation,
  setHeading,
  setTextColor,
  STYLES,
  symbolText,
  toggleList,
  toggleStyle,
  wrapBlock,
  type StyleName,
} from "./format";
import { blank, formFor, TITLES, type FormContext } from "./forms";

export interface Host {
  editor: () => Editor;
  documentPath: () => string | null;
  notify: (text: string, isError?: boolean) => void;
  /** Files used by the document changed on disk. */
  recompile: () => void;
}

/**
 * Snippets are placed on their own line. `${..}` marks fields to fill in. A
 * bare number there would be a tab-stop index, so numeric defaults are written
 * `${1:2}`.
 */
export const SNIPPETS: Record<string, [title: string, template: string]> = {
  "page-break": ["Page break", "#pagebreak()"],
  "page-setup": ["Page setup", '#set page(paper: "${a4}", margin: ${2.5cm})'],
  "text-settings": ["Text settings", '#set text(size: ${11pt}, lang: "${en}")'],
  "paragraph-settings": ["Justified paragraphs", "#set par(justify: ${true})"],
  "heading-numbering": ["Heading numbering", '#set heading(numbering: "${1.1}")'],
  "page-numbers": ["Page numbers", '#set page(numbering: "${1:1}")'],
  columns: ["Columns", "#columns(${1:2})[\n\t${2}\n]"],
  centered: ["Centered block", "#align(center)[\n\t${}\n]"],
  rule: ["Horizontal line", "#line(length: ${100%})"],
  definition: ["Definition", '#let ${name} = "${value}"'],
  date: ["Today's date", "#datetime.today().display()"],
};

const CONSTRUCT_ACTIONS: Record<string, ConstructType> = {
  "insert-equation": "equation",
  "insert-table": "table",
  "insert-figure": "figure",
  "insert-link": "link",
  "insert-footnote": "footnote",
  "insert-quote": "quote",
  "insert-code": "code",
  "insert-outline": "outline",
  "insert-bibliography": "bibliography",
};

/** Typst's named colors and how Typst draws them. */
const TEXT_COLORS: [name: string, css: string][] = [
  ["black", "#000000"],
  ["gray", "#aaaaaa"],
  ["silver", "#dddddd"],
  ["white", "#ffffff"],
  ["navy", "#001f3f"],
  ["blue", "#0074d9"],
  ["aqua", "#7fdbff"],
  ["teal", "#39cccc"],
  ["eastern", "#239dad"],
  ["purple", "#b10dc9"],
  ["fuchsia", "#f012be"],
  ["maroon", "#85144b"],
  ["red", "#ff4136"],
  ["orange", "#ff851b"],
  ["yellow", "#ffdc00"],
  ["olive", "#3d9970"],
  ["green", "#2ecc40"],
  ["lime", "#01ff70"],
];

interface Target {
  from: number;
  to: number;
  inCode: boolean;
  indent: string;
}

function isBlock(c: Construct): boolean {
  if (c.type === "table" || c.type === "figure") return c.type !== "figure" || c.figure;
  if (c.type === "outline" || c.type === "bibliography") return true;
  return (c.type === "equation" || c.type === "code" || c.type === "quote") && c.block;
}

export function insertCommands(host: Host) {
  const view = () => host.editor().view;

  function run(command: (state: EditorState) => TransactionSpec) {
    view().dispatch(command(view().state), { scrollIntoView: true, userEvent: "input.format" });
    view().focus();
  }

  const lineIndent = (position: number) =>
    /^[ \t]*/.exec(view().state.doc.lineAt(position).text)![0];

  /** Line breaks needed to put something on its own line. */
  function ownLine(from: number, to: number, indent: string): [string, string] {
    const { doc } = view().state;
    const start = doc.lineAt(from);
    const end = doc.lineAt(to);
    return [
      doc.sliceString(start.from, from).trim() ? `\n${indent}` : "",
      doc.sliceString(to, end.to).trim() ? `\n${indent}` : "",
    ];
  }

  function replace(from: number, to: number, text: string, cursor = from + text.length) {
    view().dispatch({
      changes: { from, to, insert: text },
      selection: { anchor: cursor },
      scrollIntoView: true,
      userEvent: "input.insert",
    });
  }

  // ------------------------------------------------------------ Visual editors

  function openEditor(construct: Construct, target: Target, isNew: boolean) {
    const original = view().state.sliceDoc(target.from, target.to);
    let dialog: DialogHandle | undefined;
    const context: FormContext = {
      documentPath: host.documentPath,
      documentText: () => host.editor().text,
      chooseImage,
      manageBibliography: (file) => void citations(file, false),
      showError: (text) => dialog?.showError(text),
    };
    const form = formFor(construct, context, isNew);
    dialog = openDialog({
      title: `${isNew ? "Insert" : "Edit"}: ${TITLES[construct.type]}`,
      body: form.body,
      submit: isNew ? "Insert" : "Apply",
      wide: form.wide,
      onClose: () => {
        form.dispose?.();
        view().focus();
      },
      onSubmit: async () => {
        const result = form.read();
        const source = await backend.renderConstruct(result, target.inCode, target.indent);
        if (view().state.sliceDoc(target.from, target.to) !== original) {
          return "The document changed while this window was open. Close it and try again.";
        }
        const [before, after] =
          isNew && !target.inCode && isBlock(result)
            ? ownLine(target.from, target.to, target.indent)
            : ["", ""];
        replace(
          target.from,
          target.to,
          before + source + after,
          target.from + before.length + source.length,
        );
      },
    });
  }

  async function chooseImage(): Promise<string | null> {
    const document = host.documentPath();
    if (!document) throw "Save the document first, so the image path can be written relative to it.";
    const chosen = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "svg", "webp", "pdf"] }],
    });
    return chosen ? backend.relativePath(document, chosen) : null;
  }

  async function detect() {
    const doc = view().state.doc;
    const found = await backend.detectConstruct(doc.toString(), view().state.selection.main.head);
    // Ignore the answer if the text has moved on.
    return view().state.doc === doc ? found : null;
  }

  async function editAtCursor() {
    const found = await detect();
    if (!found) {
      host.notify(
        "Nothing to edit here. Put the cursor in an equation, table, figure, image, link, quote, code, footnote, outline or bibliography.",
      );
    } else if (!found.construct) {
      host.notify(`This ${found.name} can only be edited as text: ${found.problem}.`, true);
    } else {
      openEditor(found.construct, found, false);
    }
  }

  async function insertConstruct(type: ConstructType) {
    const { from, to } = view().state.selection.main;
    const mode = await backend.modeAt(host.editor().text, from);
    if (mode === "math") {
      if (type === "equation") return editAtCursor();
      // Anything else would turn into math; leave that to the user.
      return host.notify("The cursor is inside an equation. Move it outside first.", true);
    }
    const construct = blank(type, view().state.sliceDoc(from, to));
    openEditor(construct, { from, to, inCode: mode === "code", indent: lineIndent(from) }, true);
  }

  function insertSnippet(template: string) {
    const { from, to } = view().state.selection.main;
    const indent = lineIndent(from);
    const [before, after] = ownLine(from, to, indent);
    if (before) replace(from, to, before);
    const start = view().state.selection.main.from;
    const end = before ? start : to;
    if (after) {
      view().dispatch({ changes: { from: end, insert: after } });
    }
    // `snippet` indents following lines to match the first one.
    snippet(template)(view(), null, start, end);
    view().focus();
  }

  // ------------------------------------------------------------ Text color

  let customColor = "#d6336c";

  async function textColor() {
    const { from } = view().state.selection.main;
    const mode = await backend.modeAt(host.editor().text, from);
    if (mode !== "markup") {
      return host.notify(
        mode === "math"
          ? "To color an equation, select all of it, including the $ signs."
          : "The cursor is in code. Select text in the document to color it.",
        true,
      );
    }
    let dialog: DialogHandle;
    const apply = (color: string | null) => {
      dialog.close();
      run((s) => setTextColor(s, color));
    };
    const swatches = TEXT_COLORS.map(([name, value]) => {
      const swatch = button("", () => apply(name), name);
      swatch.style.background = value;
      swatch.setAttribute("aria-label", name);
      return swatch;
    });
    const custom = el("input", { type: "color", value: customColor });
    custom.oninput = () => (customColor = custom.value);
    dialog = openDialog({
      title: "Text color",
      body: [
        el("div", { className: "swatches" }, swatches),
        el("div", { className: "custom-color" }, [
          el("label", {}, ["Custom ", custom]),
          button("Use custom color", () => apply(`rgb("${customColor}")`)),
        ]),
        button("Remove color", () => apply(null)),
      ],
      onClose: () => view().focus(),
    });
  }

  // ------------------------------------------------------------ Symbols

  let symbolList: SymbolEntry[] | null = null;

  async function symbols() {
    symbolList ??= await backend.symbols();
    const all = symbolList;
    const { from, to } = view().state.selection.main;
    const mode = await backend.modeAt(host.editor().text, from);
    const search = input("", "Search by name, such as arrow or alpha");
    const results = el("div", { className: "symbol-grid" });
    const status = el("p", { className: "hint" });
    let dialog: DialogHandle;

    const insert = (entry: SymbolEntry) => {
      const { doc } = view().state;
      const text = symbolText(
        entry.name,
        mode,
        doc.sliceString(Math.max(0, from - 1), from),
        doc.sliceString(to, to + 2),
      );
      replace(from, to, text);
      dialog.close();
    };
    const show = () => {
      const words = search.value.toLowerCase().split(/[\s.]+/).filter(Boolean);
      const found = all.filter(
        (s) => s.value === search.value || words.every((w) => s.name.toLowerCase().includes(w)),
      );
      const limit = 400;
      results.replaceChildren(
        ...found.slice(0, limit).map((s) => button(s.value, () => insert(s), s.name)),
      );
      status.textContent =
        found.length > limit
          ? `Showing ${limit} of ${found.length}. Type to narrow the list.`
          : `${found.length} symbols`;
    };
    search.oninput = show;
    show();
    dialog = openDialog({
      title: "Symbols",
      body: [search, results, status],
      wide: true,
      onClose: () => view().focus(),
      // Enter inserts the first match.
      onSubmit: async () => {
        results.querySelector("button")?.click();
      },
    });
  }

  // ------------------------------------------------------------ Citations

  async function citations(fileHint?: string, canCite = true) {
    const document = host.documentPath();
    if (!document) {
      return host.notify("Save the document first; bibliography files live next to it.", true);
    }
    const used = /\bbibliography\(\s*\(?\s*"([^"]+)"/.exec(host.editor().text)?.[1];
    const file = input(fileHint ?? used ?? "refs.bib", "refs.bib");
    const filter = input("", "Filter by key, author or title");
    const list = el("ul", { className: "entry-list" });
    let dialog: DialogHandle | undefined;

    const cite = async (key: string) => {
      const { from, to } = view().state.selection.main;
      const text = host.editor().text;
      const mode = await backend.modeAt(text, from);
      const { doc } = view().state;
      let insert = citation(
        key,
        doc.sliceString(Math.max(0, from - 1), from),
        doc.sliceString(to, to + 1),
      );
      if (mode !== "markup") {
        insert = `${mode === "math" ? "#" : ""}cite(label(${JSON.stringify(key)}))`;
      }
      const changes = [{ from, to, insert }];
      // A citation only compiles when the document has a bibliography.
      const missing = !/\bbibliography\(/.test(text);
      if (missing) {
        const addition = `\n\n#bibliography(${JSON.stringify(file.value.trim())})\n`;
        changes.push({ from: doc.length, to: doc.length, insert: addition });
      }
      view().dispatch({ changes, selection: { anchor: from + insert.length }, scrollIntoView: true });
      dialog?.close();
      if (missing) host.notify("Added a bibliography at the end of the document.");
    };

    let entries: Awaited<ReturnType<typeof backend.bibliographyEntries>> = [];
    const draw = () => {
      const needle = filter.value.toLowerCase();
      const shown = entries.filter((e) =>
        `${e.key} ${e.author} ${e.title}`.toLowerCase().includes(needle),
      );
      list.replaceChildren(
        ...shown.map((e) => {
          const details = [e.author, e.year && `(${e.year})`, e.title].filter(Boolean).join(" ");
          const row = el("li", {}, [
            el("span", {}, [el("code", { textContent: e.key }), ` ${details}`]),
          ]);
          if (canCite) row.append(button("Cite", () => void cite(e.key)));
          return row;
        }),
      );
      if (!shown.length) {
        list.append(el("li", { textContent: entries.length ? "No matches." : "No entries yet." }));
      }
    };
    const load = async () => {
      try {
        entries = await backend.bibliographyEntries(document, file.value.trim());
        dialog?.showError("");
      } catch (error) {
        entries = [];
        dialog?.showError(String(error));
      }
      draw();
    };
    file.onchange = () => void load();
    filter.oninput = draw;

    const type = select(
      ["article", "book", "inproceedings", "techreport", "phdthesis", "online", "misc"].map((t) => [
        t,
        t,
      ]),
      "article",
    );
    const names: [string, string, string][] = [
      ["title", "Title", ""],
      ["author", "Authors", "Last, First and Last, First"],
      ["year", "Year", ""],
      ["journal", "Journal", ""],
      ["booktitle", "Book title", "For conference papers"],
      ["publisher", "Publisher", ""],
      ["doi", "DOI", ""],
      ["url", "URL", ""],
    ];
    const key = input("", "knuth1984");
    const values = names.map(([name, label, placeholder]) => {
      const box = input("", placeholder);
      return { name, box, row: field(label, box) };
    });
    const add = button("Add to file", async () => {
      try {
        await backend.addBibliographyEntry(document, file.value.trim(), {
          kind: type.value,
          key: key.value.trim(),
          fields: values.map((v) => [v.name, v.box.value]),
        });
      } catch (error) {
        dialog?.showError(String(error));
        return;
      }
      for (const box of [key, ...values.map((v) => v.box)]) box.value = "";
      await load();
      host.recompile();
    });
    const adding = el("details", {}, [
      el("summary", { textContent: "New entry (.bib files)" }),
      field("Type", type),
      field("Key", key),
      ...values.map((v) => v.row),
      add,
    ]);
    // Enter in these fields adds the entry rather than closing the window.
    adding.onkeydown = (event) => {
      if (event.key === "Enter" && event.target instanceof HTMLInputElement) {
        event.preventDefault();
        add.click();
      }
    };
    const noSubmit = (event: KeyboardEvent) => {
      if (event.key === "Enter") event.preventDefault();
    };
    file.addEventListener("keydown", (event) => {
      noSubmit(event);
      if (event.key === "Enter") void load();
    });
    filter.addEventListener("keydown", noSubmit);

    dialog = openDialog({
      title: "Citations and bibliography",
      body: [field("File, relative to the document", file), filter, list, adding],
      wide: true,
      onClose: () => {
        if (!window.document.querySelector("dialog[open]")) view().focus();
      },
    });
    await load();
  }

  // ------------------------------------------------------------ Markdown

  async function pasteMarkdown() {
    let markdown: string;
    try {
      markdown = await readText();
    } catch (error) {
      return host.notify(`Could not read text from the clipboard: ${error}`, true);
    }
    const typst = await backend.markdownToTypst(markdown);
    const { from, to } = view().state.selection.main;
    replace(from, to, typst);
    view().focus();
  }

  async function convertSelection() {
    const { from, to } = view().state.selection.main;
    if (from === to) return host.notify("Select the Markdown to convert first.");
    const original = view().state.sliceDoc(from, to);
    const typst = await backend.markdownToTypst(original);
    if (view().state.sliceDoc(from, to) === original) replace(from, to, typst);
  }

  // ------------------------------------------------------------ Actions

  const report = (task: () => Promise<unknown>) => () => {
    task().catch((error) => host.notify(String(error), true));
  };

  const actions: Record<string, () => void> = {
    "edit-at-cursor": report(editAtCursor),
    citations: report(() => citations()),
    symbols: report(symbols),
    "text-color": report(textColor),
    "paste-markdown": report(pasteMarkdown),
    "convert-markdown": report(convertSelection),
    "bullet-list": () => run((s) => toggleList(s, "bullet")),
    "numbered-list": () => run((s) => toggleList(s, "numbered")),
    "term-list": () => run((s) => toggleList(s, "term")),
    "quote-block": () => run((s) => wrapBlock(s, "quote-block")),
    "code-block": () => run((s) => wrapBlock(s, "code-block")),
    comment: () => {
      toggleComment(view());
      view().focus();
    },
  };
  for (const name of Object.keys(STYLES) as StyleName[]) {
    actions[name] = () => run((s) => toggleStyle(s, name));
  }
  for (const level of [0, 1, 2, 3, 4]) {
    actions[`heading-${level}`] = () => run((s) => setHeading(s, level));
  }
  for (const [action, type] of Object.entries(CONSTRUCT_ACTIONS)) {
    actions[action] = report(() => insertConstruct(type));
  }
  for (const [name, [, template]] of Object.entries(SNIPPETS)) {
    actions[`snippet-${name}`] = () => insertSnippet(template);
  }

  const shortcuts: Record<string, string> = {
    "Mod-b": "bold",
    "Mod-i": "italic",
    "Mod-u": "underline",
    "Mod-Shift-x": "strike",
    "Mod-Alt-0": "heading-0",
    "Mod-Alt-1": "heading-1",
    "Mod-Alt-2": "heading-2",
    "Mod-Alt-3": "heading-3",
    "Mod-Alt-4": "heading-4",
    "Mod-Shift-8": "bullet-list",
    "Mod-Shift-7": "numbered-list",
    "Mod-/": "comment",
    "Mod-k": "insert-link",
    "Mod-Shift-m": "insert-equation",
    "Mod-.": "edit-at-cursor",
    "Mod-Shift-v": "paste-markdown",
  };
  const keymap: KeyBinding[] = Object.entries(shortcuts).map(([key, action]) => ({
    key,
    preventDefault: true,
    run: () => {
      actions[action]();
      return true;
    },
  }));

  return { actions, keymap };
}
