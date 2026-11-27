//! The embedded Typst compiler.
//!
//! The document being edited is served from memory, so the preview always shows
//! the editor buffer, saved or not. Every other file (includes, images, data,
//! bibliographies) is read from the folder that contains the document.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::Serialize;
use typst::diag::{FileError, FileResult, Severity, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{
    highlight, DiagSpan, FileId, LinkedNode, RootedPath, Source, VirtualPath, VirtualRoot,
};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_kit::datetime::Time;
use typst_kit::downloader::SystemDownloader;
use typst_kit::files::{FileLoader, FileStore, FsRoot};
use typst_kit::fonts::FontStore;
use typst_kit::packages::SystemPackages;
use typst_layout::PagedDocument;

/// File name used for a document that has never been saved.
const UNTITLED: &str = "untitled.typ";

/// Resolves file requests from the compiler.
struct Loader {
    /// Folder of the open document. `None` while the document is untitled.
    root: Option<PathBuf>,
    /// The open document's id and current editor text.
    main: FileId,
    text: String,
    packages: SystemPackages,
}

impl FileLoader for Loader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main {
            return Ok(Bytes::from_string(self.text.clone()));
        }
        match id.root() {
            VirtualRoot::Project => match &self.root {
                Some(root) => FsRoot::new(root.clone()).load(id.vpath()),
                None => Err(FileError::Other(Some(
                    "save the document first to use files next to it".into(),
                ))),
            },
            VirtualRoot::Package(spec) => self.packages.obtain(spec)?.load(id.vpath()),
        }
    }
}

pub struct Engine {
    library: LazyHash<Library>,
    fonts: FontStore,
    files: FileStore<Loader>,
    time: Time,
    /// The last successful compile of the open document. Autocomplete and
    /// hover use it to know about labels and the values of variables.
    last: Option<(Option<PathBuf>, PagedDocument)>,
}

impl World for Engine {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }
    fn main(&self) -> FileId {
        self.files.loader().main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }
    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}

