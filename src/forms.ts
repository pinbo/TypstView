// One form per construct. A form shows a construct and reads it back; the
// Rust side turns the result into source and checks that it is well formed.
import type { Cell, Construct, ConstructOf, ConstructType, Extra } from "./backend";
import { backend } from "./backend";
import { button, checkbox, el, field, input, select, textArea } from "./dialog";
import { escapeMarkup } from "./format";
import { coalesce } from "./schedule";

export interface FormContext {
  documentPath: () => string | null;
  documentText: () => string;
  /** Ask for an image file; resolves to the path to write, or `null`. */
  chooseImage: () => Promise<string | null>;
  manageBibliography: (file: string) => void;
  showError: (text: string) => void;
}

export interface Form<C extends Construct = Construct> {
  body: Node[];
  read: () => C;
  dispose?: () => void;
  wide?: boolean;
}

/** Arguments the form has no field for; they are written back unchanged. */
function kept(extra: Extra[]): Node[] {
  if (!extra.length) return [];
  const list = extra.map((e) => `${e.name}: ${e.value}`).join(", ");
  return [el("p", { className: "kept", textContent: `Kept as written: ${list}` })];
}

// ---------------------------------------------------------------- Equation

/** `[button label, text to insert with `|` marking the cursor]` */
const PALETTE: [string, [string, string][]][] = [
  [
    "Structures",
    [
      ["a/b", "frac(|, )"],
      ["√", "sqrt(|)"],
      ["ⁿ√", "root(3, |)"],
      ["xⁿ", "^(|)"],
      ["xₙ", "_(|)"],
      ["Σ", "sum_(i = 1)^n |"],
      ["Π", "product_(i = 1)^n |"],
      ["∫", "integral_a^b | dif x"],
      ["lim", "lim_(x -> oo) |"],
      ["matrix", "mat(|, ; , )"],
      ["vector", "vec(|, )"],
      ["cases", 'cases(| "if" , )'],
      ["(…)", "lr((|))"],
      ["|x|", "abs(|)"],
      ["‖x‖", "norm(|)"],
      ["x̂", "hat(|)"],
      ["x‾", "overline(|)"],
      ["x⃗", "arrow(|)"],
      ["bold", "bold(|)"],
      ["text", '"|"'],
      ["&=", "&= |"],
      ["new line", "\\\n|"],
    ],
  ],
  [
    "Greek",
    "α:alpha β:beta γ:gamma δ:delta ε:epsilon θ:theta λ:lambda μ:mu π:pi ρ:rho σ:sigma τ:tau φ:phi χ:chi ψ:psi ω:omega Γ:Gamma Δ:Delta Θ:Theta Λ:Lambda Π:Pi Σ:Sigma Φ:Phi Ω:Omega"
      .split(" ")
      .map((pair) => pair.split(":") as [string, string]),
  ],
  [
    "Operators",
    "±:plus.minus ×:times ⋅:dot.op ÷:div ≤:<= ≥:>= ≠:!= ≈:approx ≡:equiv ∝:prop ∈:in ∉:in.not ⊂:subset ⊆:subset.eq ∪:union ∩:inter ∀:forall ∃:exists ∇:nabla ∂:partial ∞:oo →:-> ⇒:=> ⇔:<=> ↦:|-> …:dots.h"
      .split(" ")
      .map((pair) => {
        const colon = pair.indexOf(":");
        return [pair.slice(0, colon), pair.slice(colon + 1)] as [string, string];
      }),
  ],
];

function insertIntoTextArea(area: HTMLTextAreaElement, template: string, plain: boolean) {
  const { selectionStart: from, selectionEnd: to, value } = area;
  const selected = value.slice(from, to);
  const marker = plain ? -1 : template.indexOf("|");
  let before = marker < 0 ? template : template.slice(0, marker);
  let after = marker < 0 ? "" : template.slice(marker + 1);
  // Keep names from running into neighbouring letters.
  if (/^\p{L}/u.test(before) && /[\p{L}\p{N}]$/u.test(value.slice(0, from))) before = ` ${before}`;
  if (/\p{L}$/u.test(before + selected + after) && /^[\p{L}\p{N}]/u.test(value.slice(to))) {
    after += " ";
  }
  area.setRangeText(before + selected + after, from, to, "end");
  if (marker >= 0) {
    const cursor = from + before.length + selected.length;
    area.setSelectionRange(cursor, cursor);
  }
  area.focus();
  area.dispatchEvent(new Event("input"));
}

