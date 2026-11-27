//! Autocomplete, hover help and the symbol list, built on `typst-ide`.

use std::path::Path;

use serde::Serialize;
use typst::foundations::{Scope, Value};
use typst::syntax::{FileId, LinkedNode, RootedPath, Side, Source, SyntaxKind, VirtualPath, VirtualRoot};
use typst::World;
use typst_ide::{CompletionKind, IdeWorld, Tooltip};

use crate::engine::Engine;

/// How many project files to offer as path completions.
const MAX_FILES: usize = 500;

impl IdeWorld for Engine {
    fn upcast(&self) -> &dyn World {
        self
    }

    fn files(&self) -> Vec<FileId> {
        let mut out = Vec::new();
        if let Some(root) = self.root_dir() {
            list_files(root, root, 0, &mut out);
        }
        out
    }
}

fn list_files(root: &Path, dir: &Path, depth: usize, out: &mut Vec<FileId>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name();
        let Ok(kind) = entry.file_type() else { continue };
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if depth < 3 && name != "node_modules" && name != "target" {
                list_files(root, &path, depth + 1, out);
            }
        } else if let Ok(vpath) = VirtualPath::virtualize(root, &path) {
            out.push(RootedPath::new(VirtualRoot::Project, vpath).intern());
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CompletionItem {
    pub kind: &'static str,
    pub label: String,
    /// Text to insert, with `${placeholder}` fields.
    pub apply: Option<String>,
    pub detail: Option<String>,
    /// The character, for symbol completions.
    pub symbol: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Completions {
    /// UTF-16 offset where the replaced text starts.
    pub from: usize,
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Serialize)]
pub struct Hover {
    pub text: String,
    pub code: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolEntry {
    /// The name inside `sym`, such as `arrow.r.double`.
    pub name: String,
    pub value: String,
}

/// Convert an editor offset (UTF-16) to a byte offset.
pub(crate) fn byte_offset(source: &Source, utf16: usize) -> usize {
    source
        .lines()
        .utf16_to_byte(utf16)
        .unwrap_or_else(|| source.text().len())
}

impl Engine {
    pub fn complete(
        &mut self,
        path: Option<&Path>,
        text: String,
        offset: usize,
        explicit: bool,
    ) -> Option<Completions> {
        self.set_document(path, text);
        let source = self.source(self.main()).ok()?;
        let cursor = byte_offset(&source, offset);
        let (from, found) = typst_ide::autocomplete(
            self,
            self.last_document(path),
            &source,
            cursor,
            explicit,
        )?;
        let items = found
            .into_iter()
            .map(|c| {
                let mut symbol = None;
                let kind = match c.kind {
                    CompletionKind::Syntax => "keyword",
                    CompletionKind::Func => "function",
                    CompletionKind::Type => "type",
                    CompletionKind::Param => "property",
                    CompletionKind::Constant => "constant",
                    CompletionKind::Path => "text",
                    CompletionKind::Package => "namespace",
                    CompletionKind::Label => "variable",
                    CompletionKind::Font => "text",
                    CompletionKind::Symbol(value) => {
                        symbol = Some(value.to_string());
                        "constant"
                    }
                };
                CompletionItem {
                    kind,
                    label: c.label.into(),
                    apply: c.apply.map(Into::into),
                    detail: c.detail.map(Into::into),
                    symbol,
                }
            })
            .collect();
        Some(Completions {
            from: source.lines().byte_to_utf16(from)?,
            items,
        })
    }

    pub fn hover(
        &mut self,
        path: Option<&Path>,
        text: String,
        offset: usize,
        before: bool,
    ) -> Option<Hover> {
        self.set_document(path, text);
        let source = self.source(self.main()).ok()?;
        let cursor = byte_offset(&source, offset);
        let side = if before { Side::Before } else { Side::After };
        let tooltip = typst_ide::tooltip(self, self.last_document(path), &source, cursor, side)?;
        Some(match tooltip {
            Tooltip::Text(text) => Hover {
                text: text.into(),
                code: false,
            },
            Tooltip::Code(text) => Hover {
                text: text.into(),
                code: true,
            },
        })
    }

    /// Every symbol in Typst's `sym` module, including its variants.
    pub fn symbols(&self) -> Vec<SymbolEntry> {
        let mut out = Vec::new();
        let global: &Scope = self.library_ref().global.scope();
        if let Some(Value::Module(sym)) = global.get("sym").map(|b| b.read()) {
            for (name, binding) in sym.scope().iter() {
                let Value::Symbol(symbol) = binding.read() else {
                    continue;
                };
                for (modifiers, value, deprecation) in symbol.variants() {
                    if deprecation.is_some() {
                        continue;
                    }
                    let mut full = name.to_string();
                    for modifier in modifiers.into_iter() {
                        full.push('.');
                        full.push_str(modifier);
                    }
                    out.push(SymbolEntry {
                        name: full,
                        value: value.to_string(),
                    });
                }
            }
        }
        out
    }
}

/// Whether text typed at `offset` would be markup, math or code.
pub fn mode_at(text: &str, offset: usize) -> &'static str {
    let source = Source::detached(text);
    let cursor = byte_offset(&source, offset);
    let root = LinkedNode::new(source.root());
    let mut node = root.leaf_at(cursor, Side::Before);
    while let Some(current) = node {
        let range = current.range();
        match current.kind() {
            // Only when the cursor is between the delimiters.
            SyntaxKind::Equation if cursor > range.start && cursor < range.end => return "math",
            SyntaxKind::ContentBlock if cursor > range.start && cursor < range.end => {
                return "markup"
            }
            SyntaxKind::CodeBlock | SyntaxKind::Args
                if cursor > range.start && cursor < range.end =>
            {
                return "code"
            }
            _ => {}
        }
        node = current.parent().cloned();
    }
    "markup"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::engine;

    #[test]
    fn completes_functions_and_document_labels() {
        let mut engine = engine();
        let found = engine.complete(None, "😀 #tab".into(), 7, false).unwrap();
        assert_eq!(found.from, 4, "offsets are UTF-16");
        assert!(found.items.iter().any(|c| c.label == "table"));

        let text = "#set heading(numbering: \"1.\")\n= Intro <intro>\n";
        engine.compile(None, text.into(), &[]);
        let typed = format!("{text}@");
        let at = typed.encode_utf16().count();
        let found = engine.complete(None, typed, at, false).unwrap();
        assert!(found.items.iter().any(|c| c.label == "intro"));
    }

    #[test]
    fn describes_a_parameter_on_hover() {
        let mut engine = engine();
        let hover = engine.hover(None, "#text(size: 12pt)[a]".into(), 8, false);
        assert!(hover.is_some());
    }

    #[test]
    fn lists_symbols_with_variants() {
        let symbols = engine().symbols();
        let find = |name: &str| symbols.iter().find(|s| s.name == name);
        assert_eq!(find("alpha").unwrap().value, "α");
        assert!(find("arrow.r").is_some());
        assert!(symbols.len() > 500);
    }

    #[test]
    fn tells_markup_math_and_code_apart() {
        assert_eq!(mode_at("a $x + $ b", 7), "math");
        assert_eq!(mode_at("a $x$ b", 7), "markup");
        assert_eq!(mode_at("#f(a, )", 6), "code");
        assert_eq!(mode_at("#f[text ]", 8), "markup");
    }
}
