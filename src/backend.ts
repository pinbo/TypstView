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

export const backend = {
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
