//! The model behind the visual editors: find the construct at the cursor, turn
//! it into form fields, and write form fields back as Typst source.
//!
//! The rule throughout is that editing must never silently drop part of a
//! document. Arguments the forms do not show are kept verbatim. A construct
//! that cannot be represented faithfully is reported as a problem instead.

use serde::{Deserialize, Serialize};
use typst::syntax::{ast, parse, parse_code, LinkedNode, Side, Source, SyntaxKind};

use crate::ide::byte_offset;

/// A named argument kept exactly as written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extra {
    pub name: String,
    pub value: String,
}

/// A table cell: the inside of a `[content]` block, or any other expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub text: String,
    pub code: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub path: String,
    pub width: String,
    pub extra: Vec<Extra>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Construct {
    Equation {
        body: String,
        block: bool,
        label: String,
    },
    Table {
        grid: bool,
        columns: Vec<String>,
        header: Vec<Vec<Cell>>,
        rows: Vec<Vec<Cell>>,
        extra: Vec<Extra>,
    },
    /// A `figure`, or a bare `image` when `figure` is false.
    Figure {
        figure: bool,
        image: Option<Image>,
        /// The figure's body as source, when it is not an image.
        content: String,
        caption: String,
        label: String,
        extra: Vec<Extra>,
    },
    Link {
        target: String,
        /// The target is an expression such as `<label>`, not a URL string.
        code: bool,
        text: String,
    },
    Quote {
        body: String,
        attribution: String,
        block: bool,
        extra: Vec<Extra>,
    },
    Code {
        lang: String,
        text: String,
        block: bool,
    },
    Footnote {
        body: String,
        extra: Vec<Extra>,
    },
    Outline {
        title: String,
        depth: String,
        extra: Vec<Extra>,
    },
    Bibliography {
        sources: Vec<String>,
        title: String,
        style: String,
        full: bool,
        extra: Vec<Extra>,
    },
}

/// What was found at the cursor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// UTF-16 range of the source the construct occupies.
    pub from: usize,
    pub to: usize,
    pub name: &'static str,
    /// Written inside code, so it takes no leading `#`.
    pub in_code: bool,
    /// Leading whitespace of the line the construct starts on.
    pub indent: String,
    /// Present when the construct can be edited visually.
    pub construct: Option<Construct>,
    /// Otherwise, why not.
    pub problem: Option<String>,
}

// ------------------------------------------------------------------ Finding

fn callee<'a>(node: &'a LinkedNode) -> Option<&'a str> {
    match node.cast::<ast::FuncCall>()?.callee() {
        ast::Expr::Ident(ident) => Some(ident.get()),
        _ => None,
    }
}

fn kind_of(node: &LinkedNode) -> Option<&'static str> {
    match node.kind() {
        SyntaxKind::Equation => Some("equation"),
        SyntaxKind::Raw => Some("code"),
        SyntaxKind::Link => Some("link"),
        SyntaxKind::FuncCall => match callee(node)? {
            "table" | "grid" => Some("table"),
            "figure" | "image" => Some("figure"),
            "link" => Some("link"),
            "quote" => Some("quote"),
            "footnote" => Some("footnote"),
            "outline" => Some("outline"),
            "bibliography" => Some("bibliography"),
            _ => None,
        },
        _ => None,
    }
}

/// The figure whose body is this `image(..)` call, if any.
fn enclosing_figure<'a>(node: &LinkedNode<'a>) -> Option<LinkedNode<'a>> {
    if node.kind() != SyntaxKind::FuncCall || callee(node) != Some("image") {
        return None;
    }
    let args = node.parent()?;
    let figure = args.parent()?;
    let first = args
        .children()
        .find(|c| !c.kind().is_trivia() && c.cast::<ast::Expr>().is_some())?;
    (args.kind() == SyntaxKind::Args
        && figure.kind() == SyntaxKind::FuncCall
        && callee(figure) == Some("figure")
        && first.range() == node.range())
    .then(|| figure.clone())
}

fn innermost<'a>(leaf: LinkedNode<'a>, cursor: usize) -> Option<(&'static str, LinkedNode<'a>)> {
    // A cursor on `#` or on a trailing label belongs to the neighbour.
    let start = match leaf.kind() {
        SyntaxKind::Hash => leaf.next_sibling()?,
        SyntaxKind::Label => leaf
            .prev_sibling()
            .filter(|n| matches!(kind_of(n), Some("figure" | "equation")))
            .unwrap_or_else(|| leaf.clone()),
        _ => leaf,
    };
    let mut node = Some(start);
    while let Some(current) = node {
        if let Some(kind) = kind_of(&current) {
            let current = enclosing_figure(&current).unwrap_or_else(|| current.clone());
            let range = current.range();
            // A label can sit a little after the element it belongs to.
            let labelled = matches!(kind, "figure" | "equation");
            if range.start.saturating_sub(1) <= cursor && (cursor <= range.end || labelled) {
                return Some((kind, current));
            }
        }
        node = current.parent().cloned();
    }
    None
}

