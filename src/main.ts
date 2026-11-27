// Application wiring: documents, files, compile loop, layout and shortcuts.
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { message, open, save } from "@tauri-apps/plugin-dialog";
import { backend, Diagnostic } from "./backend";
import { Editor } from "./editor";
import { Preview } from "./preview";
import { coalesce } from "./schedule";
import { WELCOME } from "./welcome";

const isMac = navigator.platform.startsWith("Mac");
const appWindow = getCurrentWindow();

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const panes = $("panes");
const problems = $("problems");
const problemList = $("problem-list");
const compileStatus = $("compile-status");

// ---------------------------------------------------------------- Document

interface DocumentState {
  path: string | null;
  /** Editor text as last loaded or saved. */
  savedText: string;
  /** Exact file contents as last loaded or saved, to notice outside edits. */
  diskText: string | null;
  /** The editor works with "\n"; these restore the file's own format. */
  crlf: boolean;
  bom: boolean;
}

function fromDisk(path: string | null, raw: string): DocumentState & { text: string } {
  const bom = raw.startsWith("﻿");
  const body = bom ? raw.slice(1) : raw;
  const text = body.replace(/\r\n?/g, "\n");
  return {
    path,
    text,
    savedText: text,
    diskText: path === null ? null : raw,
    crlf: body.includes("\r\n"),
    bom,
  };
}

function toDisk(text: string, state: DocumentState): string {
  return (state.bom ? "﻿" : "") + (state.crlf ? text.replace(/\n/g, "\r\n") : text);
}

let current: DocumentState = fromDisk(null, "");
/** Bumped whenever another document is loaded, so late results are dropped. */
let generation = 0;

const isDirty = () => editor.text !== current.savedText;
const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;
const displayName = () => (current.path ? baseName(current.path) : "Untitled");

let shownTitle = "";
function updateTitle() {
  const title = `${isDirty() ? "● " : ""}${displayName()}`;
  if (title === shownTitle) return;
  shownTitle = title;
  $("filename").textContent = title;
  $("filename").title = current.path ?? "Not saved yet";
  void appWindow.setTitle(`${title} — typstView`);
}

// ---------------------------------------------------------------- Editor and preview

const settings = {
  get: (key: string) => localStorage.getItem(`setting.${key}`),
  set: (key: string, value: string) => localStorage.setItem(`setting.${key}`, value),
};

let wrap = settings.get("wrap") !== "off";

const editor = new Editor($("editor"), wrap, {
  onChange: () => {
    updateTitle();
    requestHighlight();
    requestCompile();
    requestDraft();
  },
  onCursor: (line, column) => {
    $("cursor-position").textContent = `Ln ${line}, Col ${column}`;
  },
});

const preview = new Preview($("preview"), $("pages"), (label) => {
  $("zoom-label").textContent = label;
});

let highlightClasses: string[] = [];

const requestHighlight = coalesce(async () => {
  const doc = editor.view.state.doc;
  const spans = await backend.highlight(doc.toString());
  // If the text moved on, a newer run is already queued.
  if (editor.view.state.doc === doc) editor.applyHighlights(spans, highlightClasses);
}, 0);

const requestCompile = coalesce(async () => {
  const doc = editor.view.state.doc;
  const started = generation;
  const startTime = performance.now();
  compileStatus.dataset.busy = "true";
  try {
    const output = await backend.compile(current.path, doc.toString(), preview.hashes);
    if (started !== generation) return;
    if (output.pages) preview.render(output.pages);
    preview.setStale(!output.pages);
    // Offsets are only right for the text that was compiled.
    if (editor.view.state.doc === doc) editor.showDiagnostics(output.diagnostics);
    showProblems(output.diagnostics, performance.now() - startTime);
  } catch (error) {
    compileStatus.textContent = "Compiler failed";
    compileStatus.dataset.state = "error";
    notify(String(error), true);
  } finally {
    delete compileStatus.dataset.busy;
  }
}, 120);

// ---------------------------------------------------------------- Problems and status

/** The panel opens by itself for errors unless the user closed it. */
let panelDismissed = false;
let panelPinned = false;

