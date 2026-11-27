pub mod bibliography;
pub mod constructs;
pub mod engine;
mod files;
pub mod ide;
pub mod markdown;
#[cfg(target_os = "macos")]
mod menu;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bibliography::{BibEntry, NewEntry};
use constructs::{Construct, Detection};
use engine::{CompileOutput, Diagnostic, Engine};
use ide::{Completions, Hover, SymbolEntry};
use tauri::State;

/// The compiler is created on first use because scanning system fonts takes a
/// moment, and kept so later compiles are incremental.
#[derive(Clone, Default)]
struct Compiler(Arc<Mutex<Option<Engine>>>);

impl Compiler {
    /// Run `job` off the UI thread with exclusive access to the engine.
    async fn run<T: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Engine) -> T + Send + 'static,
    ) -> Result<T, String> {
        let slot = self.0.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // If an earlier compile panicked, start over with a fresh engine.
            let mut guard = slot.lock().unwrap_or_else(|poisoned| {
                slot.clear_poison();
                let mut guard = poisoned.into_inner();
                *guard = None;
                guard
            });
            job(guard.get_or_insert_with(Engine::new))
        })
        .await
        .map_err(|e| format!("the Typst compiler crashed: {e}"))
    }
}

#[tauri::command]
async fn compile(
    compiler: State<'_, Compiler>,
    path: Option<PathBuf>,
    text: String,
    known: Vec<String>,
) -> Result<CompileOutput, String> {
    compiler
        .run(move |engine| engine.compile(path.as_deref(), text, &known))
        .await
}

/// Export the editor buffer as PDF. Compile errors are returned as data.
#[tauri::command]
async fn export_pdf(
    compiler: State<'_, Compiler>,
    path: Option<PathBuf>,
    text: String,
    target: PathBuf,
) -> Result<Vec<Diagnostic>, String> {
    if path.as_deref() == Some(target.as_path()) {
        return Err("refusing to overwrite the source document".into());
    }
    let result = compiler
        .run(move |engine| engine.pdf(path.as_deref(), text))
        .await?;
    match result {
        Ok(pdf) => {
            files::write_atomic(&target, &pdf)?;
            Ok(Vec::new())
        }
        Err(diagnostics) => Ok(diagnostics),
    }
}

#[tauri::command]
async fn highlight(text: String) -> Vec<u32> {
    engine::highlight_spans(&text)
}

#[tauri::command]
fn highlight_classes() -> Vec<&'static str> {
    engine::highlight_classes()
}

#[tauri::command]
async fn complete(
    compiler: State<'_, Compiler>,
    path: Option<PathBuf>,
    text: String,
    offset: usize,
    explicit: bool,
) -> Result<Option<Completions>, String> {
    compiler
        .run(move |engine| engine.complete(path.as_deref(), text, offset, explicit))
        .await
}

#[tauri::command]
async fn hover(
    compiler: State<'_, Compiler>,
    path: Option<PathBuf>,
    text: String,
    offset: usize,
    before: bool,
) -> Result<Option<Hover>, String> {
    compiler
        .run(move |engine| engine.hover(path.as_deref(), text, offset, before))
        .await
}

#[tauri::command]
async fn symbols(compiler: State<'_, Compiler>) -> Result<Vec<SymbolEntry>, String> {
    compiler.run(|engine| engine.symbols()).await
}

#[tauri::command]
fn mode_at(text: String, offset: usize) -> &'static str {
    ide::mode_at(&text, offset)
}

/// A preview for a visual editor: the SVG, or why there is none.
#[tauri::command]
async fn render_snippet(
    compiler: State<'_, Compiler>,
    path: Option<PathBuf>,
    document: String,
    snippet: String,
) -> Result<String, String> {
    compiler
        .run(move |engine| engine.render_snippet(path.as_deref(), &document, &snippet))
        .await?
        .map_err(|errors| {
            let messages: Vec<String> = errors.into_iter().map(|d| d.message).collect();
            messages.join("\n")
        })
}

#[tauri::command]
async fn detect_construct(text: String, offset: usize) -> Option<Detection> {
    constructs::detect(&text, offset)
}

#[tauri::command]
fn render_construct(construct: Construct, in_code: bool, indent: String) -> Result<String, String> {
    constructs::render(&construct, in_code, &indent)
}

#[tauri::command]
async fn markdown_to_typst(markdown: String) -> String {
    markdown::to_typst(&markdown)
}

#[tauri::command]
async fn bibliography_entries(document: PathBuf, file: String) -> Result<Vec<BibEntry>, String> {
    bibliography::list(&bibliography::resolve(&document, &file)?)
}

#[tauri::command]
async fn add_bibliography_entry(
    document: PathBuf,
    file: String,
    entry: NewEntry,
) -> Result<(), String> {
    bibliography::add(&bibliography::resolve(&document, &file)?, &entry)
}

/// The path to write in the document for a file the user picked.
#[tauri::command]
fn document_relative_path(document: PathBuf, target: PathBuf) -> Result<String, String> {
    files::relative_to_document(&document, &target)
}

#[tauri::command]
async fn read_file(path: PathBuf) -> Result<String, String> {
    files::read_text(&path)
}

#[tauri::command]
async fn write_file(path: PathBuf, text: String) -> Result<(), String> {
    files::write_atomic(&path, text.as_bytes())
}

/// The document named on the command line, if any: `typstview notes.typ`.
#[tauri::command]
fn startup_file() -> Option<PathBuf> {
    let arg = std::env::args_os().nth(1)?;
    let path = Path::new(&arg);
    // Under `tauri dev` the process starts in src-tauri, so also try the
    // folder the command was typed in.
    let candidates = [
        std::path::absolute(path).ok(),
        std::env::var_os("INIT_CWD").map(|dir| Path::new(&dir).join(path)),
    ];
    candidates.into_iter().flatten().find(|p| p.is_file())
}

pub fn run() {
    let compiler = Compiler::default();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(compiler.clone())
        .setup(move |_app| {
            // Warm up the compiler while the window loads.
            let compiler = compiler.clone();
            tauri::async_runtime::spawn(async move {
                let _ = compiler.run(|_| ()).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            compile,
            export_pdf,
            highlight,
            highlight_classes,
            read_file,
            write_file,
            startup_file,
            complete,
            hover,
            symbols,
            mode_at,
            render_snippet,
            detect_construct,
            render_construct,
            markdown_to_typst,
            bibliography_entries,
            add_bibliography_entry,
            document_relative_path
        ]);

    // macOS needs a menu bar. Windows and Linux use the in-window toolbar.
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::build).on_menu_event(menu::forward);

    builder
        .run(tauri::generate_context!())
        .expect("failed to start TypstView");
}