/// Find the innermost editable construct at a UTF-16 offset.
pub fn detect(text: &str, offset: usize) -> Option<Detection> {
    let source = Source::detached(text);
    let cursor = byte_offset(&source, offset);
    let root = LinkedNode::new(source.root());
    let (name, node) = [Side::After, Side::Before]
        .into_iter()
        .filter_map(|side| innermost(root.leaf_at(cursor, side)?, cursor))
        .min_by_key(|(_, node)| node.range().len())?;

    let mut range = node.range();
    let mut in_code = false;
    if node.kind() == SyntaxKind::FuncCall {
        match node.prev_sibling_with_trivia() {
            Some(hash) if hash.kind() == SyntaxKind::Hash => range.start = hash.range().start,
            _ => in_code = true,
        }
    }
    // Take a following `<label>` along for the constructs that edit one.
    let takes_label = !in_code
        && (name == "equation"
            || (name == "figure" && callee(&node) == Some("figure")));
    if takes_label {
        let mut next = node.next_sibling_with_trivia();
        while let Some(sibling) = next {
            match sibling.kind() {
                SyntaxKind::Space => next = sibling.next_sibling_with_trivia(),
                SyntaxKind::Label => {
                    range.end = sibling.range().end;
                    break;
                }
                _ => break,
            }
        }
    }
    if cursor < range.start || cursor > range.end {
        return None;
    }

    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    // Remove the surrounding indentation; `render` adds it back.
    let snippet = text[range.clone()].replace(&format!("\n{indent}"), "\n");
    let (construct, problem) = match extract(&snippet, in_code) {
        Ok(construct) => (Some(construct), None),
        Err(problem) => (None, Some(problem)),
    };

    Some(Detection {
        from: source.lines().byte_to_utf16(range.start)?,
        to: source.lines().byte_to_utf16(range.end)?,
        name,
        in_code,
        indent,
        construct,
        problem,
    })
}

// ------------------------------------------------------------------ Reading

type Result<T> = std::result::Result<T, String>;

/// Parse a snippet that holds exactly one construct.
fn extract(snippet: &str, in_code: bool) -> Result<Construct> {
    let tree = if in_code {
        parse_code(snippet)
    } else {
        parse(snippet)
    };
    if tree.diagnosis().errors {
        return Err("it has a syntax error".into());
    }
    let root = LinkedNode::new(&tree);
    let mut children = root.children().filter(|c| !c.kind().is_trivia());
    let mut first = children.next().ok_or("it is empty")?;
    if first.kind() == SyntaxKind::Hash {
        first = children.next().ok_or("it is empty")?;
    }
    let kind = kind_of(&first).ok_or("it is not a supported construct")?;
    let mut label = String::new();
    for rest in children {
        match rest.cast::<ast::Label>() {
            Some(found) if label.is_empty() => label = found.get().to_string(),
            _ => return Err("there is more than one element".into()),
        }
    }

    let reader = Reader {
        text: snippet,
        keep: Vec::new(),
    };
    reader.read(kind, &first, label)
}

struct Args<'a> {
    positional: Vec<LinkedNode<'a>>,
    named: Vec<(String, LinkedNode<'a>)>,
}

impl<'a> Args<'a> {
    fn of(call: &LinkedNode<'a>) -> Result<Self> {
        let list = call
            .children()
            .find(|c| c.kind() == SyntaxKind::Args)
            .ok_or("the call has no arguments")?;
        let mut out = Args {
            positional: Vec::new(),
            named: Vec::new(),
        };
        for child in list.children() {
            if child.kind().is_trivia() {
                continue;
            }
            if child.kind() == SyntaxKind::Spread {
                return Err("it spreads arguments with `..`".into());
            }
            if let Some(named) = child.cast::<ast::Named>() {
                let value = child
                    .children()
                    .rfind(|c| !c.kind().is_trivia())
                    .ok_or("an argument has no value")?;
                out.named.push((named.name().get().to_string(), value));
            } else if child.cast::<ast::Expr>().is_some() {
                out.positional.push(child);
            }
        }
        Ok(out)
    }
}

struct Reader<'t> {
    text: &'t str,
    /// Ranges whose text survives editing, comments included.
    keep: Vec<std::ops::Range<usize>>,
}

