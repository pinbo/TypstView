//! Reading bibliography files, and adding entries to BibTeX ones.
//!
//! Entries are only ever appended, so existing text, comments and formatting in
//! a `.bib` file are left exactly as they were.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use typst::syntax::VirtualPath;

use crate::files;

#[derive(Debug, Serialize)]
pub struct BibEntry {
    pub key: String,
    pub kind: String,
    pub title: String,
    pub author: String,
    pub year: String,
}

#[derive(Debug, Deserialize)]
pub struct NewEntry {
    pub kind: String,
    pub key: String,
    /// BibTeX field names and values; empty values are skipped.
    pub fields: Vec<(String, String)>,
}

/// Find `file` the way Typst would: inside the folder of the document.
pub fn resolve(document: &Path, file: &str) -> Result<PathBuf, String> {
    let root = document.parent().ok_or("the document has no folder")?;
    VirtualPath::new(file)
        .ok()
        .and_then(|path| path.realize(root).ok())
        .ok_or_else(|| format!("{file} must be inside the document's folder"))
}

fn parse(path: &Path, text: &str) -> Result<hayagriva::Library, String> {
    let name = path.display();
    match path.extension().and_then(|e| e.to_str()) {
        Some("bib") => hayagriva::io::from_biblatex_str(text).map_err(|errors| {
            let first = errors.first().map(ToString::to_string).unwrap_or_default();
            format!("{name} could not be read: {first}")
        }),
        Some("yml" | "yaml") => hayagriva::io::from_yaml_str(text)
            .map_err(|e| format!("{name} could not be read: {e}")),
        _ => Err(format!("{name} is not a .bib or .yml file")),
    }
}

/// The entries of a bibliography file. A file that does not exist yet is empty.
pub fn list(path: &Path) -> Result<Vec<BibEntry>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let library = parse(path, &files::read_text(path)?)?;
    Ok(library
        .iter()
        .map(|entry| BibEntry {
            key: entry.key().to_string(),
            kind: format!("{:?}", entry.entry_type()).to_lowercase(),
            title: entry.title().map(ToString::to_string).unwrap_or_default(),
            author: entry
                .authors()
                .map(|all| {
                    all.iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default(),
            year: entry
                .date_any()
                .map(|d| d.year.to_string())
                .unwrap_or_default(),
        })
        .collect())
}

fn balanced(value: &str) -> bool {
    let mut depth = 0i32;
    let mut escaped = false;
    for c in value.chars() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0 && !escaped
}

/// Append an entry to a `.bib` file, creating the file if needed.
pub fn add(path: &Path, entry: &NewEntry) -> Result<(), String> {
    if path.extension().and_then(|e| e.to_str()) != Some("bib") {
        return Err("Entries can only be added to .bib files.".into());
    }
    let key = entry.key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || "_-.:".contains(c)) {
        return Err("The key may only use letters, digits, `_`, `-`, `.` and `:`.".into());
    }
    if entry.kind.is_empty() || !entry.kind.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err("Choose an entry type.".into());
    }

    let existing = if path.exists() {
        files::read_text(path)?
    } else {
        String::new()
    };
    // Refuse to touch a file that is already broken.
    if parse(path, &existing)?.keys().any(|k| k == key) {
        return Err(format!("An entry named `{key}` already exists."));
    }

    let mut block = format!("@{}{{{key},\n", entry.kind);
    for (name, value) in &entry.fields {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if !name.chars().all(|c| c.is_ascii_alphabetic()) || !balanced(value) {
            return Err(format!("The {name} field has unbalanced braces."));
        }
        block.push_str(&format!("  {name} = {{{value}}},\n"));
    }
    block.push_str("}\n");

    let mut text = existing;
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str(&block);
    if !parse(path, &text)?.keys().any(|k| k == key) {
        return Err("The entry could not be read back; nothing was saved.".into());
    }
    files::write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(key: &str, title: &str) -> NewEntry {
        NewEntry {
            kind: "book".into(),
            key: key.into(),
            fields: vec![
                ("title".into(), title.into()),
                ("author".into(), "Knuth, Donald".into()),
                ("year".into(), "1984".into()),
                ("publisher".into(), String::new()),
            ],
        }
    }

    #[test]
    fn appends_without_rewriting_what_is_there() {
        let dir = std::env::temp_dir().join(format!("typstview-bib-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("refs.bib");
        assert!(list(&path).unwrap().is_empty());

        let original = "% my notes\n@book{old,\n  title={Old},\n  year={1970}}";
        fs::write(&path, original).unwrap();
        add(&path, &entry("tex", "The {TeX}book")).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with(original), "{text}");

        let entries = list(&path).unwrap();
        assert_eq!(entries.len(), 2);
        let added = entries.iter().find(|e| e.key == "tex").unwrap();
        assert_eq!((added.year.as_str(), added.author.as_str()), ("1984", "Knuth"));

        let before = fs::read_to_string(&path).unwrap();
        assert!(add(&path, &entry("tex", "Again")).unwrap_err().contains("already"));
        assert!(add(&path, &entry("bad", "a } b")).unwrap_err().contains("braces"));
        assert!(add(&path, &entry("no spaces", "x")).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), before);

        fs::write(&path, "@book{broken,").unwrap();
        assert!(add(&path, &entry("x", "y")).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "@book{broken,");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stays_inside_the_document_folder() {
        let doc = Path::new("/project/main.typ");
        assert_eq!(resolve(doc, "refs/a.bib").unwrap(), Path::new("/project/refs/a.bib"));
        assert!(resolve(doc, "../outside.bib").is_err());
    }
}