function equationForm(c: ConstructOf<"equation">, context: FormContext): Form {
  const body = textArea(c.body, 4);
  const block = checkbox("On its own line (display style)", c.block);
  const label = input(c.label, "eq:energy");
  const image = el("img", { alt: "Equation preview" });
  const problem = el("p", { className: "dialog-error" });
  const preview = el("div", { className: "math-preview" }, [image, problem]);
  let url = "";
  let disposed = false;

  const update = coalesce(async () => {
    const source = body.value.trim();
    const snippet = block.box.checked ? `$ ${source} $` : `$${source}$`;
    try {
      const svg = source
        ? await backend.renderSnippet(context.documentPath(), context.documentText(), snippet)
        : null;
      if (disposed) return;
      URL.revokeObjectURL(url);
      url = svg ? URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" })) : "";
      image.hidden = !svg;
      if (svg) image.src = url;
      problem.textContent = "";
    } catch (error) {
      // Keep showing the last good preview while the input is incomplete.
      problem.textContent = String(error);
    }
  }, 150);
  body.addEventListener("input", update);
  block.box.addEventListener("change", update);
  image.hidden = true;
  update();

  const palette = PALETTE.map(([title, items], group) =>
    el("details", { className: "palette", open: group === 0 }, [
      el("summary", { textContent: title }),
      el(
        "div",
        { className: "palette-items" },
        items.map(([text, template]) =>
          // Operators such as `|->` contain `|`, so only structures use it as a marker.
          button(text, () => insertIntoTextArea(body, template, group > 0), template),
        ),
      ),
    ]),
  );

  return {
    wide: true,
    body: [field("Equation (Typst math)", body), preview, ...palette, block.row, field("Label", label)],
    read: () => ({ type: "equation", body: body.value, block: block.box.checked, label: label.value }),
    dispose: () => {
      disposed = true;
      URL.revokeObjectURL(url);
    },
  };
}

// ---------------------------------------------------------------- Table

const emptyCell = (): Cell => ({ text: "", code: false });

function tableForm(c: ConstructOf<"table">): Form {
  const columns = [...c.columns];
  const rows: Cell[][] = [...c.header, ...c.rows].map((r) => r.map((cell) => ({ ...cell })));
  const headerRows = input(String(c.header.length));
  Object.assign(headerRows, { type: "number", min: "0" });
  const grid = el("table", { className: "grid-editor" });
  /** The row and column last worked in; -1 is the width row. */
  let at = { row: 0, column: 0 };

  function draw(focus = false) {
    const headers = Number(headerRows.value) || 0;
    const widths = el(
      "tr",
      { className: "widths" },
      columns.map((width, column) => {
        const box = input(width, "auto");
        box.title = "Column width: auto, 1fr, 3cm, 40% …";
        box.setAttribute("aria-label", `Width of column ${column + 1}`);
        box.oninput = () => (columns[column] = box.value);
        box.onfocus = () => (at = { row: at.row, column });
        return el("th", {}, [box]);
      }),
    );
    const body = rows.map((cells, row) =>
      el(
        "tr",
        { className: row < headers ? "header-row" : "" },
        cells.map((cell, column) => {
          const box = textArea(cell.text, Math.max(1, cell.text.split("\n").length), cell.code);
          box.dataset.cell = `${row},${column}`;
          box.setAttribute("aria-label", `Row ${row + 1}, column ${column + 1}`);
          if (cell.code) box.title = "This cell is a Typst expression, not text.";
          box.oninput = () => (cell.text = box.value);
          box.onfocus = () => (at = { row, column });
          box.onpaste = (event) => paste(event, row, column);
          return el("td", {}, [box]);
        }),
      ),
    );
    grid.replaceChildren(widths, ...body);
    if (focus) {
      grid.querySelector<HTMLElement>(`[data-cell="${at.row},${at.column}"]`)?.focus();
    }
  }

  const newRow = () => columns.map(emptyCell);

  function addColumn(index: number) {
    columns.splice(index, 0, "auto");
    for (const row of rows) row.splice(index, 0, emptyCell());
  }

  /** Cells copied from a spreadsheet arrive separated by tabs. */
  function paste(event: ClipboardEvent, row: number, column: number) {
    const text = event.clipboardData?.getData("text/plain") ?? "";
    if (!text.includes("\t")) return;
    event.preventDefault();
    const data = text.replace(/\r\n?/g, "\n").replace(/\n$/, "").split("\n");
    data.forEach((line, r) => {
      line.split("\t").forEach((value, k) => {
        while (rows.length <= row + r) rows.push(newRow());
        while (columns.length <= column + k) addColumn(columns.length);
        rows[row + r][column + k] = { text: escapeMarkup(value), code: false };
      });
    });
    draw(true);
  }

  const tools = el("div", { className: "button-row" }, [
    button("Add row", () => {
      at.row = Math.min(rows.length, at.row + 1);
      rows.splice(at.row, 0, newRow());
      draw(true);
    }),
    button("Delete row", () => {
      if (rows.length <= 1) return;
      rows.splice(at.row, 1);
      at.row = Math.min(at.row, rows.length - 1);
      draw(true);
    }),
    button("Add column", () => {
      at.column = Math.min(columns.length, at.column + 1);
      addColumn(at.column);
      draw(true);
    }),
    button("Delete column", () => {
      if (columns.length <= 1) return;
      columns.splice(at.column, 1);
      for (const row of rows) row.splice(at.column, 1);
      at.column = Math.min(at.column, columns.length - 1);
      draw(true);
    }),
  ]);
  headerRows.oninput = () => draw();
  draw();

  return {
    wide: true,
    body: [
      field("Header rows", headerRows),
      tools,
      el("div", { className: "grid-scroll" }, [grid]),
      el("p", {
        className: "hint",
        textContent:
          "Cells hold Typst markup. The top row sets column widths. Paste cells copied from a spreadsheet into any cell.",
      }),
      ...kept(c.extra),
    ],
    read: () => {
      const headers = Math.min(rows.length, Math.max(0, Math.floor(Number(headerRows.value) || 0)));
      return {
        type: "table",
        grid: c.grid,
        columns,
        header: rows.slice(0, headers),
        rows: rows.slice(headers),
        extra: c.extra,
      };
    },
  };
}