impl Reader<'_> {
    /// A piece of source that will be written back unchanged.
    fn verbatim(&mut self, node: &LinkedNode) -> String {
        self.keep.push(node.range());
        self.text[node.range()].to_string()
    }

    /// The inside of a `[content]` block.
    fn inner(&mut self, node: &LinkedNode) -> Option<String> {
        (node.kind() == SyntaxKind::ContentBlock).then(|| {
            let range = node.range();
            self.keep.push(range.clone());
            self.text[range.start + 1..range.end - 1].to_string()
        })
    }

    fn extra(&mut self, name: &str, value: &LinkedNode) -> Extra {
        Extra {
            name: name.to_string(),
            value: self.verbatim(value),
        }
    }

    fn string(node: &LinkedNode) -> Option<String> {
        node.cast::<ast::Str>().map(|s| s.get().to_string())
    }

    fn cell(&mut self, node: &LinkedNode) -> Cell {
        match self.inner(node) {
            Some(text) => Cell { text, code: false },
            None => Cell {
                text: self.verbatim(node),
                code: true,
            },
        }
    }

    fn read(mut self, kind: &str, node: &LinkedNode, label: String) -> Result<Construct> {
        let construct = match (kind, node.kind()) {
            ("equation", _) => {
                let equation: ast::Equation = node.cast().ok_or("not an equation")?;
                let whole = &self.text[node.range()];
                let body = whole[1..whole.len() - 1].trim().to_string();
                self.keep.push(node.range());
                Construct::Equation {
                    body,
                    block: equation.block(),
                    label,
                }
            }
            ("code", _) => {
                let raw: ast::Raw = node.cast().ok_or("not a code block")?;
                self.keep.push(node.range());
                let lines: Vec<String> = raw.lines().map(|l| l.get().to_string()).collect();
                Construct::Code {
                    lang: raw.lang().map(|l| l.get().to_string()).unwrap_or_default(),
                    text: lines.join("\n"),
                    block: raw.block(),
                }
            }
            ("link", SyntaxKind::Link) => Construct::Link {
                target: self.verbatim(node),
                code: false,
                text: String::new(),
            },
            _ => {
                let name = callee(node).ok_or("not a function call")?.to_string();
                let args = Args::of(node)?;
                match kind {
                    "table" => self.table(&name, args)?,
                    "figure" => self.figure(&name, args, label)?,
                    "link" => self.link(args)?,
                    "quote" => self.quote(args)?,
                    "footnote" => self.footnote(args)?,
                    "outline" => self.outline(args)?,
                    "bibliography" => self.bibliography(args)?,
                    _ => return Err("it is not a supported construct".into()),
                }
            }
        };
        if self.loses_comments(node) {
            return Err("it contains comments that editing would remove".into());
        }
        Ok(construct)
    }

    fn loses_comments(&self, node: &LinkedNode) -> bool {
        let range = node.range();
        if matches!(
            node.kind(),
            SyntaxKind::LineComment | SyntaxKind::BlockComment
        ) {
            return !self
                .keep
                .iter()
                .any(|k| k.start <= range.start && range.end <= k.end);
        }
        node.children().any(|c| self.loses_comments(&c))
    }

    fn table(&mut self, name: &str, args: Args) -> Result<Construct> {
        let mut columns = vec!["auto".to_string()];
        let mut extra = Vec::new();
        for (key, value) in &args.named {
            if key == "columns" {
                columns = self.columns(value)?;
            } else {
                extra.push(self.extra(key, value));
            }
        }
        let width = columns.len();

        let mut header = Vec::new();
        let mut cells = Vec::new();
        for item in &args.positional {
            // `table.header(..)`, `table.cell(..)`, `table.hline()` and so on.
            let special = item
                .cast::<ast::FuncCall>()
                .and_then(|call| match call.callee() {
                    ast::Expr::FieldAccess(access) => match access.target() {
                        ast::Expr::Ident(target) if target.get() == name => {
                            Some(access.field().get().to_string())
                        }
                        _ => None,
                    },
                    _ => None,
                });
            match special.as_deref() {
                None => cells.push(self.cell(item)),
                Some("header") if header.is_empty() && cells.is_empty() => {
                    let inner = Args::of(item)?;
                    if !inner.named.is_empty() {
                        return Err("its header has options".into());
                    }
                    header = inner.positional.iter().map(|c| self.cell(c)).collect();
                }
                Some(other) => {
                    return Err(format!(
                        "it uses `{name}.{other}`, which the grid editor cannot show"
                    ))
                }
            }
        }
        if header.len() % width != 0 {
            return Err("its header does not fill whole rows".into());
        }
        while cells.len() % width != 0 {
            cells.push(Cell {
                text: String::new(),
                code: false,
            });
        }
        let rows = |cells: Vec<Cell>| cells.chunks(width).map(<[Cell]>::to_vec).collect();
        Ok(Construct::Table {
            grid: name == "grid",
            header: rows(header),
            rows: rows(cells),
            columns,
            extra,
        })
    }

    fn columns(&mut self, value: &LinkedNode) -> Result<Vec<String>> {
        const COMPUTED: &str = "its columns are computed by code";
        match value.cast::<ast::Expr>() {
            Some(ast::Expr::Int(count)) => match count.get() {
                n @ 1..=50 => Ok(vec!["auto".into(); n as usize]),
                _ => Err("it has an unusual number of columns".into()),
            },
            Some(ast::Expr::Auto(_) | ast::Expr::Numeric(_)) => Ok(vec![self.verbatim(value)]),
            Some(ast::Expr::Array(_)) => {
                let mut out = Vec::new();
                for item in value.children() {
                    if item.kind().is_trivia() || item.cast::<ast::Expr>().is_none() {
                        if item.kind() == SyntaxKind::Spread {
                            return Err(COMPUTED.into());
                        }
                        continue;
                    }
                    out.push(self.verbatim(&item));
                }
                if out.is_empty() {
                    return Err("it has no columns".into());
                }
                Ok(out)
            }
            _ => Err(COMPUTED.into()),
        }
    }

    /// `image("path", width: ..)`, when the path is a plain string.
    fn image(&mut self, node: &LinkedNode) -> Option<Image> {
        if node.kind() != SyntaxKind::FuncCall || callee(node) != Some("image") {
            return None;
        }
        let args = Args::of(node).ok()?;
        let [path] = args.positional.as_slice() else {
            return None;
        };
        let mut image = Image {
            path: Self::string(path)?,
            width: String::new(),
            extra: Vec::new(),
        };
        self.keep.push(path.range());
        for (key, value) in &args.named {
            if key == "width" {
                image.width = self.verbatim(value);
            } else {
                image.extra.push(self.extra(key, value));
            }
        }
        Some(image)
    }

    fn figure(&mut self, name: &str, args: Args, label: String) -> Result<Construct> {
        let mut caption = String::new();
        let mut extra = Vec::new();
        let mut content = String::new();
        let image;
        if name == "image" {
            let [path] = args.positional.as_slice() else {
                return Err("expected one image path".into());
            };
            let path = Self::string(path).ok_or("the image path is computed by code")?;
            let mut found = Image {
                path,
                width: String::new(),
                extra: Vec::new(),
            };
            for (key, value) in &args.named {
                if key == "width" {
                    found.width = self.verbatim(value);
                } else {
                    found.extra.push(self.extra(key, value));
                }
            }
            image = Some(found);
        } else {
            let [body] = args.positional.as_slice() else {
                return Err("expected the figure to have one body".into());
            };
            image = self.image(body);
            if image.is_none() {
                content = self.verbatim(body);
            }
            for (key, value) in &args.named {
                match (key.as_str(), self.inner(value)) {
                    ("caption", Some(text)) if !text.is_empty() => caption = text,
                    _ => extra.push(self.extra(key, value)),
                }
            }
        }
        Ok(Construct::Figure {
            figure: name == "figure",
            image,
            content,
            caption,
            label,
            extra,
        })
    }

    fn link(&mut self, args: Args) -> Result<Construct> {
        if !args.named.is_empty() || !(1..=2).contains(&args.positional.len()) {
            return Err("expected a destination and optional text".into());
        }
        let destination = &args.positional[0];
        let (target, code) = match Self::string(destination) {
            Some(url) => (url, false),
            None => (self.verbatim(destination), true),
        };
        self.keep.push(destination.range());
        let text = match args.positional.get(1) {
            Some(node) => self
                .inner(node)
                .unwrap_or_else(|| format!("#{}", self.verbatim(node))),
            None => String::new(),
        };
        Ok(Construct::Link { target, code, text })
    }

    /// The single `[content]` argument of a quote or footnote.
    fn body(&mut self, args: &Args) -> Result<String> {
        match args.positional.as_slice() {
            [only] => self
                .inner(only)
                .ok_or_else(|| "its body is not a `[...]` block".into()),
            _ => Err("expected one body".into()),
        }
    }

    fn quote(&mut self, args: Args) -> Result<Construct> {
        let body = self.body(&args)?;
        let (mut attribution, mut block, mut extra) = (String::new(), false, Vec::new());
        for (key, value) in &args.named {
            match key.as_str() {
                "block" if matches!(&self.text[value.range()], "true" | "false") => {
                    block = &self.text[value.range()] == "true";
                }
                "attribution" => match self.inner(value) {
                    Some(text) if !text.is_empty() => attribution = text,
                    _ => extra.push(self.extra(key, value)),
                },
                _ => extra.push(self.extra(key, value)),
            }
        }
        Ok(Construct::Quote {
            body,
            attribution,
            block,
            extra,
        })
    }

    fn footnote(&mut self, args: Args) -> Result<Construct> {
        let body = self.body(&args)?;
        let extra = args.named.iter().map(|(k, v)| self.extra(k, v)).collect();
        Ok(Construct::Footnote { body, extra })
    }

    fn outline(&mut self, args: Args) -> Result<Construct> {
        if !args.positional.is_empty() {
            return Err("it has unexpected arguments".into());
        }
        let (mut title, mut depth, mut extra) = (String::new(), String::new(), Vec::new());
        for (key, value) in &args.named {
            match key.as_str() {
                "depth" => depth = self.verbatim(value),
                "title" => match self.inner(value) {
                    Some(text) if !text.is_empty() => title = text,
                    _ => extra.push(self.extra(key, value)),
                },
                _ => extra.push(self.extra(key, value)),
            }
        }
        Ok(Construct::Outline { title, depth, extra })
    }

    fn bibliography(&mut self, args: Args) -> Result<Construct> {
        const SOURCES: &str = "its sources are computed by code";
        let [first] = args.positional.as_slice() else {
            return Err("expected the bibliography file".into());
        };
        let sources = match Self::string(first) {
            Some(path) => vec![path],
            None => {
                let array: ast::Array = first.cast().ok_or(SOURCES)?;
                array
                    .items()
                    .map(|item| match item {
                        ast::ArrayItem::Pos(ast::Expr::Str(s)) => Ok(s.get().to_string()),
                        _ => Err(SOURCES.to_string()),
                    })
                    .collect::<Result<_>>()?
            }
        };
        self.keep.push(first.range());
        let (mut title, mut style, mut full) = (String::new(), String::new(), false);
        let mut extra = Vec::new();
        for (key, value) in &args.named {
            match key.as_str() {
                "style" => match Self::string(value) {
                    Some(name) => style = name,
                    None => extra.push(self.extra(key, value)),
                },
                "full" if &self.text[value.range()] == "true" => full = true,
                "title" => match self.inner(value) {
                    Some(text) if !text.is_empty() => title = text,
                    _ => extra.push(self.extra(key, value)),
                },
                _ => extra.push(self.extra(key, value)),
            }
        }
        Ok(Construct::Bibliography {
            sources,
            title,
            style,
            full,
            extra,
        })
    }
}

