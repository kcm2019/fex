//! Filesystem operations: directory listing, previews, and file management.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub is_hidden: bool,
}

/// List the entries of a directory. Returns an empty vec if it can't be read.
pub fn list_dir(dir: &Path) -> Vec<Entry> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for dir_entry in read_dir.flatten() {
        let path = dir_entry.path();
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        let metadata = dir_entry.metadata().ok();
        let is_hidden = name.starts_with('.') || hidden_attr(&metadata);
        entries.push(Entry {
            path,
            name,
            is_dir: metadata.as_ref().is_some_and(|m| m.is_dir()),
            size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: metadata.as_ref().and_then(|m| m.modified().ok()),
            is_hidden,
        });
    }
    entries
}

/// True when the OS marks the file hidden (Windows file attribute; on
/// Unix-likes hidden-ness is just the dot-prefix, handled by the caller).
#[cfg(target_os = "windows")]
fn hidden_attr(metadata: &Option<std::fs::Metadata>) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    metadata
        .as_ref()
        .is_some_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(target_os = "windows"))]
fn hidden_attr(_metadata: &Option<std::fs::Metadata>) -> bool {
    false
}

const PREVIEW_MAX_BYTES: usize = 8192;
const PREVIEW_MAX_LINES: usize = 120;
/// What kind of preview a path gets, decided by extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewKind {
    Pdf,
    Markdown,
    Csv,
    Other,
}

fn preview_kind(path: &Path) -> PreviewKind {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "pdf" => PreviewKind::Pdf,
        "md" | "markdown" => PreviewKind::Markdown,
        "csv" => PreviewKind::Csv,
        _ => PreviewKind::Other,
    }
}

/// A preview of a file or directory for the preview pane.
#[derive(Debug, Clone)]
pub enum Preview {
    /// Plain text, already limited to PREVIEW_MAX_LINES lines.
    Text(String),
    /// Raw markdown source, byte-limited; styled at render time.
    Markdown(String),
    /// Directory summary, e.g. "12 items".
    Dir(String),
    /// First rows of a CSV, for the column preview.
    Csv(Vec<Vec<String>>),
}

/// Read the first PREVIEW_MAX_BYTES of a file. None when it can't be read.
fn read_head(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut buf = vec![0u8; PREVIEW_MAX_BYTES];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(buf)
}

fn first_lines(text: &str) -> String {
    text.lines()
        .take(PREVIEW_MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a preview of a file, or an item count for directories.
/// PDFs become extracted text, markdown stays as source for styled
/// rendering; images get the generic text/binary treatment and anything
/// binary stays "[binary file]".
pub fn read_preview(path: &Path, is_dir: bool) -> Preview {
    if is_dir {
        let count = fs::read_dir(path).map(|rd| rd.count()).unwrap_or(0);
        let s = if count == 1 {
            String::from("1 item")
        } else {
            format!("{count} items")
        };
        return Preview::Dir(s);
    }
    match preview_kind(path) {
        PreviewKind::Pdf => match pdf_extract::extract_text(path) {
            Ok(text) => {
                let text = first_lines(&text);
                if text.trim().is_empty() {
                    Preview::Text(String::from("[no extractable text]"))
                } else {
                    Preview::Text(text)
                }
            }
            Err(_) => Preview::Text(String::from("[cannot read file]")),
        },
        kind => {
            let Some(buf) = read_head(path) else {
                return Preview::Text(String::from("[cannot read file]"));
            };
            if buf.is_empty() {
                return Preview::Text(String::from("[empty file]"));
            }
            if buf.contains(&0) {
                return Preview::Text(String::from("[binary file]"));
            }
            let text = first_lines(&String::from_utf8_lossy(&buf));
            if kind == PreviewKind::Markdown {
                Preview::Markdown(text)
            } else if kind == PreviewKind::Csv {
                Preview::Csv(parse_csv_preview(&text))
            } else {
                Preview::Text(text)
            }
        }
    }
}

/// Parse the first rows of CSV text for the preview pane.
fn parse_csv_preview(text: &str) -> Vec<Vec<String>> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes())
        .records()
        .take(15)
        .filter_map(|r| r.ok())
        .map(|rec| {
            rec.iter()
                .take(8)
                .map(|s| {
                    let s: String = s.chars().take(24).collect();
                    s
                })
                .collect()
        })
        .collect()
}

/// A single search hit, from filename or deep (content) search.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Set for deep-search hits: 1-based line number and a trimmed snippet.
    pub line_no: Option<usize>,
    pub snippet: Option<String>,
}

