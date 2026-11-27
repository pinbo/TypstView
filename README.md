# TypstView

A cross-platform desktop editor for [Typst](https://typst.app): source on one
side, live preview on the other. It follows the core workflow of TypstEdit (a
macOS-only SwiftUI app) on a stack that also targets Windows and Linux.

This is a first version. It has been **run only on macOS (Apple Silicon)**. See
[Test status](#test-status).

## Features

- Source editor and live preview side by side, with a draggable divider and
  editor-only, split and preview-only layouts.
- The preview follows the editor buffer as you type. Saving is not required.
- Syntax highlighting from Typst's own parser.
- Line wrapping, on by default (toggle: **Wrap** or Alt/Option+Z).
- Compile errors and warnings: underlined in the editor, marked in the gutter and
  listed in a panel. Click an entry for the open document to jump to it. While the
  document has errors, the last good preview stays visible, dimmed.
- New, open, save, save as, drag and drop, and `typstview file.typ` on the command line.
- PDF export of the current buffer.
- Preview zoom and fit to width.
- Search and replace in the open file, undo/redo, bracket matching and closing.
- Light and dark appearance, following the system.
- Protection against losing work: a prompt for unsaved changes, a recovery copy
  of unsaved text, a warning before overwriting a file changed by another
  program, and saves that go through a temporary file.
- CRLF line endings and a UTF-8 BOM are preserved. (A file that mixes line
  endings is saved with CRLF.)

## Editing help

- **Autocomplete and hover help** from Typst's IDE library (`typst-ide`):
  functions, parameters, symbols, fonts, labels and files next to the document.
  Suggestions appear as you type; Ctrl+Space also asks for them. Package names
  are not suggested. Label suggestions come from the last successful compile.
- **Format bar** (and a Format menu on macOS): bold, italic, underline,
  strikethrough, highlight, subscript, superscript, inline code, headings,
  bullet, numbered and term lists, block quote, code block and comment.
- **Insert menu:** the visual editors below, a searchable symbol picker, and
  snippets (page break, page setup, text and paragraph settings, numbering,
  columns, centred block, line, definition, date).
- **Markdown:** *Paste Markdown as Typst* converts the clipboard, and *Convert
  selection from Markdown* converts text already in the document. An ordinary
  paste is never changed. LaTeX math and Markdown footnotes come through as
  literal text, inline HTML tags are dropped (their text stays), and HTML blocks
  are kept as code blocks.

## Visual editors

*Insert* opens an empty form. **Edit at Cursor** (Mod+.) reads the innermost
supported construct at the cursor into the same form and writes it back.

| Editor | What the form covers |
|---|---|
| Equation | Typst math with symbol buttons, a live rendered preview, display or inline, label. The preview uses the document's top-level `#let` and `#import` lines. It is not a WYSIWYG formula editor: you edit Typst source. |
| Table and grid | Cells, header rows, column widths, adding and removing rows and columns, pasting cells from a spreadsheet |
| Figure and image | Image file and width, or other content; caption and label |
| Link | Address or label, and text |
| Citations and bibliography | List and filter entries, cite one, add entries to a `.bib` file; separately, the `#bibliography(..)` line itself |
| Quote, code, footnote, outline | Their text and main options |

**What editing does to your source:**

- Named arguments without a form field, such as `align:` or `stroke:`, are
  written back unchanged. The form lists them.
- The construct you apply is reformatted in a standard layout. The rest of the
  document is not touched.
- A construct the form cannot represent in full is left alone, with the reason
  shown. Examples: tables using `table.cell`, `table.hline` or `table.footer`,
  computed columns, spread arguments, or comments that would be lost.
- A field that would spill into the rest of the document, such as one with an
  unbalanced bracket, is rejected before anything changes.
- Adding a bibliography entry only appends to the file. Existing entries,
  comments and formatting stay as they were. `.yml` files can be read and cited
  but not added to.
- Citing in a document with no bibliography adds one at the end. It only looks at
  the open file, so check for a duplicate if another file already has one.

## Why this stack

| Part | Choice | Reason |
|---|---|---|
| Shell | [Tauri 2](https://tauri.app) | One codebase for macOS, Windows and Linux. It uses the system web view, so the app is much smaller than a bundled-browser one. On macOS, Apple's Command Line Tools are enough; full Xcode is not needed. |
| Editor | [CodeMirror 6](https://codemirror.net) | Mature wrapping, input-method, undo, search and error-marker support. |
| UI code | TypeScript and Vite, no UI framework | The UI is small. |
| Compiler | Typst 0.15.1 **linked in as Rust crates** | No `typst` binary to ship or find per platform. Unsaved text compiles from memory while relative includes and images still resolve. Errors arrive as structured data, and recompiles are incremental. |

The main trade-off of linking the compiler is that upgrading Typst means
rebuilding the app. TypstEdit can instead swap its compiler binary.

## Build and run

### Requirements

All platforms: [Rust](https://rustup.rs) 1.92 or newer (tested with 1.99.1) and
[Node.js](https://nodejs.org) with npm (tested with Node 26.5; building needs
Node 20.19+ or 22.12+, and the tests need 22.18+ or 23.6+).

- **macOS:** `xcode-select --install` (Command Line Tools).
- **Windows:** Microsoft C++ Build Tools ("Desktop development with C++") and the
  WebView2 runtime (already part of Windows 11).
- **Linux (Debian/Ubuntu):**

  ```bash
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

  For other distributions see the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

### Commands

Install the JavaScript dependencies once:

```bash
npm install
```

Run in development mode (the first Rust build takes a few minutes):

```bash
npm run app
```

Open a file at startup:

```bash
npm run app -- -- -- samples/demo.typ
```

Run the tests (formatting commands with Node, everything else with Rust):

```bash
npm test
```

Type-check and build only the frontend:

```bash
npm run build
```

Build installers for the platform you are on:

```bash
npm run bundle
```

Output goes to `src-tauri/target/release/bundle/`. To build only the macOS `.app`:

```bash
npm run bundle -- --bundles app
```

Tauri does not generally cross-compile, so build each platform's package on that
platform (or on a CI runner for it). The builds are not code-signed or notarized;
other people's machines will warn about or block them.

## Continuous integration

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) type-checks the frontend,
runs the Rust tests and builds installers with GitHub Actions on four targets:

| Job | Runner | Installers |
|---|---|---|
| `macos-arm64` | `macos-latest` | `.dmg` |
| `macos-x64` | `macos-26-intel` | `.dmg` |
| `windows-x64` | `windows-latest` | `.exe` (NSIS), `.msi` |
| `linux-x64` | `ubuntu-22.04` | `.deb`, `.rpm`, `.AppImage` |

In its first completed jobs (the commit "Rename the app to TypstView", which
predates the editing help and visual editors), Linux, Windows and Apple Silicon
passed. The Intel Mac job had no failed step but had not finished when this was
written. It runs on pushes to `main`, on pull requests, and on demand from the Actions tab;
change the branch name in the file if yours differs. Installers are attached to
each run as artifacts, unsigned. Private repositories use up Actions minutes,
and macOS and Windows minutes cost more than Linux ones.

A passing job shows that the code compiles, tests pass and installers are
produced. It does not launch the app, so the interface still needs checking by
hand on each platform.

## Keyboard shortcuts

`Mod` is Cmd on macOS and Ctrl on Windows and Linux.

| Action | Keys |
|---|---|
| New / Open / Save / Save As | Mod+N / Mod+O / Mod+S / Mod+Shift+S |
| Export PDF | Mod+E |
| Editor only / Split / Preview only | Mod+1 / Mod+2 / Mod+3 |
| Toggle line wrapping | Alt/Option+Z |
| Zoom in / out / fit | Mod+= / Mod+- / Mod+0 |
| Find and replace | Mod+F |
| Edit at cursor | Mod+. |
| Autocomplete | Ctrl+Space |
| Bold / Italic / Underline / Strikethrough | Mod+B / Mod+I / Mod+U / Mod+Shift+X |
| Heading 1–4 / plain text | Mod+Alt+1–4 / Mod+Alt+0 |
| Bullet list / Numbered list | Mod+Shift+8 / Mod+Shift+7 |
| Toggle comment | Mod+/ |
| Insert equation / link | Mod+Shift+M / Mod+K |
| Paste Markdown as Typst | Mod+Shift+V |

## How files and packages resolve

- The folder containing the open document is the Typst project root. Includes,
  images and data files must be inside it. An unsaved document cannot refer to
  files on disk.
- Only the document in the editor comes from memory. Other files are read from
  disk at each compile. After changing one in another program, return to the
  TypstView window to refresh the preview.
- Fonts come from the system, plus the fonts bundled with Typst.
- `@preview/...` packages are downloaded from Typst Universe on first use and
  cached where the Typst CLI caches them. This needs network access.

## Test status

Checked on macOS 26.7.3, arm64, with only the Command Line Tools installed.

- **Automated:** 31 Rust tests cover compiling and PDF output, error positions,
  highlighting, file writing, autocomplete, hover and symbols, reading and
  rewriting every construct (including refusal of those it cannot keep),
  Markdown conversion and bibliography files. 7 Node tests cover the formatting
  commands. The dialogs have no automated tests.
- **Real app (development and packaged `.app`):** launching with a file,
  highlighting, line wrapping, preview rendering including an included file, and
  the display of a compile error.
- **Real UI and real compiler, in a browser with a simulated desktop shell**
  (a temporary harness, not kept in the repository): typing with live preview,
  wrap toggle, zoom, layouts, jump to error, new/open/save/save as logic and
  their prompts, the overwrite warning, CRLF/BOM preservation, the close prompt,
  draft recovery and dark appearance. Native dialogs and file access were faked
  there, and it ran in the Browser pane's engine, not the system web view.
- **Editing help and visual editors:** exercised in the browser harness,
  including real bibliography files. In the packaged app, a temporary self-test
  confirmed that the new commands answer (autocomplete, hover, symbols, construct
  reading and writing, Markdown, bibliography, clipboard access) and that the
  equation dialog opens with its preview and a sensible layout. That check was
  made by measurement: the screen was locked, so **the new interface has not been
  seen in the real app window**, or used there with a keyboard and mouse. The new
  macOS menus and the image file picker are unchecked.
- **Not yet checked by hand in the real app:** native file and message dialogs,
  the macOS menu and shortcuts, the close and quit prompt, drag and drop, saving
  and PDF export through the UI, package download, and installing from the
  `.dmg` (it builds on this Mac and passes `hdiutil verify`).
- **Never launched:** Windows, Linux and Intel Macs. CI has built and tested an
  earlier version (see [Continuous integration](#continuous-integration)), but
  nobody has run the app on them, and the editing help and visual editors have
  not been through CI. The code has no macOS-only logic apart from the menu bar, but
  expect to fix things on first contact, particularly in shortcuts, dialogs and
  web view rendering.

## Known limitations

- One document and one window. Running two copies at once is not supported
  because they share the recovery copy.
- Quitting from the Dock or logging out on macOS may skip the unsaved-changes
  prompt; the text is offered for recovery at the next launch. Recovery copies
  are written about every half second, and very large documents may not fit.
- Links in the preview are not clickable.
- The formatting commands do not check whether the cursor is in math or code.
- The whole document is re-highlighted on each change, which may be slow for very
  large files.
- No file association: double-clicking a `.typ` file does not open it here.

## Possible later work

Features TypstEdit has that this version leaves out:

- **More editing:** automatic Markdown conversion on ordinary paste, WYSIWYG
  equation editing, merged table cells and cell styling, and TypstEdit's other
  pickers and editors (layout, external data, foundation items and more).
- **Projects:** file sidebar, tabs and multiple windows, a chosen project root
  and main file, find in project, outline, recent files and welcome screen,
  templates, notebooks, backups.
- **Git:** the source control panel.
- **Import and export:** PDF and LyX import; PNG, SVG, Markdown and "Standard
  Typst" export.
- **Calculator:** calculator mode and calculations and plots inside documents.
- **AI:** autocomplete, prompt and rewrite, the chat panel, skills, MCP,
  semantic search, the MCP server and CLI.
- **Other files and tools:** highlighting for non-Typst code and data files,
  Python terminal.
- **Settings:** themes, fonts and font folders, colour-blindness modes,
  horizontal split, package manager, compiler updates.

Not tied to TypstEdit but worth doing next: jumping between preview and source,
and code signing.

## Layout

```
.github/workflows/   CI: tests and installers for each platform
src/                 frontend: main.ts (app), editor.ts, preview.ts, assist.ts
                     (autocomplete), insert.ts, forms.ts, dialog.ts, format.ts
src-tauri/src/       engine.rs (Typst), ide.rs, constructs.rs (visual editor model),
                     markdown.rs, bibliography.rs, files.rs, lib.rs, menu.rs
samples/             a small multi-file example document
app-icon.svg         icon source; regenerate with `npx tauri icon app-icon.svg`
```