// ------------------------------------------------------------------ Writing

/// A Typst string literal.
pub fn string_literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Named arguments: the form's values first, then the kept ones that the form
/// did not override.
fn arguments(set: Vec<(&str, Option<String>)>, extra: &[Extra]) -> Vec<String> {
    let set: Vec<(&str, String)> = set
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?)))
        .collect();
    let mut out: Vec<String> = set.iter().map(|(n, v)| format!("{n}: {v}")).collect();
    out.extend(
        extra
            .iter()
            .filter(|e| !set.iter().any(|(name, _)| *name == e.name))
            .map(|e| format!("{}: {}", e.name, e.value)),
    );
    out
}

fn content(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| format!("[{text}]"))
}

fn source_value(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.trim().to_string())
}

fn call_with_body(name: &str, args: Vec<String>, body: &str) -> String {
    if args.is_empty() {
        format!("{name}[{body}]")
    } else {
        format!("{name}({})[{body}]", args.join(", "))
    }
}

fn write_cell(cell: &Cell) -> String {
    match cell.code {
        true if !cell.text.trim().is_empty() => cell.text.clone(),
        _ => format!("[{}]", cell.text),
    }
}

fn write_image(image: &Image) -> String {
    let mut args = vec![string_literal(&image.path)];
    args.extend(arguments(
        vec![("width", source_value(&image.width))],
        &image.extra,
    ));
    format!("image({})", args.join(", "))
}