const MAX_SEARCH_RESULTS: usize = 300;
const MAX_FILES_WALKED: usize = 20_000;
const MAX_FILE_READ: u64 = 2_000_000; // deep search skips bigger files
const MAX_LINES_PER_FILE: usize = 20_000;

/// Walk `root` recursively, yielding entries. Stops after MAX_FILES_WALKED
/// to keep huge trees responsive. Unreadable directories are skipped.
fn walk(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for de in rd.flatten() {
            if out.len() >= MAX_FILES_WALKED {
                return out;
            }
            let path = de.path();
            let name = de.file_name().to_string_lossy().into_owned();
            let metadata = de.metadata().ok();
            let is_dir = metadata.as_ref().is_some_and(|m| m.is_dir());
            out.push(Entry {
                path: path.clone(),
                name,
                is_dir,
                size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: metadata.as_ref().and_then(|m| m.modified().ok()),
                is_hidden: false,
            });
            if is_dir {
                stack.push(path);
            }
        }
    }
    out
}

/// Case-insensitive substring search over file and directory names under
/// `root`. Returns at most MAX_SEARCH_RESULTS hits.
pub fn search_names(root: &Path, query: &str) -> Vec<SearchResult> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    walk(root)
        .into_iter()
        .filter(|e| e.name.to_lowercase().contains(&needle))
        .take(MAX_SEARCH_RESULTS)
        .map(|e| SearchResult {
            path: e.path,
            is_dir: e.is_dir,
            line_no: None,
            snippet: None,
        })
        .collect()
}

/// Case-insensitive substring search over file *contents* under `root`.
/// Skips directories, binaries (NUL byte in the head), and oversized files.
/// Returns at most MAX_SEARCH_RESULTS hits as path:line with a snippet.
pub fn search_contents(root: &Path, query: &str) -> Vec<SearchResult> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    'files: for entry in walk(root) {
        if hits.len() >= MAX_SEARCH_RESULTS {
            break;
        }
        if entry.is_dir || entry.size > MAX_FILE_READ {
            continue;
        }
        let Ok(file) = File::open(&entry.path) else {
            continue;
        };
        let mut buf = Vec::new();
        // Read at most MAX_FILE_READ bytes.
        if file.take(MAX_FILE_READ).read_to_end(&mut buf).is_err() {
            continue;
        }
        if buf.contains(&0) {
            continue; // binary
        }
        let text = String::from_utf8_lossy(&buf);
        for (i, line) in text.lines().enumerate().take(MAX_LINES_PER_FILE) {
            if line.to_lowercase().contains(&needle) {
                let snippet: String = line.trim().chars().take(100).collect();
                hits.push(SearchResult {
                    path: entry.path.clone(),
                    is_dir: false,
                    line_no: Some(i + 1),
                    snippet: Some(snippet),
                });
                if hits.len() >= MAX_SEARCH_RESULTS {
                    break 'files;
                }
            }
        }
    }
    hits
}

/// Create an empty file inside `dir`. Errors if the name is taken.
pub fn create_file(dir: &Path, name: &str) -> io::Result<()> {
    let path = dir.join(name);
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "already exists",
        ));
    }
    File::create(path)?;
    Ok(())
}

/// Create a directory inside `dir`. Errors if the name is taken.
pub fn create_dir(dir: &Path, name: &str) -> io::Result<()> {
    let path = dir.join(name);
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "already exists",
        ));
    }
    fs::create_dir(path)?;
    Ok(())
}

/// Rename an entry, keeping it in the same directory. Errors if taken.
pub fn rename_entry(path: &Path, new_name: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no parent directory"))?;
    let dest = parent.join(new_name);
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "already exists",
        ));
    }
    fs::rename(path, dest)?;
    Ok(())
}

/// Delete a file or directory (directories are removed recursively).
pub fn delete_entry(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Generate a non-colliding destination name inside `dir`,
/// e.g. "notes copy.txt", then "notes copy 2.txt".
pub fn unique_name(dir: &Path, name: &str) -> PathBuf {
    let direct = dir.join(name);
    if !direct.exists() {
        return direct;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.to_string(), String::new()),
    };
    let mut candidate = dir.join(format!("{stem} copy{ext}"));
    if !candidate.exists() {
        return candidate;
    }
    let mut i = 2u32;
    loop {
        candidate = dir.join(format!("{stem} copy {i}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        fs::copy(src, dst)?;
    }
    Ok(())
}

fn entry_name(path: &Path) -> io::Result<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "bad file name"))
}