// ---------------------------------------------------------------- Figure and image

function figureForm(c: ConstructOf<"figure">, context: FormContext, isNew: boolean): Form {
  const figure = checkbox("Wrap in a figure (adds numbering, caption and label)", c.figure);
  const kind = select(
    [
      ["image", "An image"],
      ["content", "A table or other content"],
    ],
    c.image ? "image" : "content",
  );
  const path = input(c.image?.path ?? "", "images/plot.png");
  const width = input(c.image?.width ?? "", "80%");
  const content = textArea(c.content, 6);
  const caption = textArea(c.caption, 2, false);
  const label = input(c.label, "fig:plot");

  const choose = button("Choose…", async () => {
    try {
      const chosen = await context.chooseImage();
      if (chosen !== null) path.value = chosen;
    } catch (error) {
      context.showError(String(error));
    }
  });
  const imageFields = el("div", {}, [
    field("Image file, relative to the document", el("span", { className: "with-button" }, [path, choose])),
    field("Width", width, "Leave empty for the natural size."),
  ]);
  const contentField = field("Content (Typst code)", content, "For example table(columns: 2, [a], [b])");
  const figureFields = el("div", {}, [field("Caption", caption), field("Label", label)]);

  const sync = () => {
    const isImage = kind.value === "image" || !figure.box.checked;
    imageFields.hidden = !isImage;
    contentField.hidden = isImage;
    figureFields.hidden = !figure.box.checked;
    kind.disabled = !figure.box.checked;
  };
  kind.onchange = sync;
  figure.box.onchange = sync;
  sync();

  return {
    body: [
      figure.row,
      ...(isNew ? [field("Body", kind)] : []),
      imageFields,
      contentField,
      figureFields,
      ...kept([...c.extra, ...(c.image?.extra ?? [])]),
    ],
    read: () => {
      const isImage = !imageFields.hidden;
      return {
        type: "figure",
        figure: figure.box.checked,
        image: isImage
          ? { path: path.value, width: width.value, extra: c.image?.extra ?? [] }
          : null,
        content: isImage ? "" : content.value,
        caption: caption.value,
        label: label.value,
        extra: c.extra,
      };
    },
  };
}

// ---------------------------------------------------------------- Smaller forms

function linkForm(c: ConstructOf<"link">): Form {
  const target = input(c.target, "https://typst.app");
  const code = checkbox("The destination is Typst code, such as <intro>", c.code);
  const text = input(c.text, "Optional; the address is shown without it");
  return {
    body: [field("Destination", target), code.row, field("Text", text)],
    read: () => ({ type: "link", target: target.value, code: code.box.checked, text: text.value }),
  };
}

function quoteForm(c: ConstructOf<"quote">): Form {
  const body = textArea(c.body, 4, false);
  const attribution = input(c.attribution);
  const block = checkbox("Block quote", c.block);
  return {
    body: [field("Quote", body), field("Attribution", attribution), block.row, ...kept(c.extra)],
    read: () => ({
      type: "quote",
      body: body.value,
      attribution: attribution.value,
      block: block.box.checked,
      extra: c.extra,
    }),
  };
}

const LANGUAGES =
  "bash c cpp css go html java javascript json julia latex markdown python r ruby rust sql swift toml typ typescript xml yaml";