pub(crate) fn write_raw(lang: &str, text: &str, block: bool) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    if !block && lang.is_empty() && longest == 0 && !text.contains('\n') {
        return format!("`{text}`");
    }
    let fence = "`".repeat((longest + 1).max(3));
    if block {
        format!("{fence}{lang}\n{text}\n{fence}")
    } else if text.ends_with('`') {
        format!("{fence}{lang} {text} {fence}")
    } else {
        format!("{fence}{lang} {text}{fence}")
    }
}

/// The construct in the form that reading it back would produce.
fn normalize(construct: &Construct) -> Construct {
    let mut out = construct.clone();
    match &mut out {
        Construct::Equation { body, label, .. } => {
            *body = body.trim().to_string();
            *label = label.trim().to_string();
        }
        Construct::Table {
            columns,
            header,
            rows,
            ..
        } => {
            for column in columns.iter_mut() {
                *column = source_value(column).unwrap_or_else(|| "auto".into());
            }
            for cell in header.iter_mut().chain(rows.iter_mut()).flatten() {
                if cell.code && cell.text.trim().is_empty() {
                    *cell = Cell {
                        text: String::new(),
                        code: false,
                    };
                }
            }
        }
        Construct::Figure {
            figure,
            image,
            content,
            caption,
            label,
            extra,
        } => {
            *label = label.trim().to_string();
            if let Some(image) = image {
                image.width = image.width.trim().to_string();
                content.clear();
            }
            if !*figure {
                caption.clear();
                label.clear();
                extra.clear();
            }
        }
        Construct::Outline { depth, .. } => *depth = depth.trim().to_string(),
        _ => {}
    }
    // A value typed into a field replaces a kept argument of the same name.
    let drop = |extra: &mut Vec<Extra>, name: &str, used: bool| {
        if used {
            extra.retain(|e| e.name != name);
        }
    };
    match &mut out {
        Construct::Figure { caption, extra, .. } => drop(extra, "caption", !caption.is_empty()),
        Construct::Quote {
            attribution, extra, ..
        } => drop(extra, "attribution", !attribution.is_empty()),
        Construct::Outline { title, extra, .. } => drop(extra, "title", !title.is_empty()),
        Construct::Bibliography {
            title,
            style,
            extra,
            ..
        } => {
            drop(extra, "title", !title.is_empty());
            drop(extra, "style", !style.is_empty());
        }
        _ => {}
    }
    out
}