/// One compile error or warning, ready for display.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: &'static str,
    pub message: String,
    pub hints: Vec<String>,
    /// File the diagnostic points into; `None` for the open document.
    pub file: Option<String>,
    /// 1-based position, when the diagnostic points at source code.
    pub line: Option<usize>,
    pub column: Option<usize>,
    /// UTF-16 offsets (what the editor uses); only set for the open document.
    pub from: Option<usize>,
    pub to: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct PageOutput {
    pub hash: String,
    /// Omitted when the caller already holds a page with this hash.
    pub svg: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompileOutput {
    /// `None` when compilation failed; the caller keeps its last good preview.
    pub pages: Option<Vec<PageOutput>>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        let mut fonts = FontStore::new();
        // System fonts first, so they win over the bundled copies (as in the CLI).
        fonts.extend(typst_kit::fonts::system());
        fonts.extend(typst_kit::fonts::embedded());

        let packages = SystemPackages::new(SystemDownloader::new(concat!(
            "typstview/",
            env!("CARGO_PKG_VERSION")
        )));
        let loader = Loader {
            root: None,
            main: file_id(UNTITLED),
            text: String::new(),
            packages,
        };

        Self {
            library: LazyHash::new(Library::default()),
            fonts,
            files: FileStore::new(loader),
            time: Time::system(),
            last: None,
        }
    }

    /// Point the compiler at the current editor contents.
    pub(crate) fn set_document(&mut self, path: Option<&Path>, text: String) {
        let (root, name) = match path {
            Some(p) => (
                p.parent().map(Path::to_path_buf),
                p.file_name().map(|n| n.to_string_lossy().into_owned()),
            ),
            None => (None, None),
        };
        let loader = self.files.loader_mut();
        loader.main = file_id(name.as_deref().unwrap_or(UNTITLED));
        loader.root = root;
        loader.text = text;
        // Re-read everything on next access so edits to included files show up.
        self.files.reset();
        self.time.reset();
    }

    fn run(&self) -> (Option<PagedDocument>, Vec<Diagnostic>) {
        let result = typst::compile::<PagedDocument>(self);
        let mut diagnostics = Vec::new();
        let document = match result.output {
            Ok(document) => Some(document),
            Err(errors) => {
                diagnostics.extend(errors.iter().map(|d| self.convert(d)));
                None
            }
        };
        diagnostics.extend(result.warnings.iter().map(|d| self.convert(d)));
        // Bound the incremental compilation cache.
        comemo::evict(10);
        (document, diagnostics)
    }

    /// Compile to one SVG per page. Pages whose hash is in `known` at the same
    /// position are returned without their SVG.
    pub fn compile(
        &mut self,
        path: Option<&Path>,
        text: String,
        known: &[String],
    ) -> CompileOutput {
        self.set_document(path, text);
        let (document, diagnostics) = self.run();
        let pages = document.as_ref().map(|document| {
            let options = typst_svg::SvgOptions::default();
            document
                .pages()
                .iter()
                .enumerate()
                .map(|(i, page)| {
                    let svg = typst_svg::svg(page, &options);
                    let mut hasher = DefaultHasher::new();
                    svg.hash(&mut hasher);
                    let hash = format!("{:016x}", hasher.finish());
                    let unchanged = known.get(i) == Some(&hash);
                    PageOutput {
                        hash,
                        svg: (!unchanged).then_some(svg),
                    }
                })
                .collect()
        });
        if let Some(document) = document {
            self.last = Some((path.map(Path::to_path_buf), document));
        }
        CompileOutput { pages, diagnostics }
    }

    /// Compile to PDF bytes, or return the errors that prevented it.
    pub fn pdf(&mut self, path: Option<&Path>, text: String) -> Result<Vec<u8>, Vec<Diagnostic>> {
        self.set_document(path, text);
        let (document, diagnostics) = self.run();
        let Some(document) = document else {
            return Err(diagnostics);
        };
        typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
            .map_err(|errors| errors.iter().map(|d| self.convert(d)).collect())
    }

    /// The last compiled document, if it belongs to the file at `path`.
    pub(crate) fn last_document(&self, path: Option<&Path>) -> Option<&PagedDocument> {
        self.last
            .as_ref()
            .filter(|(of, _)| of.as_deref() == path)
            .map(|(_, document)| document)
    }

    /// Render a small piece of Typst, such as an equation being edited, as a
    /// tightly cropped SVG. The open document's top-level `#let` and `#import`
    /// lines come first, so its own definitions work in the preview.
    pub fn render_snippet(
        &mut self,
        path: Option<&Path>,
        document: &str,
        snippet: &str,
    ) -> Result<String, Vec<Diagnostic>> {
        const PAGE: &str = "#set page(width: auto, height: auto, margin: 6pt)\n";
        let preamble = definitions(document);
        let mut attempt = |preamble: &str| {
            self.set_document(path, format!("{preamble}{PAGE}{snippet}\n"));
            let (output, diagnostics) = self.run();
            match output.as_ref().and_then(|d| d.pages().first()) {
                Some(page) => Ok(typst_svg::svg(page, &Default::default())),
                None => Err(diagnostics),
            }
        };
        match attempt(&preamble) {
            // A broken definition elsewhere must not break the preview.
            Err(_) if !preamble.is_empty() => attempt(""),
            result => result,
        }
        // Positions would refer to the temporary source, so drop them.
        .map_err(|mut diagnostics| {
            for d in &mut diagnostics {
                (d.line, d.column, d.from, d.to) = (None, None, None, None);
            }
            diagnostics
        })
    }

    pub(crate) fn library_ref(&self) -> &Library {
        &self.library
    }

    pub(crate) fn root_dir(&self) -> Option<&Path> {
        self.files.loader().root.as_deref()
    }

    fn convert(&self, diagnostic: &SourceDiagnostic) -> Diagnostic {
        let mut out = Diagnostic {
            severity: match diagnostic.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            message: diagnostic.message.to_string(),
            hints: diagnostic.hints.iter().map(|h| h.v.to_string()).collect(),
            file: None,
            line: None,
            column: None,
            from: None,
            to: None,
        };

        // An error raised inside a package is more useful at the call site in
        // the open document. Errors in the project's own files stay where they are.
        let main = self.main();
        let mut span: DiagSpan = diagnostic.span;
        let in_package = span
            .id()
            .is_some_and(|id| matches!(id.root(), VirtualRoot::Package(_)));
        if in_package || span.id().is_none() {
            if let Some(site) = diagnostic.trace.iter().find(|t| t.span.id() == Some(main)) {
                span = site.span.into();
            }
        }
        self.locate(span, &mut out);
        out
    }

    fn locate(&self, span: DiagSpan, out: &mut Diagnostic) {
        let Some(id) = span.id() else { return };
        if id != self.main() {
            out.file = Some(match id.root() {
                VirtualRoot::Project => id.vpath().get_without_slash().to_string(),
                VirtualRoot::Package(spec) => {
                    format!("{spec}{}", id.vpath().get_with_slash())
                }
            });
        }
        let (Some(range), Ok(source)) = (WorldExt::range(self, span), self.source(id)) else {
            return;
        };
        let lines = source.lines();
        if let Some((line, column)) = lines.byte_to_line_column(range.start) {
            out.line = Some(line + 1);
            out.column = Some(column + 1);
        }
        if id == self.main() {
            out.from = lines.byte_to_utf16(range.start);
            out.to = lines.byte_to_utf16(range.end);
        }
    }
}

/// The top-level imports and definitions of a document.
fn definitions(text: &str) -> String {
    use typst::syntax::SyntaxKind::{LetBinding, ModuleImport};
    let root = typst::syntax::parse(text);
    let mut out = String::new();
    for node in LinkedNode::new(&root).children() {
        if matches!(node.kind(), LetBinding | ModuleImport) {
            out.push('#');
            out.push_str(&text[node.range()]);
            out.push('\n');
        }
    }
    out
}