function codeForm(c: ConstructOf<"code">): Form {
  const lang = input(c.lang, "python");
  const list = el(
    "datalist",
    { id: "code-languages" },
    LANGUAGES.split(" ").map((value) => el("option", { value })),
  );
  lang.setAttribute("list", list.id);
  const text = textArea(c.text, 10);
  const block = checkbox("Block", c.block);
  return {
    wide: true,
    body: [field("Language", lang), list, field("Code", text), block.row],
    read: () => ({ type: "code", lang: lang.value.trim(), text: text.value, block: block.box.checked }),
  };
}

function footnoteForm(c: ConstructOf<"footnote">): Form {
  const body = textArea(c.body, 4, false);
  return {
    body: [field("Footnote text", body), ...kept(c.extra)],
    read: () => ({ type: "footnote", body: body.value, extra: c.extra }),
  };
}

function outlineForm(c: ConstructOf<"outline">): Form {
  const title = input(c.title, "Default title");
  const depth = input(c.depth, "All levels");
  return {
    body: [
      field("Title", title),
      field("Depth", depth, "The deepest heading level to list, such as 2."),
      ...kept(c.extra),
    ],
    read: () => ({ type: "outline", title: title.value, depth: depth.value, extra: c.extra }),
  };
}

const CITATION_STYLES =
  "alphanumeric american-physics-society apa chicago-author-date chicago-notes harvard-cite-them-right ieee mla nature vancouver";

function bibliographyForm(c: ConstructOf<"bibliography">, context: FormContext): Form {
  const sources = input(c.sources.join(", "), "refs.bib");
  const title = input(c.title, "Default title");
  const style = input(c.style, "ieee");
  const list = el(
    "datalist",
    { id: "citation-styles" },
    CITATION_STYLES.split(" ").map((value) => el("option", { value })),
  );
  style.setAttribute("list", list.id);
  const full = checkbox("List every entry, including uncited ones", c.full);
  const files = () =>
    sources.value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
  return {
    body: [
      field("Files (.bib or .yml), separated by commas", sources),
      button("Manage entries in the first file…", () =>
        context.manageBibliography(files()[0] ?? "refs.bib"),
      ),
      field("Title", title),
      field("Citation style", style),
      list,
      full.row,
      ...kept(c.extra),
    ],
    read: () => ({
      type: "bibliography",
      sources: files(),
      title: title.value,
      style: style.value.trim(),
      full: full.box.checked,
      extra: c.extra,
    }),
  };
}

export function formFor(construct: Construct, context: FormContext, isNew: boolean): Form {
  switch (construct.type) {
    case "equation":
      return equationForm(construct, context);
    case "table":
      return tableForm(construct);
    case "figure":
      return figureForm(construct, context, isNew);
    case "link":
      return linkForm(construct);
    case "quote":
      return quoteForm(construct);
    case "code":
      return codeForm(construct);
    case "footnote":
      return footnoteForm(construct);
    case "outline":
      return outlineForm(construct);
    case "bibliography":
      return bibliographyForm(construct, context);
  }
}

export const TITLES: Record<ConstructType, string> = {
  equation: "Equation",
  table: "Table",
  figure: "Figure or image",
  link: "Link",
  quote: "Quote",
  code: "Code",
  footnote: "Footnote",
  outline: "Outline",
  bibliography: "Bibliography",
};

/** A new construct, filled in from the selected text. */
export function blank(type: ConstructType, selection: string): Construct {
  const cell = (text = ""): Cell => ({ text, code: false });
  switch (type) {
    case "equation":
      return { type, body: selection, block: true, label: "" };
    case "table":
      return {
        type,
        grid: false,
        columns: ["auto", "auto", "auto"],
        header: [[cell(), cell(), cell()]],
        rows: [
          [cell(selection), cell(), cell()],
          [cell(), cell(), cell()],
        ],
        extra: [],
      };
    case "figure":
      return {
        type,
        figure: true,
        image: { path: "", width: "80%", extra: [] },
        content: "",
        caption: selection,
        label: "",
        extra: [],
      };
    case "link": {
      const url = /^[a-z][a-z0-9+.-]*:\S+$/i.test(selection);
      return { type, target: url ? selection : "", code: false, text: url ? "" : selection };
    }
    case "quote":
      return { type, body: selection, attribution: "", block: true, extra: [] };
    case "code":
      return { type, lang: "", text: selection, block: true };
    case "footnote":
      return { type, body: selection, extra: [] };
    case "outline":
      return { type, title: "", depth: "", extra: [] };
    case "bibliography":
      return { type, sources: ["refs.bib"], title: "", style: "", full: false, extra: [] };
  }
}