fn write(construct: &Construct) -> Result<(String, String)> {
    // (source without `#`, label)
    Ok(match construct {
        Construct::Equation { body, block, label } => {
            if body.is_empty() {
                return Err("The equation is empty.".into());
            }
            let source = if *block {
                format!("$ {body} $")
            } else {
                format!("${body}$")
            };
            (source, label.clone())
        }
        Construct::Table {
            grid,
            columns,
            header,
            rows,
            extra,
        } => {
            let width = columns.len();
            if width == 0 || header.iter().chain(rows).any(|r| r.len() != width) {
                return Err("Every row needs one cell per column.".into());
            }
            let name = if *grid { "grid" } else { "table" };
            let spec = if columns.iter().all(|c| c == "auto") {
                width.to_string()
            } else if width == 1 {
                format!("({},)", columns[0])
            } else {
                format!("({})", columns.join(", "))
            };
            let mut lines = arguments(vec![("columns", Some(spec))], extra);
            if !header.is_empty() {
                let cells: Vec<String> = header.iter().flatten().map(write_cell).collect();
                lines.push(format!("{name}.header({})", cells.join(", ")));
            }
            for row in rows {
                lines.push(row.iter().map(write_cell).collect::<Vec<_>>().join(", "));
            }
            (
                format!("{name}(\n  {},\n)", lines.join(",\n  ")),
                String::new(),
            )
        }
        Construct::Figure {
            figure,
            image,
            content: body,
            caption,
            label,
            extra,
        } => {
            if let Some(image) = image {
                if image.path.is_empty() {
                    return Err("Choose an image file.".into());
                }
            }
            let body = match image {
                Some(image) => write_image(image),
                None if !body.trim().is_empty() => body.clone(),
                None => return Err("The figure has no content.".into()),
            };
            if !*figure {
                (body, String::new())
            } else {
                let mut lines = vec![body];
                lines.extend(arguments(vec![("caption", content(caption))], extra));
                (
                    format!("figure(\n  {},\n)", lines.join(",\n  ")),
                    label.clone(),
                )
            }
        }
        Construct::Link { target, code, text } => {
            if target.trim().is_empty() {
                return Err("The link needs a destination.".into());
            }
            let destination = if *code {
                target.clone()
            } else {
                string_literal(target)
            };
            let mut source = format!("link({destination})");
            if let Some(text) = content(text) {
                source.push_str(&text);
            }
            (source, String::new())
        }
        Construct::Quote {
            body,
            attribution,
            block,
            extra,
        } => {
            let args = arguments(
                vec![
                    ("block", block.then(|| "true".to_string())),
                    ("attribution", content(attribution)),
                ],
                extra,
            );
            (call_with_body("quote", args, body), String::new())
        }
        Construct::Code { lang, text, block } => {
            if lang.chars().any(|c| c.is_whitespace() || c == '`') {
                return Err("The language name cannot contain spaces.".into());
            }
            (write_raw(lang, text, *block), String::new())
        }
        Construct::Footnote { body, extra } => (
            call_with_body("footnote", arguments(vec![], extra), body),
            String::new(),
        ),
        Construct::Outline { title, depth, extra } => {
            let args = arguments(
                vec![("title", content(title)), ("depth", source_value(depth))],
                extra,
            );
            (format!("outline({})", args.join(", ")), String::new())
        }
        Construct::Bibliography {
            sources,
            title,
            style,
            full,
            extra,
        } => {
            let files: Vec<String> = sources.iter().map(|s| string_literal(s)).collect();
            let files = match files.as_slice() {
                [] => return Err("Name a bibliography file.".into()),
                [only] => only.clone(),
                many => format!("({})", many.join(", ")),
            };
            let mut args = vec![files];
            args.extend(arguments(
                vec![
                    ("title", content(title)),
                    (
                        "style",
                        (!style.is_empty()).then(|| string_literal(style)),
                    ),
                    ("full", full.then(|| "true".to_string())),
                ],
                extra,
            ));
            (format!("bibliography({})", args.join(", ")), String::new())
        }
    })
}