/// Copy a file or directory into `dst_dir`, avoiding name collisions.
/// Returns the path it was copied to.
pub fn copy_entry(src: &Path, dst_dir: &Path) -> io::Result<PathBuf> {
    let dst = unique_name(dst_dir, &entry_name(src)?);
    copy_recursive(src, &dst)?;
    Ok(dst)
}

/// Move a file or directory into `dst_dir`, avoiding name collisions.
/// Falls back to copy+delete when a plain rename can't do it.
/// Returns the path it was moved to.
pub fn move_entry(src: &Path, dst_dir: &Path) -> io::Result<PathBuf> {
    let dst = unique_name(dst_dir, &entry_name(src)?);
    match fs::rename(src, &dst) {
        Ok(()) => Ok(dst),
        Err(_) => {
            copy_recursive(src, &dst)?;
            delete_entry(src)?;
            Ok(dst)
        }
    }
}

/// Copy text to the OS clipboard. Best-effort: returns false when no
/// clipboard tool is available; callers keep working without it.
pub fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        pipe_to_clipboard("pbcopy", &[], text)
    }
    #[cfg(target_os = "windows")]
    {
        pipe_to_clipboard("clip", &[], text)
    }
    #[cfg(target_os = "linux")]
    {
        // Wayland (wl-copy) and X11 (xclip/xsel), first one that exists wins.
        pipe_to_clipboard("wl-copy", &[], text)
            || pipe_to_clipboard("xclip", &["-selection", "clipboard"], text)
            || pipe_to_clipboard("xsel", &["--clipboard", "--input"], text)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = text;
        false
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn pipe_to_clipboard(cmd: &str, args: &[&str], text: &str) -> bool {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = match Command::new(cmd).args(args).stdin(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let wrote = child
        .stdin
        .as_mut()
        .is_some_and(|s| s.write_all(text.as_bytes()).is_ok());
    wrote && child.wait().is_ok_and(|s| s.success())
}

/// Read text from the OS clipboard. None when unavailable.
pub fn read_clipboard() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("pbpaste").output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).ok())
            .flatten()
    }
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-Clipboard"])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).ok())
            .flatten()
    }
    #[cfg(target_os = "linux")]
    {
        for (cmd, args) in [
            ("wl-paste", &["-n"][..]),
            ("xclip", &["-selection", "clipboard", "-o"][..]),
            ("xsel", &["--clipboard", "--output"][..]),
        ] {
            if let Ok(out) = Command::new(cmd).args(args).output() {
                if out.status.success() {
                    if let Ok(s) = String::from_utf8(out.stdout) {
                        return Some(s);
                    }
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    None
}

/// Open a file with the OS default application.
pub fn open_with_default(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        // `start` takes the first quoted argument as a window title, so pass
        // an empty one before the path.
        Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported platform",
        ))
    }
}

