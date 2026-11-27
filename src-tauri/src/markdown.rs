//! Converting pasted Markdown into Typst markup.
//!
//! LaTeX math and Markdown footnotes are not interpreted; they come through as
//! literal text. Anything without a Typst counterpart is kept as text or as a
//! raw block, so no content silently disappears.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::constructs::{string_literal, write_raw};

/// Escape text so Typst shows it literally. `line_start` says whether it
/// begins a line, where a few more characters have a meaning.
fn escape(text: &str, line_start: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    if line_start {
        let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 && text[digits..].starts_with('.') {
            out.push_str(&text[..digits]);
            out.push_str("\\.");
            rest = &text[digits + 1..];
        } else if text.starts_with(['=', '-', '+', '/']) {
            out.push('\\');
        }
    }
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        let comment = c == '/' && matches!(chars.peek(), Some('/' | '*'));
        // A slash escaped at the start of the line must not get a second one.
        let already = out.ends_with('\\') && c == '/' && out.len() == 1;
        if (matches!(c, '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '@' | '[' | ']' | '~')
            || comment)
            && !already
        {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[derive(Default)]
struct Frame {
    out: String,
    /// What to do with the next text, after something that could merge with it.
    after: After,
}

#[derive(Default)]
enum After {
    #[default]
    Nothing,
    /// A `#function[..]` just ended; `(`, `[` or `.name` would extend it.
    Call,
    /// `*strong*` or `_emph_` just ended at this position. Typst ignores such
    /// a delimiter when a letter follows, so it may need the function form.
    Delimiter { start: usize, function: &'static str },
}

impl Frame {
    fn at_line_start(&self) -> bool {
        self.out.trim_end_matches(' ').is_empty() || self.out.trim_end_matches(' ').ends_with('\n')
    }

    /// Prepare for appending `next`.
    fn join(&mut self, next: &str) {
        let mut chars = next.chars();
        let first = chars.next();
        match std::mem::take(&mut self.after) {
            After::Call => {
                let extends = matches!(first, Some('(' | '['))
                    || (first == Some('.') && chars.next().is_some_and(char::is_alphabetic));
                if extends {
                    self.out.push(';');
                }
            }
            After::Delimiter { start, function } if first.is_some_and(char::is_alphanumeric) => {
                let inner = self.out[start + 1..self.out.len() - 1].to_string();
                self.out.truncate(start);
                self.out.push_str(&format!("#{function}[{inner}]"));
            }
            _ => {}
        }
    }

    fn inline(&mut self, piece: &str) {
        self.join(piece);
        self.out.push_str(piece);
    }

    fn call(&mut self, piece: &str) {
        self.inline(piece);
        self.after = After::Call;
    }

    fn block(&mut self, piece: &str) {
        self.after = After::Nothing;
        if !self.out.is_empty() {
            let trimmed = self.out.trim_end().len();
            self.out.truncate(trimmed);
            self.out.push_str("\n\n");
        }
        self.out.push_str(piece);
    }

    fn emphasis(&mut self, inner: &str, mark: char, function: &'static str) {
        let intraword = self.out.chars().last().is_some_and(char::is_alphanumeric);
        if intraword || inner.trim() != inner || inner.is_empty() {
            self.call(&format!("#{function}[{inner}]"));
        } else {
            self.inline("");
            let start = self.out.len();
            self.out.push_str(&format!("{mark}{inner}{mark}"));
            self.after = After::Delimiter { start, function };
        }
    }
}

struct List {
    next: Option<u64>,
    items: Vec<String>,
}

#[derive(Default)]
struct Table {
    align: Vec<Alignment>,
    header: Vec<String>,
    cells: Vec<String>,
}

pub fn to_typst(markdown: &str) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut stack = vec![Frame::default()];
    let mut lists: Vec<List> = Vec::new();
    let mut table = Table::default();
    // Link and image destinations, and code block languages.
    let mut targets: Vec<String> = Vec::new();
    let mut code: Option<String> = None;
    let mut html = String::new();
    // Plain text of the image being read, for its alternative text.
    let mut alt: Option<String> = None;

    for event in Parser::new_ext(markdown, options) {
        let top = stack.last_mut().expect("the root frame stays");
        match event {
            Event::Start(tag) => {
                match &tag {
                    Tag::List(start) => lists.push(List {
                        next: *start,
                        items: Vec::new(),
                    }),
                    Tag::Table(align) => {
                        table = Table {
                            align: align.clone(),
                            ..Table::default()
                        }
                    }
                    Tag::CodeBlock(kind) => {
                        let lang = match kind {
                            CodeBlockKind::Fenced(info) => info
                                .split([' ', ',', '{'])
                                .next()
                                .unwrap_or_default()
                                .to_string(),
                            CodeBlockKind::Indented => String::new(),
                        };
                        targets.push(lang);
                        code = Some(String::new());
                    }
                    Tag::Link { dest_url, .. } => targets.push(dest_url.to_string()),
                    Tag::Image { dest_url, .. } => {
                        targets.push(dest_url.to_string());
                        alt = Some(String::new());
                    }
                    _ => {}
                }
                stack.push(Frame::default());
            }
            Event::End(end) => {
                let frame = stack.pop().expect("every end has a start");
                let inner = frame.out;
                let top = stack.last_mut().expect("the root frame stays");
                match end {
                    TagEnd::Paragraph => top.block(inner.trim_end()),
                    TagEnd::Heading(level) => {
                        top.block(&format!("{} {inner}", "=".repeat(level as usize)))
                    }
                    TagEnd::Strong => top.emphasis(&inner, '*', "strong"),
                    TagEnd::Emphasis => top.emphasis(&inner, '_', "emph"),
                    TagEnd::Strikethrough => top.call(&format!("#strike[{inner}]")),
                    TagEnd::BlockQuote(_) => top.block(&format!(
                        "#quote(block: true)[\n  {}\n]",
                        inner.trim().replace('\n', "\n  ")
                    )),
                    TagEnd::CodeBlock => {
                        let lang = targets.pop().unwrap_or_default();
                        let text = code.take().unwrap_or_default();
                        let text = text.strip_suffix('\n').unwrap_or(&text);
                        top.block(&write_raw(&lang, text, true));
                    }
                    TagEnd::HtmlBlock => {
                        let source = std::mem::take(&mut html);
                        let source = source.trim();
                        if !(source.starts_with("<!--") && source.ends_with("-->")) {
                            top.block(&write_raw("html", source, true));
                        }
                    }
                    TagEnd::Link => {
                        let url = targets.pop().unwrap_or_default();
                        if url.is_empty() || url.starts_with('#') {
                            top.inline(&inner);
                        } else if inner == url || inner == escape(&url, false) {
                            top.call(&format!("#link({})", string_literal(&url)));
                        } else {
                            top.call(&format!("#link({})[{inner}]", string_literal(&url)));
                        }
                    }
                    TagEnd::Image => {
                        let url = targets.pop().unwrap_or_default();
                        let text = alt.take().unwrap_or_default();
                        if url.contains("://") || url.starts_with("data:") {
                            // Typst cannot load remote images.
                            let label = if inner.is_empty() {
                                escape(&url, false)
                            } else {
                                inner
                            };
                            top.call(&format!("#link({})[{label}]", string_literal(&url)));
                        } else if text.is_empty() {
                            top.call(&format!("#image({})", string_literal(&url)));
                        } else {
                            top.call(&format!(
                                "#image({}, alt: {})",
                                string_literal(&url),
                                string_literal(&text)
                            ));
                        }
                    }
                    TagEnd::Item => {
                        if let Some(list) = lists.last_mut() {
                            list.items.push(inner.trim().to_string());
                        }
                    }
                    TagEnd::List(_) => {
                        let Some(list) = lists.pop() else { continue };
                        let mut number = list.next;
                        let lines: Vec<String> = list
                            .items
                            .iter()
                            .map(|item| {
                                let marker = match number {
                                    None => "- ".to_string(),
                                    // Typst numbers `+` items from one by itself.
                                    Some(_) if list.next == Some(1) => "+ ".to_string(),
                                    Some(n) => format!("{n}. "),
                                };
                                number = number.map(|n| n + 1);
                                let indent = " ".repeat(marker.len());
                                format!("{marker}{}", item.replace('\n', &format!("\n{indent}")))
                            })
                            .collect();
                        let text = lines.join("\n");
                        if lists.is_empty() {
                            top.block(&text);
                        } else {
                            // A nested list goes on its own line inside the item.
                            if !top.out.is_empty() {
                                let trimmed = top.out.trim_end().len();
                                top.out.truncate(trimmed);
                                top.out.push('\n');
                            }
                            top.after = After::Nothing;
                            top.out.push_str(&text);
                        }
                    }
                    TagEnd::TableCell => table.cells.push(format!("[{inner}]")),
                    TagEnd::TableHead => table.header = std::mem::take(&mut table.cells),
                    TagEnd::TableRow => {}
                    TagEnd::Table => {
                        let width = table.align.len().max(1);
                        let mut lines = vec![format!("columns: {width}")];
                        if table.align.iter().any(|a| *a != Alignment::None) {
                            let names: Vec<&str> = table
                                .align
                                .iter()
                                .map(|a| match a {
                                    Alignment::None => "auto",
                                    Alignment::Left => "left",
                                    Alignment::Center => "center",
                                    Alignment::Right => "right",
                                })
                                .collect();
                            lines.push(format!("align: ({},)", names.join(", ")));
                        }
                        lines.push(format!("table.header({})", table.header.join(", ")));
                        lines.extend(table.cells.chunks(width).map(|row| row.join(", ")));
                        top.block(&format!("#table(\n  {},\n)", lines.join(",\n  ")));
                    }
                    _ => top.inline(&inner),
                }
            }
            Event::Text(text) => {
                if let Some(alt) = alt.as_mut() {
                    alt.push_str(&text);
                }
                match code.as_mut() {
                    Some(code) => code.push_str(&text),
                    None => {
                        top.join(&text);
                        // The parser may split `1.` into two pieces, so look
                        // at what the line holds so far.
                        let line = top.out.rsplit('\n').next().unwrap_or_default().trim_start();
                        if text.starts_with('.')
                            && !line.is_empty()
                            && line.chars().all(|c| c.is_ascii_digit())
                        {
                            top.out.push('\\');
                        }
                        let escaped = escape(&text, top.at_line_start());
                        top.out.push_str(&escaped);
                    }
                }
            }
            Event::Code(text) => {
                if text.contains('`') || text.is_empty() {
                    top.call(&format!("#raw({})", string_literal(&text)));
                } else {
                    top.inline(&format!("`{text}`"));
                }
            }
            Event::SoftBreak => top.inline("\n"),
            Event::HardBreak => top.inline(" \\\n"),
            Event::Rule => top.block("#line(length: 100%)"),
            Event::Html(source) => html.push_str(&source),
            Event::InlineHtml(source) => {
                // Inline tags are dropped; the text between them stays.
                if source.starts_with("<br") {
                    top.inline(" \\\n");
                }
            }
            Event::TaskListMarker(done) => top.inline(if done { "☒ " } else { "☐ " }),
            Event::InlineMath(text) | Event::DisplayMath(text) => top.inline(&escape(&text, false)),
            Event::FootnoteReference(name) => top.inline(&escape(&format!("[^{name}]"), false)),
        }
    }
    stack.pop().map(|f| f.out.trim_end().to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::engine;

    #[test]
    fn converts_the_common_structures() {
        let markdown = "# Title\n\nSome **bold**, *soft* and ~~gone~~ `code`.\n\n\
                        - one\n- two\n    1. inner\n\n3. three\n4. four\n\n\
                        > quoted\n\n```rust\nfn main() {}\n```\n\n\
                        [site](https://typst.app) ![alt](a.png)\n\n---\n\n\
                        | A | B |\n|:--|--:|\n| 1 | 2 |\n\n- [x] done\n";
        let typst = to_typst(markdown);
        for expected in [
            "= Title",
            "*bold*, _soft_ and #strike[gone] `code`.",
            "- one\n- two\n  + inner",
            "3. three\n4. four",
            "#quote(block: true)[\n  quoted\n]",
            "```rust\nfn main() {}\n```",
            "#link(\"https://typst.app\")[site] #image(\"a.png\", alt: \"alt\")",
            "#line(length: 100%)",
            "align: (left, right,)",
            "table.header([A], [B])",
            "- ☒ done",
        ] {
            assert!(typst.contains(expected), "missing {expected:?} in:\n{typst}");
        }
    }

    #[test]
    fn keeps_special_characters_literal() {
        assert_eq!(to_typst("cost $5 #1 a@b.org"), "cost \\$5 \\#1 a\\@b.org");
        assert_eq!(to_typst("1\\. not a list"), "1\\. not a list");
        assert_eq!(to_typst("\\= not a heading"), "\\= not a heading");
        assert_eq!(to_typst("a // b"), "a \\// b");
    }

    #[test]
    fn avoids_markup_that_typst_would_read_differently() {
        assert_eq!(to_typst("in**tra**word"), "in#strong[tra]word");
        assert_eq!(to_typst("~~a~~(b)"), "#strike[a];(b)");
        assert_eq!(to_typst("~~a~~.then"), "#strike[a];.then");
        assert_eq!(to_typst("~~a~~. Then"), "#strike[a]. Then");
    }

    #[test]
    fn hostile_text_still_compiles() {
        let markdown = "= + - / \\\\ # $ * _ ` < > @ [ ] ~ /* // \" ' { } ( ) ^ & % ;\n\n\
                        /term: x\n\n- - nested dash\n\n+ plus\n\n12. twelve\n\n\
                        <div>block html</div>\n\n<!-- hidden -->\n\ninline <b>tag</b><br>next\n\n\
                        `` a ` b `` and $x^2$ and [^1] and http://example.com/*x*\n\n\
                        **a **b** c** __d__e *f*g ***h***\n\n[x [y] z](u_(v)) # trailing\n\n\
                        | *a* | `|` |\n|---|---|\n| #x | ] |\n\n~~~\n```\n~~~\n";
        let typst = to_typst(markdown);
        let result = engine().compile(None, typst.clone(), &[]);
        assert!(result.pages.is_some(), "{:?}\n{typst}", result.diagnostics);
    }
}