/// Write a construct as Typst source.
///
/// Fails, with a message for the user, if a field would break the document:
/// for example an unbalanced bracket that would spill into other fields.
pub fn render(construct: &Construct, in_code: bool, indent: &str) -> Result<String> {
    let wanted = normalize(construct);
    let (mut source, label) = write(&wanted)?;

    let markup_syntax = matches!(wanted, Construct::Equation { .. } | Construct::Code { .. });
    if !in_code && !markup_syntax {
        source.insert(0, '#');
    }
    if !label.is_empty() {
        let valid = |c: char| c.is_alphanumeric() || "_-.:".contains(c);
        if !label.chars().all(valid) {
            return Err("A label may only use letters, digits, `_`, `-`, `.` and `:`.".into());
        }
        if in_code {
            return Err("A label cannot be added inside code.".into());
        }
        source.push_str(&format!(" <{label}>"));
    }

    // Read the result back to be sure each field stayed in its place.
    let mismatch = || {
        "A field would break the document. Check for unbalanced brackets, quotes or $ signs."
            .to_string()
    };
    let read = extract(&source, in_code).map_err(|_| mismatch())?;
    let faithful = match &wanted {
        // Typst trims indentation in code blocks, so compare only the kind.
        Construct::Code { .. } => matches!(read, Construct::Code { .. }),
        wanted => read == *wanted,
    };
    if !faithful {
        return Err(mismatch());
    }
    Ok(source.replace('\n', &format!("\n{indent}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detect with the cursor at `|`.
    fn at(marked: &str) -> Option<Detection> {
        let cursor = marked.find('|').expect("mark the cursor with |");
        let text = marked.replacen('|', "", 1);
        detect(&text, text[..cursor].encode_utf16().count())
    }

    /// Reading, writing and reading again must not change anything.
    fn round_trip(source: &str) -> (Construct, String) {
        let found = at(&format!("{}|{}", &source[..2], &source[2..])).expect(source);
        let construct = found.construct.unwrap_or_else(|| panic!("{source}: {:?}", found.problem));
        let written = render(&construct, found.in_code, &found.indent).expect(source);
        let again = detect(&written, 2).unwrap().construct.unwrap();
        assert_eq!(again, construct, "{written}");
        assert_eq!(render(&again, false, "").unwrap(), written);
        (construct, written)
    }

    #[test]
    fn round_trips_every_kind() {
        for source in [
            "$ sum_(i=0)^n i $ <eq:sum>",
            "$x^2$",
            "#table(\n  columns: (1fr, auto),\n  align: center,\n  table.header([*A*], [B]),\n  [1], \"two\",\n)",
            "#grid(columns: 2, gutter: 4pt, [a], [$x$])",
            "#figure(\n  image(\"a b.png\", width: 80%, alt: \"A\"),\n  caption: [Hi [there]],\n) <fig:a>",
            "#figure(table(columns: 1, [x]), caption: [T], kind: table)",
            "#image(\"x.svg\", height: 2cm)",
            "#link(\"https://typst.app/\")[Typst]",
            "#link(<intro>)[Back]",
            "#quote(block: true, attribution: [Plato])[Know thyself]",
            "#quote(attribution: <plato>)[x]",
            "```py\ndef f():\n    return 1\n```",
            "`inline`",
            "#footnote[See #link(\"x\")[there].]",
            "#outline(title: [Contents], depth: 2, indent: auto)",
            "#outline()",
            "#bibliography((\"a.bib\", \"b.yml\"), title: none, style: \"ieee\", full: true)",
        ] {
            round_trip(source);
        }
    }

    #[test]
    fn reads_table_fields_and_keeps_other_arguments() {
        let (construct, written) =
            round_trip("#table(columns: 2, stroke: none, table.header[A][B], [1], [2], [3])");
        let Construct::Table {
            columns,
            header,
            rows,
            extra,
            ..
        } = construct
        else {
            panic!()
        };
        assert_eq!(columns.len(), 2);
        assert_eq!(header.len(), 1);
        assert_eq!(rows.len(), 2, "a short last row is padded");
        assert_eq!(extra[0].value, "none");
        assert!(written.contains("stroke: none"), "{written}");
    }

    #[test]
    fn refuses_what_it_cannot_keep() {
        for (source, why) in [
            ("#table(columns: 2, table.cell(colspan: 2)[|x])", "table.cell"),
            ("#table(columns: 2, // note\n [|a], [b])", "comments"),
            ("#table(columns: (1fr,) * 3, [|a])", "computed"),
            ("#table(|..data)", "spreads"),
            ("#quote(\"|text\")", "block"),
        ] {
            let found = at(source).unwrap_or_else(|| panic!("{source}"));
            assert!(found.construct.is_none(), "{source}");
            let problem = found.problem.unwrap();
            assert!(problem.contains(why), "{source}: {problem}");
        }
    }

    #[test]
    fn keeps_comments_that_live_inside_a_cell() {
        let (_, written) = round_trip("#table(columns: 1, [a /* kept */ b])");
        assert!(written.contains("/* kept */"));
    }

    #[test]
    fn picks_the_innermost_construct() {
        assert_eq!(at("#table(columns: 1, [$a|$])").unwrap().name, "equation");
        assert_eq!(at("#figure(table(columns: 1, [|a]))").unwrap().name, "table");
        // The image of a figure edits the whole figure.
        let found = at("#figure(image(\"|a.png\"), caption: [c])").unwrap();
        assert!(matches!(
            found.construct,
            Some(Construct::Figure { figure: true, .. })
        ));
        assert!(at("plain |text").is_none());
        assert!(at("$x$ and | more").is_none());
    }

    #[test]
    fn finds_constructs_from_the_hash_and_the_label() {
        assert_eq!(at("a |#link(\"x\") b").unwrap().name, "link");
        let found = at("$ x $ <e|q>").unwrap();
        assert_eq!((found.name, found.from, found.to), ("equation", 0, 10));
    }

    #[test]
    fn reports_utf16_ranges_and_indentation() {
        let text = "😀\n  #footnote[a\n  b]";
        let found = detect(text, 8).unwrap();
        assert_eq!((found.from, found.to), (5, text.encode_utf16().count()));
        assert_eq!(found.indent, "  ");
        // Writing back unchanged gives the original, with no extra indentation.
        let written = render(&found.construct.unwrap(), false, &found.indent).unwrap();
        assert_eq!(written, "#footnote[a\n  b]");
    }

    #[test]
    fn writes_calls_inside_code_without_a_hash() {
        let found = at("#figure(table(columns: 1, [|a]))").unwrap();
        assert!(found.in_code);
        let written = render(&found.construct.unwrap(), true, "").unwrap();
        assert!(written.starts_with("table("), "{written}");
    }

    #[test]
    fn rejects_fields_that_would_spill_over() {
        let table = |text: &str| Construct::Table {
            grid: false,
            columns: vec!["auto".into()],
            header: vec![],
            rows: vec![vec![Cell {
                text: text.into(),
                code: false,
            }]],
            extra: vec![],
        };
        assert!(render(&table("a [b] c"), false, "").is_ok());
        for broken in ["a], [b", "open [", "$x", "x] #evil["] {
            assert!(render(&table(broken), false, "").is_err(), "{broken}");
        }
        let equation = Construct::Equation {
            body: "a $ b".into(),
            block: false,
            label: String::new(),
        };
        assert!(render(&equation, false, "").is_err());
    }

    #[test]
    fn escapes_strings_and_fences_code() {
        let link = Construct::Link {
            target: "a\"b\\c".into(),
            code: false,
            text: String::new(),
        };
        assert_eq!(render(&link, false, "").unwrap(), r#"#link("a\"b\\c")"#);
        let code = Construct::Code {
            lang: "md".into(),
            text: "```\nx\n```".into(),
            block: true,
        };
        assert!(render(&code, false, "").unwrap().starts_with("````md\n"));
    }

    #[test]
    fn a_typed_field_replaces_a_kept_argument() {
        let mut outline = at("#outline(title: |none)").unwrap().construct.unwrap();
        assert_eq!(render(&outline, false, "").unwrap(), "#outline(title: none)");
        if let Construct::Outline { title, .. } = &mut outline {
            *title = "Index".into();
        }
        assert_eq!(render(&outline, false, "").unwrap(), "#outline(title: [Index])");
    }
}