/// Show a file or folder in the OS file explorer: Finder on macOS (`open -R`
/// reveals a file and selects it; `open` opens a folder), Explorer on
/// Windows (`/select,` highlights a file), the default file manager on Linux
/// (`xdg-open` opens the folder itself — or the parent folder for a file,
/// since `xdg-open` on a file would launch its default app instead).
pub fn reveal_in_explorer(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        if path.is_dir() {
            Command::new("open").arg(path).spawn().map(|_| ())
        } else {
            Command::new("open").arg("-R").arg(path).spawn().map(|_| ())
        }
    }
    #[cfg(target_os = "windows")]
    {
        if path.is_dir() {
            Command::new("explorer").arg(path).spawn().map(|_| ())
        } else {
            Command::new("explorer")
                .arg(format!("/select,{}", path.to_string_lossy()))
                .spawn()
                .map(|_| ())
        }
    }
    #[cfg(target_os = "linux")]
    {
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        Command::new("xdg-open").arg(dir).spawn().map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("fex-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_rename_delete_file() {
        let dir = tmpdir("crud");
        create_file(&dir, "a.txt").unwrap();
        assert!(dir.join("a.txt").exists());
        assert!(create_file(&dir, "a.txt").is_err());
        rename_entry(&dir.join("a.txt"), "b.txt").unwrap();
        assert!(!dir.join("a.txt").exists());
        assert!(dir.join("b.txt").exists());
        delete_entry(&dir.join("b.txt")).unwrap();
        assert!(!dir.join("b.txt").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unique_name_avoids_collisions() {
        let dir = tmpdir("unique");
        create_file(&dir, "a.txt").unwrap();
        let p1 = unique_name(&dir, "a.txt");
        assert_eq!(p1.file_name().unwrap(), "a copy.txt");
        create_file(&dir, "a copy.txt").unwrap();
        let p2 = unique_name(&dir, "a.txt");
        assert_eq!(p2.file_name().unwrap(), "a copy 2.txt");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn copy_and_move_roundtrip() {
        let dir = tmpdir("copymove");
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        create_file(&sub, "x.txt").unwrap();
        let copied = copy_entry(&sub, &dir).unwrap();
        assert!(copied.join("x.txt").exists());
        assert!(sub.exists(), "copy must not remove the source");
        let moved = move_entry(&copied, &dir).unwrap();
        assert!(moved.exists());
        assert!(!copied.exists(), "move must remove the source");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn preview_handles_text_binary_and_dirs() {
        let dir = tmpdir("preview");
        fs::write(dir.join("t.txt"), "hello\nworld\n").unwrap();
        match read_preview(&dir.join("t.txt"), false) {
            Preview::Text(s) => assert!(s.contains("hello")),
            other => panic!("expected text preview, got {other:?}"),
        }
        fs::write(dir.join("b.bin"), [0u8, 1, 2, 3]).unwrap();
        match read_preview(&dir.join("b.bin"), false) {
            Preview::Text(s) => assert_eq!(s, "[binary file]"),
            other => panic!("expected text preview, got {other:?}"),
        }
        match read_preview(&dir, true) {
            Preview::Dir(s) => assert_eq!(s, "2 items"),
            other => panic!("expected dir preview, got {other:?}"),
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn image_files_get_generic_preview() {
        // Image previews were removed: images get the generic
        // text/binary treatment like any other file.
        let dir = tmpdir("imgpreview");
        fs::write(dir.join("a.png"), b"not a png").unwrap();
        assert!(matches!(
            read_preview(&dir.join("a.png"), false),
            Preview::Text(_)
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn markdown_preview_detected() {
        let dir = tmpdir("mdpreview");
        fs::write(dir.join("a.md"), "# Title\n\nhello\n").unwrap();
        match read_preview(&dir.join("a.md"), false) {
            Preview::Markdown(s) => assert!(s.contains("# Title")),
            other => panic!("expected markdown preview, got {other:?}"),
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Build a minimal one-page PDF containing the text "Hello PDF".
    fn tiny_pdf() -> Vec<u8> {
        let stream = b"BT /F1 12 Tf 10 180 Td (Hello PDF) Tj ET\n";
        let mut obj4 = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        obj4.extend_from_slice(stream);
        obj4.extend_from_slice(b"endstream");
        let objs: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
            obj4,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, obj) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            pdf.extend_from_slice(obj);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref_at = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objs.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
                objs.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    #[test]
    fn pdf_text_extracted() {
        let dir = tmpdir("pdfpreview");
        let p = dir.join("a.pdf");
        fs::write(&p, tiny_pdf()).unwrap();
        match read_preview(&p, false) {
            Preview::Text(s) => assert!(s.contains("Hello PDF"), "got: {s:?}"),
            other => panic!("expected text preview, got {other:?}"),
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn search_names_and_contents() {
        let dir = tmpdir("search");
        fs::create_dir(dir.join("docs")).unwrap();
        fs::write(
            dir.join("docs").join("notes.txt"),
            "hello world\nsecond line\n",
        )
        .unwrap();
        fs::write(dir.join("readme.md"), "nothing here\n").unwrap();
        fs::write(dir.join("docs").join("image.png"), [0u8, 1, 2, 3]).unwrap(); // binary: skipped

        let names = search_names(&dir, "note");
        assert_eq!(names.len(), 1);
        assert!(names[0].path.ends_with("notes.txt"));

        let deep = search_contents(&dir, "second");
        assert_eq!(deep.len(), 1);
        assert_eq!(deep[0].line_no, Some(2));
        assert!(deep[0].snippet.as_ref().unwrap().contains("second line"));

        // Binary files are skipped by deep search.
        let deep_bin = search_contents(&dir, "notes");
        assert!(deep_bin.is_empty() || deep_bin.iter().all(|r| !r.path.ends_with("image.png")));

        assert!(search_names(&dir, "").is_empty());
        assert!(search_contents(&dir, "").is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn list_dir_marks_hidden_and_dirs() {
        let dir = tmpdir("list");
        fs::create_dir(dir.join("mydir")).unwrap();
        create_file(&dir, ".hidden").unwrap();
        let entries = list_dir(&dir);
        let d = entries.iter().find(|e| e.name == "mydir").unwrap();
        assert!(d.is_dir && !d.is_hidden);
        let h = entries.iter().find(|e| e.name == ".hidden").unwrap();
        assert!(!h.is_dir && h.is_hidden);
        fs::remove_dir_all(&dir).unwrap();
    }
}
