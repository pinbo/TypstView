// Typed wrappers around the Rust commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";

export interface Diagnostic {
  severity: "error" | "warning";
  message: string;
  hints: string[];
  /** Set when the problem is in a file other than the open document. */
  file: string | null;
  line: number | null;
  column: number | null;
  from: number | null;
  to: number | null;
}

export interface PageOutput {
  hash: string;
  svg: string | null;
}

export interface CompileOutput {
  pages: PageOutput[] | null;
  diagnostics: Diagnostic[];
}

export interface Completions {
  from: number;
  items: {
    kind: string;
    label: string;
    apply: string | null;
    detail: string | null;
    symbol: string | null;
  }[];
}

export interface SymbolEntry {
  name: string;
  value: string;
}

export interface Extra {
  name: string;
  value: string;
}

export interface Cell {
  text: string;
  code: boolean;
}

export interface Image {
  path: string;
  width: string;
  extra: Extra[];
}

// Mirrors `Construct` in src-tauri/src/constructs.rs.
export type Construct =
  | { type: "equation"; body: string; block: boolean; label: string }
  | {
      type: "table";
      grid: boolean;
      columns: string[];
      header: Cell[][];
      rows: Cell[][];
      extra: Extra[];
    }
  | {
      type: "figure";
      figure: boolean;
      image: Image | null;
      content: string;
      caption: string;
      label: string;
      extra: Extra[];
    }
  | { type: "link"; target: string; code: boolean; text: string }
  | { type: "quote"; body: string; attribution: string; block: boolean; extra: Extra[] }
  | { type: "code"; lang: string; text: string; block: boolean }
  | { type: "footnote"; body: string; extra: Extra[] }
  | { type: "outline"; title: string; depth: string; extra: Extra[] }
  | {
      type: "bibliography";
      sources: string[];
      title: string;
      style: string;
      full: boolean;
      extra: Extra[];
    };

export type ConstructType = Construct["type"];
export type ConstructOf<T extends ConstructType> = Extract<Construct, { type: T }>;

export interface Detection {
  from: number;
  to: number;
  name: string;
  inCode: boolean;
  indent: string;
  construct: Construct | null;
  problem: string | null;
}

export interface BibEntry {
  key: string;
  kind: string;
  title: string;
  author: string;
  year: string;
}

export interface NewEntry {
  kind: string;
  key: string;
  fields: [string, string][];
}

export type Mode = "markup" | "math" | "code";

export const backend = {
  complete: (path: string | null, text: string, offset: number, explicit: boolean) =>
    invoke<Completions | null>("complete", { path, text, offset, explicit }),
  hover: (path: string | null, text: string, offset: number, before: boolean) =>
    invoke<{ text: string; code: boolean } | null>("hover", { path, text, offset, before }),
  symbols: () => invoke<SymbolEntry[]>("symbols"),
  modeAt: (text: string, offset: number) => invoke<Mode>("mode_at", { text, offset }),
  renderSnippet: (path: string | null, document: string, snippet: string) =>
    invoke<string>("render_snippet", { path, document, snippet }),
  detectConstruct: (text: string, offset: number) =>
    invoke<Detection | null>("detect_construct", { text, offset }),
  renderConstruct: (construct: Construct, inCode: boolean, indent: string) =>
    invoke<string>("render_construct", { construct, inCode, indent }),
  markdownToTypst: (markdown: string) => invoke<string>("markdown_to_typst", { markdown }),
  bibliographyEntries: (document: string, file: string) =>
    invoke<BibEntry[]>("bibliography_entries", { document, file }),
  addBibliographyEntry: (document: string, file: string, entry: NewEntry) =>
    invoke<void>("add_bibliography_entry", { document, file, entry }),
  relativePath: (document: string, target: string) =>
    invoke<string>("document_relative_path", { document, target }),
  compile: (path: string | null, text: string, known: string[]) =>
    invoke<CompileOutput>("compile", { path, text, known }),
  exportPdf: (path: string | null, text: string, target: string) =>
    invoke<Diagnostic[]>("export_pdf", { path, text, target }),
  highlight: (text: string) => invoke<number[]>("highlight", { text }),
  highlightClasses: () => invoke<string[]>("highlight_classes"),
  readFile: (path: string) => invoke<string>("read_file", { path }),
  writeFile: (path: string, text: string) => invoke<void>("write_file", { path, text }),
  startupFile: () => invoke<string | null>("startup_file"),
};