function showProblems(diagnostics: Diagnostic[], elapsed: number) {
  const errors = diagnostics.filter((d) => d.severity === "error").length;
  const warnings = diagnostics.length - errors;
  const count = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

  compileStatus.dataset.state = errors ? "error" : warnings ? "warning" : "ok";
  compileStatus.textContent = errors
    ? `✖ ${count(errors, "error")}${warnings ? `, ${count(warnings, "warning")}` : ""}`
    : warnings
      ? `⚠ ${count(warnings, "warning")}`
      : `✓ Compiled in ${Math.round(elapsed)} ms`;
  $("page-count").textContent = preview.pageCount ? count(preview.pageCount, "page") : "";

  problemList.replaceChildren(
    ...diagnostics.map((d) => {
      const row = document.createElement("li");
      const button = document.createElement("button");
      button.className = d.severity;
      const where = d.line === null ? "" : `${d.line}:${d.column}`;
      const place = d.file ? `${d.file}${where && `:${where}`}` : where && `Line ${where}`;
      for (const [name, text] of [
        ["place", place],
        ["text", d.message],
        ["hint", d.hints.map((h) => `Hint: ${h}`).join("  ")],
      ]) {
        if (!text) continue;
        const span = document.createElement("span");
        span.className = name;
        span.textContent = text;
        button.append(span);
      }
      button.disabled = d.from === null;
      button.onclick = () => {
        if (panes.dataset.view === "preview") setView("split");
        editor.jumpTo(d.from!);
      };
      row.append(button);
      return row;
    }),
  );

  if (!errors) panelDismissed = false;
  problems.hidden = diagnostics.length === 0 || !(panelPinned || (errors > 0 && !panelDismissed));
}

compileStatus.onclick = () => {
  if (!problemList.childElementCount) return;
  const show = problems.hidden !== false;
  problems.hidden = !show;
  panelPinned = show;
  panelDismissed = !show;
};

let messageTimer: number | undefined;
function notify(text: string, isError = false) {
  const target = $("message");
  target.textContent = text;
  target.title = text;
  target.classList.toggle("error", isError);
  clearTimeout(messageTimer);
  messageTimer = window.setTimeout(() => (target.textContent = ""), isError ? 12000 : 4000);
}

async function showError(title: string, error: unknown) {
  await message(String(error), { title, kind: "error" });
}

// ---------------------------------------------------------------- Draft recovery

// Unsaved text is kept in local storage, so a crash, a forced quit or a logout
// does not lose it.
const DRAFT_KEY = "draft";

interface Draft extends DocumentState {
  text: string;
}

/** Set once the user has chosen to close, so a discarded draft stays discarded. */
let closing = false;

function writeDraft() {
  if (closing) return;
  try {
    if (isDirty()) {
      const draft: Draft = { ...current, text: editor.text };
      localStorage.setItem(DRAFT_KEY, JSON.stringify(draft));
    } else {
      localStorage.removeItem(DRAFT_KEY);
    }
  } catch {
    notify("Document too large to keep a recovery copy", true);
  }
}

const requestDraft = coalesce(async () => writeDraft(), 500);