pub(crate) fn file_id(name: &str) -> FileId {
    let vpath = VirtualPath::new(name)
        .or_else(|_| VirtualPath::new(UNTITLED))
        .expect("the fallback file name is a valid path");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

/// Syntax highlighting using Typst's own parser.
///
/// Returns flat `[from, to, tag]` triples in UTF-16 offsets, outermost spans
/// first. `tag` indexes into [`highlight_classes`].
pub fn highlight_spans(text: &str) -> Vec<u32> {
    fn walk(node: &LinkedNode, offset: &mut u32, text: &str, out: &mut Vec<u32>) {
        let tag = highlight(node);
        let start = *offset;
        let slot = out.len();
        if let Some(tag) = tag {
            out.extend([start, start, tag as u32]);
        }
        if node.children().len() == 0 {
            let leaf = &text[node.range()];
            *offset += leaf.encode_utf16().count() as u32;
        } else {
            for child in node.children() {
                walk(&child, offset, text, out);
            }
        }
        if tag.is_some() {
            out[slot + 1] = *offset;
        }
    }

    let root = typst::syntax::parse(text);
    let mut out = Vec::new();
    walk(&LinkedNode::new(&root), &mut 0, text, &mut out);
    out
}

/// CSS class for each highlight tag index.
pub fn highlight_classes() -> Vec<&'static str> {
    typst::syntax::Tag::LIST.iter().map(|t| t.css_class()).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    use std::sync::OnceLock;
    use std::sync::{Mutex, MutexGuard};

    /// Font discovery is slow, so the tests share one engine.
    pub(crate) fn engine() -> MutexGuard<'static, Engine> {
        static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();
        ENGINE
            .get_or_init(|| Mutex::new(Engine::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("typstview-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn compiles_pages_and_skips_unchanged_ones() {
        let mut engine = engine();
        let text = "= One\n#pagebreak()\n= Two";
        let first = engine.compile(None, text.into(), &[]);
        let pages = first.pages.unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages.iter().all(|p| p.svg.as_ref().unwrap().starts_with("<svg")));

        let known: Vec<_> = pages.iter().map(|p| p.hash.clone()).collect();
        let again = engine.compile(None, "= One\n#pagebreak()\n= Changed".into(), &known);
        let pages = again.pages.unwrap();
        assert!(pages[0].svg.is_none(), "page 1 did not change");
        assert!(pages[1].svg.is_some(), "page 2 changed");
    }

    #[test]
    fn reports_errors_at_utf16_positions() {
        // The emoji is 4 UTF-8 bytes but 2 UTF-16 units.
        let out = engine().compile(None, "😀\n#nope".into(), &[]);
        assert!(out.pages.is_none());
        let d = &out.diagnostics[0];
        assert_eq!(d.severity, "error");
        assert!(d.message.contains("unknown variable"), "{}", d.message);
        assert_eq!((d.line, d.from, d.to), (Some(2), Some(4), Some(8)));
        assert!(d.file.is_none());
    }

    #[test]
    fn reads_neighbouring_files_and_picks_up_changes() {
        let mut engine = engine();
        let dir = temp_dir("include");
        let main = dir.join("main.typ");
        fs::write(dir.join("part.typ"), "#let x = missing").unwrap();
        let text = "#include \"part.typ\"";

        let broken = engine.compile(Some(&main), text.into(), &[]);
        assert_eq!(broken.diagnostics[0].file.as_deref(), Some("part.typ"));

        fs::write(dir.join("part.typ"), "Fine now").unwrap();
        let fixed = engine.compile(Some(&main), text.into(), &[]);
        assert!(fixed.pages.is_some(), "{:?}", fixed.diagnostics);
        // The main file never has to exist on disk: it comes from the editor.
        assert!(!main.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn untitled_documents_explain_missing_files() {
        let out = engine().compile(None, "#include \"a.typ\"".into(), &[]);
        assert!(out.diagnostics[0].message.contains("save the document first"));
    }

    #[test]
    fn renders_snippets_with_the_documents_definitions() {
        let mut engine = engine();
        let doc = "#let half = $1 / 2$\n= Title";
        let svg = engine.render_snippet(None, doc, "$ #half + x $").unwrap();
        assert!(svg.starts_with("<svg"));
        // A broken definition falls back to rendering without it.
        assert!(engine.render_snippet(None, "#let a = nope", "$x$").is_ok());
        let errors = engine.render_snippet(None, "", "$ nope $").unwrap_err();
        assert_eq!(errors[0].line, None);
    }

    #[test]
    fn exports_pdf() {
        let mut engine = engine();
        let pdf = engine.pdf(None, "Hello $x^2$".into()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(engine.pdf(None, "#nope".into()).is_err());
    }

    #[test]
    fn highlights_with_utf16_offsets() {
        let spans = highlight_spans("😀 #let a = 1");
        let classes = highlight_classes();
        let tokens: Vec<_> = spans
            .chunks(3)
            .map(|c| (c[0], c[1], classes[c[2] as usize]))
            .collect();
        assert!(tokens.contains(&(4, 7, "typ-key")), "{tokens:?}");
        assert!(tokens.contains(&(12, 13, "typ-num")), "{tokens:?}");
    }
}
