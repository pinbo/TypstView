# TypstView

A cross-platform desktop editor for [Typst](https://typst.app): source on one
side, live preview on the other. It follows the core workflow of TypstEdit (a
macOS-only SwiftUI app) on a stack that also targets Windows and Linux.

This is a first version. It has been built and run **only on macOS (Apple
Silicon)**. See [Test status](#test-status).

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
[Node.js](https://nodejs.org) with npm (tested with Node 26.5; the build tool,
Vite 8, needs Node 20.19+ or 22.12+).

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

Run the Rust tests:

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

**It has not run yet.** It starts once the project is in a GitHub repository. It
runs on pushes to `main`, on pull requests, and on demand from the Actions tab;
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

- **Automated:** 8 Rust tests cover page output and reuse of unchanged pages,
  error positions in non-ASCII text, included files, unsaved documents, PDF
  output, highlighting offsets and file writing. The frontend is type-checked but
  has no automated tests.
- **Real app (development and packaged `.app`):** launching with a file,
  highlighting, line wrapping, preview rendering including an included file, and
  the display of a compile error.
- **Real UI and real compiler, in a browser with a simulated desktop shell**
  (a temporary harness, not kept in the repository): typing with live preview,
  wrap toggle, zoom, layouts, jump to error, new/open/save/save as logic and
  their prompts, the overwrite warning, CRLF/BOM preservation, the close prompt,
  draft recovery and dark appearance. Native dialogs and file access were faked
  there, and it ran in the Browser pane's engine, not the system web view.
- **Not yet checked by hand in the real app:** native file and message dialogs,
  the macOS menu and shortcuts, the close and quit prompt, drag and drop, saving
  and PDF export through the UI, package download, and installing from the
  `.dmg` (it builds on this Mac and passes `hdiutil verify`).
- **Not tested at all:** Windows, Linux and Intel Macs. Nothing has been built or
  run on them. The code has no macOS-only logic apart from the menu bar, but
  expect to fix things on first contact, particularly in shortcuts, dialogs and
  web view rendering.

## Known limitations

- One document and one window. Running two copies at once is not supported
  because they share the recovery copy.
- Quitting from the Dock or logging out on macOS may skip the unsaved-changes
  prompt; the text is offered for recovery at the next launch. Recovery copies
  are written about every half second, and very large documents may not fit.
- Links in the preview are not clickable.
- The whole document is re-highlighted on each change, which may be slow for very
  large files.
- No file association: double-clicking a `.typ` file does not open it here.

## Possible later work

Features TypstEdit has that this version leaves out:

- **Editing help:** Typst autocomplete and hover (the `typst-ide` crate is a good
  base), formatting buttons and shortcuts, lists, quotes and code blocks,
  snippet and symbol insertion, Markdown-to-Typst paste.
- **Visual editors:** equations, tables, figures, links, bibliography and others,
  plus "edit at cursor".
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
src/                 frontend: main.ts (app), editor.ts, preview.ts, styles.css
src-tauri/src/       engine.rs (Typst), lib.rs (commands), files.rs, menu.rs (macOS)
samples/             a small multi-file example document
app-icon.svg         icon source; regenerate with `npx tauri icon app-icon.svg`
```