function readDraft(): Draft | null {
  try {
    return JSON.parse(localStorage.getItem(DRAFT_KEY) ?? "null");
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------- File actions

function show(state: DocumentState, text: string) {
  current = state;
  generation++;
  preview.clear();
  editor.load(text);
  problemList.replaceChildren();
  problems.hidden = true;
  $("page-count").textContent = "";
  shownTitle = "";
  updateTitle();
  if (state.path) settings.set("lastFile", state.path);
  writeDraft();
  requestHighlight();
  requestCompile();
}

/** Ask what to do with unsaved changes. `false` means stay on this document. */
async function confirmLeave(): Promise<boolean> {
  if (!isDirty()) return true;
  const buttons = { yes: "Save", no: "Don't Save", cancel: "Cancel" };
  const answer = await message(`Save the changes to ${displayName()}?`, {
    title: "Unsaved changes",
    kind: "warning",
    buttons,
  });
  if (answer === buttons.yes || answer === "Yes") return saveDocument();
  return answer === buttons.no || answer === "No";
}

const TYPST_FILTERS = [
  { name: "Typst", extensions: ["typ"] },
  { name: "Text files", extensions: ["typ", "txt", "md", "bib", "yml", "yaml", "toml", "json", "csv"] },
  { name: "All files", extensions: ["*"] },
];

async function openPath(path: string) {
  try {
    const { text, ...state } = fromDisk(path, await backend.readFile(path));
    show(state, text);
  } catch (error) {
    await showError("Could not open the file", error);
  }
}

async function newDocument() {
  if (!(await confirmLeave())) return;
  const { text, ...state } = fromDisk(null, "");
  show(state, text);
}

async function openDocument(path?: string) {
  if (!(await confirmLeave())) return;
  const chosen = path ?? (await open({ filters: TYPST_FILTERS, multiple: false, directory: false }));
  if (chosen) await openPath(chosen);
}

async function saveDocument(): Promise<boolean> {
  if (!current.path) return saveDocumentAs();
  if (current.diskText !== null) {
    // A missing file is fine: saving recreates it.
    const onDisk = await backend.readFile(current.path).catch(() => null);
    if (onDisk !== null && onDisk !== current.diskText) {
      const buttons = { ok: "Overwrite", cancel: "Cancel" };
      const answer = await message(
        `${displayName()} was changed by another program. Overwrite it with your version?`,
        { title: "File changed on disk", kind: "warning", buttons },
      );
      if (answer !== buttons.ok && answer !== "Ok") return false;
    }
  }
  return writeTo(current.path);
}

async function saveDocumentAs(): Promise<boolean> {
  const path = await save({
    filters: TYPST_FILTERS,
    defaultPath: current.path ?? "untitled.typ",
  });
  return path ? writeTo(path) : false;
}

async function writeTo(path: string): Promise<boolean> {
  const state = current;
  const text = editor.text;
  const raw = toDisk(text, state);
  try {
    await backend.writeFile(path, raw);
  } catch (error) {
    await showError("Could not save the file", error);
    return false;
  }
  if (current !== state) return true;
  const moved = state.path !== path;
  Object.assign(state, { path, savedText: text, diskText: raw });
  settings.set("lastFile", path);
  shownTitle = "";
  updateTitle();
  writeDraft();
  notify(`Saved ${baseName(path)}`);
  if (moved) {
    // Relative includes and images now resolve against the new folder.
    generation++;
    requestCompile();
  }
  return true;
}

async function exportPdf() {
  const stem = current.path ? current.path.replace(/\.[^.\\/]*$/, "") : "untitled";
  const target = await save({
    filters: [{ name: "PDF", extensions: ["pdf"] }],
    defaultPath: `${stem}.pdf`,
  });
  if (!target) return;
  notify("Exporting PDF…");
  try {
    const failures = await backend.exportPdf(current.path, editor.text, target);
    if (failures.length) {
      await showError("PDF export failed", failures.map((d) => d.message).join("\n"));
      notify("PDF not exported", true);
    } else {
      notify(`Exported ${baseName(target)}`);
    }
  } catch (error) {
    await showError("PDF export failed", error);
  }
}

// ---------------------------------------------------------------- Layout

type View = "editor" | "split" | "preview";

function setView(view: View) {
  panes.dataset.view = view;
  settings.set("view", view);
  for (const button of document.querySelectorAll<HTMLElement>('[data-action^="view-"]')) {
    button.setAttribute("aria-pressed", String(button.dataset.action === `view-${view}`));
  }
  if (view !== "preview") editor.view.requestMeasure();
}

function setWrap(next: boolean) {
  wrap = next;
  settings.set("wrap", next ? "on" : "off");
  editor.setWrap(next);
  $("wrap-button").setAttribute("aria-pressed", String(next));
}

function setSplit(fraction: number) {
  const clamped = Math.min(0.8, Math.max(0.2, fraction));
  panes.style.setProperty("--split", `${(clamped * 100).toFixed(2)}%`);
  settings.set("split", String(clamped));
}

$("divider").addEventListener("pointerdown", (down) => {
  const divider = down.currentTarget as HTMLElement;
  divider.setPointerCapture(down.pointerId);
  const move = (event: PointerEvent) => {
    const box = panes.getBoundingClientRect();
    setSplit((event.clientX - box.left) / box.width);
  };
  const stop = () => divider.removeEventListener("pointermove", move);
  divider.addEventListener("pointermove", move);
  divider.addEventListener("lostpointercapture", stop, { once: true });
});

// ---------------------------------------------------------------- Commands

let busy = false;

/** Actions that open dialogs must not stack on top of each other. */
async function exclusive(task: () => Promise<unknown>) {
  if (busy) return;
  busy = true;
  try {
    await task();
  } finally {
    busy = false;
  }
}

const actions: Record<string, () => void> = {
  new: () => void exclusive(newDocument),
  open: () => void exclusive(() => openDocument()),
  save: () => void exclusive(saveDocument),
  "save-as": () => void exclusive(saveDocumentAs),
  export: () => void exclusive(exportPdf),
  close: () => void appWindow.close(),
  wrap: () => setWrap(!wrap),
  "view-editor": () => setView("editor"),
  "view-split": () => setView("split"),
  "view-preview": () => setView("preview"),
  "zoom-in": () => preview.zoomBy(1),
  "zoom-out": () => preview.zoomBy(-1),
  "zoom-fit": () => preview.fit(),
};

for (const button of document.querySelectorAll<HTMLElement>("[data-action]")) {
  button.onclick = () => actions[button.dataset.action!]();
}

function shortcutFor(event: KeyboardEvent): string | null {
  if (event.altKey) {
    return !event.ctrlKey && !event.metaKey && event.code === "KeyZ" ? "wrap" : null;
  }
  if (!event.ctrlKey || event.metaKey) return null;
  const key = event.key.toLowerCase();
  if (event.shiftKey && key === "s") return "save-as";
  const plain: Record<string, string> = {
    n: "new",
    o: "open",
    s: "save",
    e: "export",
    q: "close",
    w: "close",
    "1": "view-editor",
    "2": "view-split",
    "3": "view-preview",
    "=": "zoom-in",
    "+": "zoom-in",
    "-": "zoom-out",
    "0": "zoom-fit",
  };
  return plain[key] ?? null;
}

// On macOS the menu bar owns these shortcuts.
if (!isMac) {
  window.addEventListener(
    "keydown",
    (event) => {
      const action = shortcutFor(event);
      if (!action) return;
      event.preventDefault();
      event.stopPropagation();
      actions[action]();
    },
    { capture: true },
  );
}

// ---------------------------------------------------------------- Startup

async function start() {
  setView((settings.get("view") as View | null) ?? "split");
  setWrap(wrap);
  setSplit(Number(settings.get("split") ?? 0.5));

  highlightClasses = await backend.highlightClasses();

  await listen<string>("menu", (event) => actions[event.payload]?.());

  await appWindow.onCloseRequested(async (event) => {
    event.preventDefault();
    await exclusive(async () => {
      if (!(await confirmLeave())) return;
      closing = true;
      localStorage.removeItem(DRAFT_KEY);
      await appWindow.destroy();
    });
  });

  await appWindow.onDragDropEvent((event) => {
    if (event.payload.type !== "drop") return;
    const [path] = event.payload.paths;
    if (path) void exclusive(() => openDocument(path));
  });

  window.addEventListener("beforeunload", writeDraft);
  // Files next to the document may have changed while we were in the background.
  window.addEventListener("focus", requestCompile);

  const draft = readDraft();
  const requested = await backend.startupFile();
  if (draft) {
    const { text, ...state } = draft;
    const buttons = { yes: "Restore", no: "Discard", cancel: "Cancel" };
    const name = state.path ? baseName(state.path) : "an untitled document";
    const answer = await message(`Restore unsaved changes to ${name}?`, {
      title: "Recovered document",
      buttons,
    });
    // Only an explicit "Discard" throws the text away; Escape restores it.
    if (answer !== buttons.no && answer !== "No") return show(state, text);
    localStorage.removeItem(DRAFT_KEY);
  }

  const path = requested ?? settings.get("lastFile");
  if (path) {
    try {
      const { text, ...state } = fromDisk(path, await backend.readFile(path));
      return show(state, text);
    } catch (error) {
      if (requested) await showError("Could not open the file", error);
    }
  }
  const { text: _, ...state } = fromDisk(null, "");
  show({ ...state, savedText: WELCOME }, WELCOME);
}

start().catch((error) => notify(`Startup failed: ${error}`, true));
