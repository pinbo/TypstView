//! The macOS menu bar. Items are forwarded to the web view as `menu` events.

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Runtime};

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let item = |id: &str, text: &str, keys: Option<&str>| {
        MenuItem::with_id(app, id, text, true, keys)
    };
    let line = || PredefinedMenuItem::separator(app);

    let application = Submenu::with_items(
        app,
        "TypstView",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &line()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &line()?,
            // Not the predefined Quit item: that one skips the unsaved-changes prompt.
            &item("close", "Quit TypstView", Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &item("new", "New", Some("CmdOrCtrl+N"))?,
            &item("open", "Open…", Some("CmdOrCtrl+O"))?,
            &line()?,
            &item("save", "Save", Some("CmdOrCtrl+S"))?,
            &item("save-as", "Save As…", Some("CmdOrCtrl+Shift+S"))?,
            &item("export", "Export PDF…", Some("CmdOrCtrl+E"))?,
            &line()?,
            &item("close", "Close Window", Some("CmdOrCtrl+W"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &line()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let format = Submenu::with_items(
        app,
        "Format",
        true,
        &[
            &item("bold", "Bold", Some("CmdOrCtrl+B"))?,
            &item("italic", "Italic", Some("CmdOrCtrl+I"))?,
            &item("underline", "Underline", Some("CmdOrCtrl+U"))?,
            &item("strike", "Strikethrough", Some("CmdOrCtrl+Shift+X"))?,
            &item("highlight", "Highlight", None)?,
            &item("text-color", "Text Color…", None)?,
            &item("subscript", "Subscript", None)?,
            &item("superscript", "Superscript", None)?,
            &item("inline-code", "Inline Code", None)?,
            &line()?,
            &item("heading-1", "Heading 1", Some("CmdOrCtrl+Alt+1"))?,
            &item("heading-2", "Heading 2", Some("CmdOrCtrl+Alt+2"))?,
            &item("heading-3", "Heading 3", Some("CmdOrCtrl+Alt+3"))?,
            &item("heading-0", "Plain Text", Some("CmdOrCtrl+Alt+0"))?,
            &line()?,
            &item("bullet-list", "Bullet List", Some("CmdOrCtrl+Shift+8"))?,
            &item("numbered-list", "Numbered List", Some("CmdOrCtrl+Shift+7"))?,
            &item("term-list", "Term List", None)?,
            &item("quote-block", "Block Quote", None)?,
            &item("code-block", "Code Block", None)?,
            &line()?,
            &item("comment", "Toggle Comment", Some("CmdOrCtrl+/"))?,
        ],
    )?;
    let insert = Submenu::with_items(
        app,
        "Insert",
        true,
        &[
            &item("edit-at-cursor", "Edit at Cursor", Some("CmdOrCtrl+."))?,
            &line()?,
            &item("insert-equation", "Equation…", Some("CmdOrCtrl+Shift+M"))?,
            &item("insert-table", "Table…", None)?,
            &item("insert-figure", "Figure or Image…", None)?,
            &item("insert-link", "Link…", Some("CmdOrCtrl+K"))?,
            &item("citations", "Citations and Bibliography…", None)?,
            &item("insert-footnote", "Footnote…", None)?,
            &item("insert-quote", "Quote…", None)?,
            &item("insert-code", "Code…", None)?,
            &item("insert-outline", "Outline…", None)?,
            &item("symbols", "Symbol…", None)?,
            &line()?,
            &item("paste-markdown", "Paste Markdown as Typst", Some("CmdOrCtrl+Shift+V"))?,
            &item("convert-markdown", "Convert Selection from Markdown", None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item("view-editor", "Editor Only", Some("CmdOrCtrl+1"))?,
            &item("view-split", "Editor and Preview", Some("CmdOrCtrl+2"))?,
            &item("view-preview", "Preview Only", Some("CmdOrCtrl+3"))?,
            &line()?,
            &item("wrap", "Toggle Line Wrapping", Some("Alt+Z"))?,
            &line()?,
            &item("zoom-in", "Zoom In Preview", Some("CmdOrCtrl+="))?,
            &item("zoom-out", "Zoom Out Preview", Some("CmdOrCtrl+-"))?,
            &item("zoom-fit", "Fit Preview to Width", Some("CmdOrCtrl+0"))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    Menu::with_items(
        app,
        &[&application, &file, &edit, &format, &insert, &view, &window],
    )
}

pub fn forward<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let _ = app.emit("menu", event.id().as_ref());
}
