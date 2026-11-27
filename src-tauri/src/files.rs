//! Reading and writing documents.

use std::fs;
use std::io::Write;
use std::path::Path;

pub fn read_text(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8 text", path.display()))
}

/// Write through a temporary file so a crash or full disk cannot leave a
/// half-written document behind.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());

    // Write through symlinks instead of replacing the link itself.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target
        .file_name()
        .ok_or_else(|| format!("{} is not a file path", path.display()))?;
    let temp = target.with_file_name(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));

    let result = (|| {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        if let Ok(meta) = fs::metadata(&target) {
            if meta.permissions().readonly() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "file is read-only",
                ));
            }
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &target)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(fail)
}

/// `target` written relative to the document's folder, with `/` separators.
/// Typst cannot reach files outside that folder.
pub fn relative_to_document(document: &Path, target: &Path) -> Result<String, String> {
    let real = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let folder = document.parent().ok_or("the document has no folder")?;
    let relative = real(target)
        .strip_prefix(real(folder))
        .map(Path::to_path_buf)
        .map_err(|_| {
            format!(
                "Typst can only use files inside the document's folder ({}). Copy the file there first.",
                folder.display()
            )
        })?;
    let parts: Vec<_> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_contents_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("typstview-files-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.typ");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(read_text(&path).unwrap(), "two");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn makes_paths_relative_to_the_document() {
        let doc = Path::new("/p/main.typ");
        let inside = relative_to_document(doc, Path::new("/p/img/a b.png"));
        assert_eq!(inside.unwrap(), "img/a b.png");
        assert!(relative_to_document(doc, Path::new("/other/a.png")).is_err());
    }

    #[test]
    fn rejects_binary_files() {
        let path = std::env::temp_dir().join(format!("typstview-bin-{}", std::process::id()));
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert!(read_text(&path).unwrap_err().contains("not UTF-8"));
        fs::remove_file(path).unwrap();
    }
}
