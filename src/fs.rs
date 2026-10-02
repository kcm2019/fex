//! Filesystem operations: directory listing, previews, and file management.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
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

/// True when `path` sits inside a hidden directory (any ancestor directory
/// under `root` whose name starts with `.`, e.g. `.git`). `root` itself is
/// allowed to be hidden; only what lies below it is checked.
fn in_hidden_dir(root: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let parent = rel.parent().unwrap_or_else(|| Path::new(""));
    parent.components().any(
        |c| matches!(c, std::path::Component::Normal(os) if os.to_string_lossy().starts_with('.')),
    )
}

/// Case-insensitive substring search over file *contents* under `root`.
/// Skips directories, binaries (NUL byte in the head), oversized files, and
/// anything inside a hidden directory (`.git` and friends). Returns at most
/// `limit` hits as path:line with a snippet.
pub fn search_contents_limit(root: &Path, query: &str, limit: usize) -> Vec<SearchResult> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    'files: for entry in walk(root) {
        if hits.len() >= limit {
            break;
        }
        if entry.is_dir || entry.size > MAX_FILE_READ {
            continue;
        }
        if in_hidden_dir(root, &entry.path) {
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
                if hits.len() >= limit {
                    break 'files;
                }
            }
        }
    }
    hits
}

/// Case-insensitive content search with the default 300-hit cap.
/// See `search_contents_limit`.
pub fn search_contents(root: &Path, query: &str) -> Vec<SearchResult> {
    search_contents_limit(root, query, MAX_SEARCH_RESULTS)
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
    // Office documents are created as valid empty files, not 0-byte blobs,
    // so they open straight away (in fex and in Word/Excel/PowerPoint).
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "xlsx" => {
            let book = umya_spreadsheet::new_file();
            umya_spreadsheet::writer::xlsx::write(&book, &path)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            Ok(())
        }
        "docx" => write_ooxml(&path, &docx_parts()),
        "pptx" => write_ooxml(&path, &pptx_parts()),
        _ => {
            File::create(path)?;
            Ok(())
        }
    }
}

/// Write a minimal OOXML package (a zip of XML parts): enough for Word /
/// PowerPoint to open the file as a blank document.
fn write_ooxml(path: &Path, parts: &[(&str, &str)]) -> io::Result<()> {
    use std::io::Write as _;
    let f = File::create(path)?;
    let mut w = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default();
    for (name, data) in parts {
        w.start_file(*name, opts)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        w.write_all(data.as_bytes())?;
    }
    w.finish()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    Ok(())
}

fn docx_parts() -> [(&'static str, &'static str); 3] {
    [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/document.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p/></w:body></w:document>"#,
        ),
    ]
}

fn pptx_parts() -> [(&'static str, &'static str); 5] {
    [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld></p:sld>"#,
        ),
    ]
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
    // Always open a folder: the selected directory itself, or the parent
    // directory when a file is selected. The per-file reveal/select flags
    // (`open -R`, `explorer /select,`) proved unreliable — on some machines
    // they landed in the Documents folder instead of on the file.
    let dir = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(dir).spawn().map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(dir).spawn().map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(dir).spawn().map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = dir;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported platform",
        ))
    }
}

// -- archive extraction -----------------------------------------------------

/// Archive formats fex can extract, detected by file-name suffix.
fn archive_kind(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some("targz")
    } else if name.ends_with(".zip") {
        Some("zip")
    } else if name.ends_with(".7z") {
        Some("7z")
    } else if name.ends_with(".tar") {
        Some("tar")
    } else {
        None
    }
}

/// True when the path names a supported archive.
pub fn is_archive(path: &Path) -> bool {
    archive_kind(path).is_some()
}

/// File stem without the archive suffix: "photos.tar.gz" -> "photos".
pub fn archive_stem(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_lowercase();
    for suffix in [".tar.gz", ".tgz", ".zip", ".7z", ".tar"] {
        if lower.ends_with(suffix) {
            return Some(name[..name.len() - suffix.len()].to_string());
        }
    }
    None
}

/// Join an archive entry path onto `dest`, rejecting absolute paths and
/// `..` escapes (zip-slip). Returns None for unsafe entries.
fn safe_join(dest: &Path, entry: &Path) -> Option<PathBuf> {
    let mut out = dest.to_path_buf();
    for comp in entry.components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Write one archive entry's bytes, never overwriting: existing files are
/// skipped and counted.
fn write_entry(
    dest: &Path,
    data: &mut dyn io::Read,
    counts: &mut (usize, usize),
) -> io::Result<()> {
    if dest.exists() {
        counts.1 += 1;
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = File::create(dest)?;
    io::copy(data, &mut f)?;
    counts.0 += 1;
    Ok(())
}

fn other_err(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

fn sevenz_err(e: sevenz_rust::Error) -> io::Error {
    match &e {
        sevenz_rust::Error::PasswordRequired | sevenz_rust::Error::MaybeBadPassword(_) => {
            other_err("password-protected archives are not supported")
        }
        // Without the aes256 feature, encrypted entries surface as an
        // unsupported AES method instead of PasswordRequired — same meaning
        // to the user.
        sevenz_rust::Error::UnsupportedCompressionMethod(m) if m.contains("AES") => {
            other_err("password-protected archives are not supported")
        }
        _ => other_err(e),
    }
}

/// Extract an archive into `dest` (created when missing). Returns
/// (files extracted, files skipped): existing files are never overwritten,
/// and unsafe entries (absolute paths, `..`) are skipped rather than
/// written. Password-protected archives fail with a readable error.
pub fn extract_archive(src: &Path, dest: &Path) -> io::Result<(usize, usize)> {
    extract_archive_filtered(src, dest, None)
}

/// Extract an archive into `dest` (created when missing). When `prefix`
/// is `Some`, only entries whose normalized internal path equals it or
/// starts with it are extracted — used to pull one file or folder out of
/// an archive-browsing tab. Same never-overwrite / zip-slip / password
/// rules as `extract_archive`; returns (files extracted, files skipped).
pub fn extract_archive_filtered(
    src: &Path,
    dest: &Path,
    prefix: Option<&str>,
) -> io::Result<(usize, usize)> {
    let kind = archive_kind(src).ok_or_else(|| {
        other_err(format!(
            "{} is not a supported archive",
            src.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ))
    })?;
    fs::create_dir_all(dest)?;
    let mut counts = (0usize, 0usize);
    match kind {
        "zip" => {
            let mut archive = zip::ZipArchive::new(File::open(src)?).map_err(other_err)?;
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(other_err)?;
                if entry.encrypted() {
                    return Err(other_err("password-protected archives are not supported"));
                }
                let name = match entry.enclosed_name() {
                    Some(n) => n,
                    None => {
                        counts.1 += 1;
                        continue;
                    }
                };
                if !match entry_wanted(&name.to_string_lossy(), prefix) {
                    None => {
                        counts.1 += 1;
                        continue;
                    }
                    Some(wanted) => wanted,
                } {
                    continue;
                }
                let out = match safe_join(dest, &name) {
                    Some(p) => p,
                    None => {
                        counts.1 += 1;
                        continue;
                    }
                };
                if entry.is_dir() {
                    if !out.exists() {
                        fs::create_dir_all(&out)?;
                    }
                    continue;
                }
                write_entry(&out, &mut entry, &mut counts)?;
            }
        }
        "7z" => {
            let mut reader = sevenz_rust::SevenZReader::open(src, sevenz_rust::Password::empty())
                .map_err(sevenz_err)?;
            reader
                .for_each_entries(|entry, data| {
                    if !match entry_wanted(entry.name(), prefix) {
                        None => {
                            counts.1 += 1;
                            return Ok(true);
                        }
                        Some(wanted) => wanted,
                    } {
                        return Ok(true);
                    }
                    let out = match safe_join(dest, Path::new(entry.name())) {
                        Some(p) => p,
                        None => {
                            counts.1 += 1;
                            return Ok(true);
                        }
                    };
                    if entry.is_directory() {
                        if !out.exists() {
                            fs::create_dir_all(&out).map_err(sevenz_rust::Error::from)?;
                        }
                        return Ok(true);
                    }
                    write_entry(&out, data, &mut counts).map_err(sevenz_rust::Error::from)?;
                    Ok(true)
                })
                .map_err(sevenz_err)?;
        }
        _ => {
            // "tar" and "targz"
            let file = File::open(src)?;
            if kind == "targz" {
                extract_tar(
                    tar::Archive::new(flate2::read::GzDecoder::new(file)),
                    dest,
                    &mut counts,
                    prefix,
                )?;
            } else {
                extract_tar(tar::Archive::new(file), dest, &mut counts, prefix)?;
            }
        }
    }
    Ok(counts)
}

/// Classify a raw archive entry name against the optional `prefix`
/// filter. With no filter every entry is wanted (the existing
/// safe_join/enclosed_name paths still reject unsafe entries exactly as
/// before). With a filter, names are normalized the same way as
/// `archive_entries`; None marks unsafe entries, which callers count as
/// skipped.
fn entry_wanted(raw_name: &str, prefix: Option<&str>) -> Option<bool> {
    let Some(p) = prefix else {
        return Some(true);
    };
    let name = normalize_entry_name(raw_name)?;
    Some(name == p || name.starts_with(p))
}

fn extract_tar<R: io::Read>(
    mut archive: tar::Archive<R>,
    dest: &Path,
    counts: &mut (usize, usize),
    prefix: Option<&str>,
) -> io::Result<()> {
    for entry in archive.entries().map_err(other_err)? {
        let mut entry = entry.map_err(other_err)?;
        let path: PathBuf = entry.path().map_err(other_err)?.into_owned();
        if !match entry_wanted(&path.to_string_lossy(), prefix) {
            None => {
                counts.1 += 1;
                continue;
            }
            Some(wanted) => wanted,
        } {
            continue;
        }
        let out = match safe_join(dest, &path) {
            Some(p) => p,
            None => {
                counts.1 += 1;
                continue;
            }
        };
        let ty = entry.header().entry_type();
        if ty.is_dir() {
            if !out.exists() {
                fs::create_dir_all(&out)?;
            }
        } else if ty.is_file() {
            write_entry(&out, &mut entry, counts)?;
        } else {
            // Symlinks, devices, etc.: skip rather than recreate.
            counts.1 += 1;
        }
    }
    Ok(())
}

// -- archive listing ---------------------------------------------------------

/// One entry inside an archive. `path` is the internal path with `/`
/// separators (no leading `./`, no `..`, never absolute).
#[derive(Debug, Clone)]
pub struct ArchEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Normalize an archive entry name: backslashes become `/`, a leading
/// `./` is stripped, a trailing `/` is dropped. Returns None for unsafe
/// entries (absolute paths or `..` components) — the same zip-slip rule
/// as `safe_join`.
fn normalize_entry_name(raw: &str) -> Option<String> {
    let mut s = raw.replace('\\', "/");
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_string();
    }
    while s.ends_with('/') && s.len() > 1 {
        s.pop();
    }
    if s.is_empty() {
        return None;
    }
    let p = Path::new(&s);
    if p.is_absolute()
        || p.components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return None;
    }
    Some(s)
}

/// List the entries of an archive without extracting it. Unsafe entries
/// (absolute paths, `..`) are skipped. Password-protected archives fail
/// with a readable error.
pub fn archive_entries(src: &Path) -> io::Result<Vec<ArchEntry>> {
    let kind = archive_kind(src).ok_or_else(|| {
        other_err(format!(
            "{} is not a supported archive",
            src.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ))
    })?;
    let mut out = Vec::new();
    match kind {
        "zip" => {
            let mut archive = zip::ZipArchive::new(File::open(src)?).map_err(other_err)?;
            for i in 0..archive.len() {
                let entry = archive.by_index(i).map_err(other_err)?;
                if entry.encrypted() {
                    return Err(other_err("password-protected archives are not supported"));
                }
                let (name, is_dir, size) = (entry.name().to_owned(), entry.is_dir(), entry.size());
                if let Some(path) = normalize_entry_name(&name) {
                    out.push(ArchEntry { path, is_dir, size });
                }
            }
        }
        "7z" => {
            let reader = sevenz_rust::SevenZReader::open(src, sevenz_rust::Password::empty())
                .map_err(sevenz_err)?;
            for entry in &reader.archive().files {
                if let Some(path) = normalize_entry_name(entry.name()) {
                    out.push(ArchEntry {
                        path,
                        is_dir: entry.is_directory(),
                        size: entry.size(),
                    });
                }
            }
        }
        _ => {
            // "tar" and "targz"
            let file = File::open(src)?;
            if kind == "targz" {
                list_tar(
                    tar::Archive::new(flate2::read::GzDecoder::new(file)),
                    &mut out,
                )?;
            } else {
                list_tar(tar::Archive::new(file), &mut out)?;
            }
        }
    }
    Ok(out)
}

fn list_tar<R: io::Read>(mut archive: tar::Archive<R>, out: &mut Vec<ArchEntry>) -> io::Result<()> {
    for entry in archive.entries().map_err(other_err)? {
        let entry = entry.map_err(other_err)?;
        let raw = entry
            .path()
            .map_err(other_err)?
            .to_string_lossy()
            .into_owned();
        let ty = entry.header().entry_type();
        if !(ty.is_file() || ty.is_dir()) {
            continue; // symlinks, devices, etc.
        }
        if let Some(path) = normalize_entry_name(&raw) {
            out.push(ArchEntry {
                path,
                is_dir: ty.is_dir(),
                size: entry.size(),
            });
        }
    }
    Ok(())
}

/// Byte counts as "1.5 MB" etc., for the list and previews.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// Priority of git status badges, highest first: renamed > deleted >
/// staged > modified > untracked.
fn git_badge_rank(b: char) -> u8 {
    match b {
        'R' => 4,
        'D' => 3,
        'A' => 2,
        'M' => 1,
        _ => 0, // '?'
    }
}

/// Map one `git status --porcelain=v1` X/Y code pair to a badge letter.
/// `R` renamed, `D` deleted, `A` staged/added, `M` modified (or unmerged),
/// `?` untracked. Ignored files and clean entries get no badge.
fn git_badge_for_codes(x: char, y: char) -> Option<char> {
    if x == 'R' || y == 'R' {
        Some('R')
    } else if x == 'D' || y == 'D' {
        Some('D')
    } else if x == 'M' || x == 'A' || x == 'T' {
        Some('A')
    } else if y == 'M' || y == 'T' || x == 'U' || y == 'U' {
        Some('M')
    } else if x == '?' {
        Some('?')
    } else {
        None
    }
}

/// Parse `git status --porcelain=v1 -z --untracked-files=normal` output
/// (NUL-separated records) into repo-relative path -> badge letter.
///
/// Each record starts with the X (index) and Y (worktree) status codes and
/// the repo-relative path. Rename records carry a second path record (the
/// old name); the badge is attributed to the new (first) path. Directory
/// badges are aggregated: every ancestor directory of a badged path gets
/// the highest-priority badge among its descendants, so folders show
/// something in the list even when only files inside them changed.
pub fn parse_git_porcelain(z: &str) -> HashMap<String, char> {
    let mut badges = HashMap::new();
    let records: Vec<&str> = z.split('\0').collect();
    let mut i = 0;
    while i < records.len() {
        let rec = records[i];
        i += 1;
        let mut chars = rec.chars();
        let (Some(x), Some(y)) = (chars.next(), chars.next()) else {
            continue;
        };
        // Renames carry the old path as a second NUL-separated record;
        // the new path is the one in this record. Consume it either way.
        let is_rename = x == 'R' || y == 'R';
        if is_rename && i < records.len() {
            i += 1;
        }
        let Some(path) = rec.get(3..) else { continue };
        if path.is_empty() {
            continue;
        }
        let Some(badge) = git_badge_for_codes(x, y) else {
            continue;
        };
        insert_badge(&mut badges, path, badge);
    }
    badges
}

/// Insert a file badge and propagate it to every ancestor directory,
/// keeping the highest-priority badge per directory.
fn insert_badge(map: &mut HashMap<String, char>, path: &str, badge: char) {
    if let Some(prev) = map.get(path) {
        if git_badge_rank(*prev) >= git_badge_rank(badge) {
            return;
        }
    }
    map.insert(path.to_string(), badge);
    // Walk ancestor dirs ("src/foo/bar.txt" -> "src/foo", "src").
    let mut rest = path;
    while let Some(idx) = rest.rfind('/') {
        rest = &rest[..idx];
        if rest.is_empty() {
            break;
        }
        match map.get(rest) {
            Some(prev) if git_badge_rank(*prev) >= git_badge_rank(badge) => {}
            _ => {
                map.insert(rest.to_string(), badge);
            }
        }
    }
}

/// Git status badges for the repository containing `dir`.
///
/// Runs `git status` on a background thread's behalf (the caller spawns
/// it) so the UI never blocks. Returns `(repo_root, badges)` where badges
/// maps repo-relative `/`-separated paths (files and aggregated
/// directories) to one of `M` `A` `D` `?` `R`. Returns `None` when `git`
/// is missing, fails, or `dir` isn't in a repository — callers show no
/// badges, silently.
pub fn git_badges(dir: &Path) -> Option<(PathBuf, HashMap<String, char>)> {
    let top = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .ok()?;
    if !top.status.success() {
        return None;
    }
    let root = PathBuf::from(String::from_utf8(top.stdout).ok()?.trim());
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("--no-optional-locks")
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .arg("--untracked-files=normal")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let z = String::from_utf8_lossy(&out.stdout);
    Some((root, parse_git_porcelain(&z)))
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
    fn create_xlsx_makes_valid_workbook() {
        let dir = tmpdir("xlsxnew");
        create_file(&dir, "report.xlsx").unwrap();
        let p = dir.join("report.xlsx");
        assert!(p.metadata().unwrap().len() > 0);
        // Reads back as a real workbook with one sheet.
        let book = umya_spreadsheet::reader::xlsx::read(&p).unwrap();
        assert_eq!(book.get_sheet_collection().len(), 1);
        // Uppercase extension gets the same treatment.
        create_file(&dir, "DATA.XLSX").unwrap();
        umya_spreadsheet::reader::xlsx::read(&dir.join("DATA.XLSX")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_docx_pptx_make_valid_packages() {
        let dir = tmpdir("ooxmlnew");
        create_file(&dir, "doc.docx").unwrap();
        create_file(&dir, "deck.pptx").unwrap();
        for (name, main_part) in [
            ("doc.docx", "word/document.xml"),
            ("deck.pptx", "ppt/slides/slide1.xml"),
        ] {
            let p = dir.join(name);
            assert!(p.metadata().unwrap().len() > 0);
            let mut z = zip::ZipArchive::new(File::open(&p).unwrap()).unwrap();
            assert!(
                z.by_name(main_part).is_ok(),
                "{name} is missing {main_part}"
            );
            let mut ct = String::new();
            z.by_name("[Content_Types].xml")
                .unwrap()
                .read_to_string(&mut ct)
                .unwrap();
            assert!(ct.contains(main_part), "{name} has a bad content type");
        }
        fs::remove_dir_all(&dir).unwrap();
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
    fn search_contents_limit_filters_and_caps() {
        let dir = tmpdir("gref");
        fs::write(dir.join("a.txt"), "Alpha here\nsecond line\n").unwrap();
        fs::write(dir.join("b.txt"), "ALPHA again\n").unwrap();
        // Hidden dirs are skipped entirely.
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git").join("config"), "alpha secret\n").unwrap();
        fs::create_dir_all(dir.join("sub").join(".hidden")).unwrap();
        fs::write(
            dir.join("sub").join(".hidden").join("x.txt"),
            "alpha buried\n",
        )
        .unwrap();
        // Binary (NUL byte) is skipped.
        fs::write(dir.join("blob.bin"), [b'a', b'l', b'p', b'h', b'a', 0u8]).unwrap();

        let hits = search_contents_limit(&dir, "alpha", 2000);
        assert_eq!(hits.len(), 2, "only a.txt and b.txt hit");
        for h in &hits {
            let p = h.path.to_string_lossy();
            assert!(!p.contains(".git"), "no hits from .git: {p}");
            assert!(!p.contains(".hidden"), "no hits from .hidden: {p}");
            assert!(!p.ends_with(".bin"), "no hits from binaries: {p}");
            assert!(h.line_no.is_some() && h.snippet.is_some());
        }

        // Case-insensitive: same result for a differently-cased query.
        assert_eq!(search_contents_limit(&dir, "ALPHA", 2000).len(), 2);

        // The limit is respected.
        assert_eq!(search_contents_limit(&dir, "alpha", 1).len(), 1);
        assert_eq!(search_contents_limit(&dir, "alpha", 0).len(), 0);
        assert!(search_contents_limit(&dir, "", 2000).is_empty());

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

    // -- archive extraction -------------------------------------------------

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            use std::io::Write as _;
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    fn write_targz(path: &Path, entries: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let enc = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
        let mut b = tar::Builder::new(enc);
        for (name, data) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, *data).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn archive_kind_and_stem() {
        assert!(is_archive(Path::new("a.zip")));
        assert!(is_archive(Path::new("a.7z")));
        assert!(is_archive(Path::new("a.tar")));
        assert!(is_archive(Path::new("a.tar.gz")));
        assert!(is_archive(Path::new("a.tgz")));
        assert!(is_archive(Path::new("A.ZIP")));
        assert!(!is_archive(Path::new("a.txt")));
        assert!(!is_archive(Path::new("azip")));
        assert_eq!(
            archive_stem(Path::new("photos.zip")).as_deref(),
            Some("photos")
        );
        assert_eq!(
            archive_stem(Path::new("photos.tar.gz")).as_deref(),
            Some("photos")
        );
        assert_eq!(archive_stem(Path::new("a.tgz")).as_deref(), Some("a"));
        assert_eq!(archive_stem(Path::new("a.7z")).as_deref(), Some("a"));
    }

    #[test]
    fn extract_zip_roundtrip() {
        let dir = tmpdir("xzip");
        let src = dir.join("bundle.zip");
        write_zip(
            &src,
            &[
                ("a.txt", b"hello"),
                ("sub/b.txt", b"world"),
                ("sub/deep/c.txt", b"!"),
            ],
        );
        let dest = dir.join("out");
        let (extracted, skipped) = extract_archive(&src, &dest).unwrap();
        assert_eq!((extracted, skipped), (3, 0));
        assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "hello");
        assert_eq!(fs::read_to_string(dest.join("sub/b.txt")).unwrap(), "world");
        assert_eq!(
            fs::read_to_string(dest.join("sub/deep/c.txt")).unwrap(),
            "!"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extract_targz_roundtrip() {
        let dir = tmpdir("xtgz");
        let src = dir.join("bundle.tar.gz");
        write_targz(&src, &[("a.txt", b"hello"), ("sub/b.txt", b"world")]);
        let dest = dir.join("out");
        let (extracted, skipped) = extract_archive(&src, &dest).unwrap();
        assert_eq!((extracted, skipped), (2, 0));
        assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "hello");
        assert_eq!(fs::read_to_string(dest.join("sub/b.txt")).unwrap(), "world");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extract_7z_roundtrip() {
        let dir = tmpdir("x7z");
        let srcdir = dir.join("srcdir");
        fs::create_dir_all(srcdir.join("sub")).unwrap();
        fs::write(srcdir.join("a.txt"), "hello").unwrap();
        fs::write(srcdir.join("sub/b.txt"), "world").unwrap();
        let src = dir.join("bundle.7z");
        sevenz_rust::compress_to_path(&srcdir, &src).unwrap();
        let dest = dir.join("out");
        let (extracted, skipped) = extract_archive(&src, &dest).unwrap();
        assert_eq!((extracted, skipped), (2, 0));
        assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "hello");
        assert_eq!(fs::read_to_string(dest.join("sub/b.txt")).unwrap(), "world");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extract_never_overwrites() {
        let dir = tmpdir("xskip");
        let src = dir.join("b.zip");
        write_zip(&src, &[("a.txt", b"new"), ("c.txt", b"fresh")]);
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("a.txt"), "original").unwrap();
        let (extracted, skipped) = extract_archive(&src, &dest).unwrap();
        assert_eq!((extracted, skipped), (1, 1));
        assert_eq!(
            fs::read_to_string(dest.join("a.txt")).unwrap(),
            "original",
            "existing file must not be overwritten"
        );
        assert_eq!(fs::read_to_string(dest.join("c.txt")).unwrap(), "fresh");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extract_rejects_dotdot_entries() {
        let dir = tmpdir("xslip");
        let src = dir.join("evil.zip");
        write_zip(&src, &[("../evil.txt", b"pwned"), ("ok.txt", b"fine")]);
        let dest = dir.join("out");
        let (extracted, skipped) = extract_archive(&src, &dest).unwrap();
        assert_eq!((extracted, skipped), (1, 1));
        assert!(!dir.join("evil.txt").exists(), "zip-slip must not escape");
        assert_eq!(fs::read_to_string(dest.join("ok.txt")).unwrap(), "fine");

        // Same for tar: poke the name field past the crate's own `..` guard
        // so the fixture really is hostile.
        let tsrc = dir.join("evil.tar");
        let f = File::create(&tsrc).unwrap();
        let mut b = tar::Builder::new(f);
        let mut h = tar::Header::new_gnu();
        let data = b"pwned";
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        {
            let raw = h.as_mut_bytes();
            let name = b"../evil2.txt";
            raw[..name.len()].copy_from_slice(name);
        }
        h.set_cksum();
        b.append(&h, &data[..]).unwrap();
        let mut h2 = tar::Header::new_gnu();
        let data2 = b"fine";
        h2.set_size(data2.len() as u64);
        h2.set_mode(0o644);
        h2.set_cksum();
        b.append_data(&mut h2, "ok2.txt", &data2[..]).unwrap();
        b.into_inner().unwrap();
        let dest2 = dir.join("out2");
        let (extracted, skipped) = extract_archive(&tsrc, &dest2).unwrap();
        assert_eq!((extracted, skipped), (1, 1));
        assert!(!dir.join("evil2.txt").exists(), "tar-slip must not escape");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extract_password_protected_7z_fails_cleanly() {
        // Fixture generated with py7zr using password "hunter2" (the
        // aes256 feature is disabled, so the crate can't create one).
        const LOCKED: &[u8] = include_bytes!("../tests/fixtures/locked.7z");
        let dir = tmpdir("xpass");
        let src = dir.join("locked.7z");
        fs::write(&src, LOCKED).unwrap();
        let dest = dir.join("out");
        match extract_archive(&src, &dest) {
            Ok(_) => panic!("expected a password error"),
            Err(e) => assert!(
                e.to_string().contains("password-protected"),
                "unexpected error: {e}"
            ),
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extract_unsupported_is_an_error() {
        let dir = tmpdir("xbad");
        let src = dir.join("a.txt");
        fs::write(&src, "not an archive").unwrap();
        assert!(extract_archive(&src, &dir.join("out")).is_err());
        assert!(!is_archive(&src));
        fs::remove_dir_all(&dir).unwrap();
    }

    // -- archive listing ----------------------------------------------------

    fn entry_paths(entries: &[ArchEntry]) -> Vec<String> {
        let mut v: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn archive_entries_zip_lists_nested_and_skips_unsafe() {
        let dir = tmpdir("xlist");
        let src = dir.join("nested.zip");
        write_zip(
            &src,
            &[
                ("top.txt", b"top"),
                ("docs/a.txt", b"a"),
                ("docs/sub/b.txt", b"b"),
                ("./dot.txt", b"dot"),
                ("../evil.txt", b"pwned"),
                ("/abs.txt", b"nope"),
            ],
        );
        let entries = archive_entries(&src).unwrap();
        assert_eq!(
            entry_paths(&entries),
            vec!["docs/a.txt", "docs/sub/b.txt", "dot.txt", "top.txt"],
            "unsafe entries must be skipped, ./ stripped"
        );
        let a = entries.iter().find(|e| e.path == "docs/a.txt").unwrap();
        assert!(!a.is_dir && a.size == 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn archive_entries_targz_lists_and_skips_unsafe() {
        let dir = tmpdir("xtlist");
        let src = dir.join("nested.tar.gz");
        let f = File::create(&src).unwrap();
        let enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        let mut b = tar::Builder::new(enc);
        let mut h1 = tar::Header::new_gnu();
        h1.set_size(1);
        h1.set_mode(0o644);
        h1.set_cksum();
        b.append_data(&mut h1, "x/one.txt", &b"1"[..]).unwrap();
        let mut h2 = tar::Header::new_gnu();
        h2.set_size(2);
        h2.set_mode(0o644);
        h2.set_cksum();
        b.append_data(&mut h2, "x/y/two.txt", &b"22"[..]).unwrap();
        b.into_inner().unwrap().finish().unwrap();
        let entries = archive_entries(&src).unwrap();
        assert_eq!(entry_paths(&entries), vec!["x/one.txt", "x/y/two.txt"]);
        let two = entries.iter().find(|e| e.path == "x/y/two.txt").unwrap();
        assert_eq!(two.size, 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn archive_entries_7z_lists_nested() {
        let dir = tmpdir("x7list");
        let srcdir = dir.join("srcdir");
        fs::create_dir_all(srcdir.join("sub")).unwrap();
        fs::write(srcdir.join("a.txt"), "a").unwrap();
        fs::write(srcdir.join("sub").join("b.txt"), "bb").unwrap();
        let src = dir.join("nested.7z");
        sevenz_rust::compress_to_path(&srcdir, &src).unwrap();
        let entries = archive_entries(&src).unwrap();
        let paths = entry_paths(&entries);
        assert!(
            paths.iter().any(|p| p.ends_with("a.txt")),
            "missing a.txt in {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.ends_with("sub/b.txt")),
            "missing sub/b.txt in {paths:?}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extract_filtered_extracts_only_the_prefix() {
        let dir = tmpdir("xfilt");
        let src = dir.join("sel.zip");
        write_zip(
            &src,
            &[
                ("keep.txt", b"keep"),
                ("docs/a.txt", b"a"),
                ("docs/sub/b.txt", b"b"),
                ("other/c.txt", b"c"),
            ],
        );
        // One file.
        let d1 = dir.join("one");
        let (e, s) = extract_archive_filtered(&src, &d1, Some("keep.txt")).unwrap();
        assert_eq!((e, s), (1, 0));
        assert_eq!(fs::read_to_string(d1.join("keep.txt")).unwrap(), "keep");
        assert!(!d1.join("docs").exists());
        // One folder subtree, paths kept relative to the archive root.
        let d2 = dir.join("sub");
        let (e, s) = extract_archive_filtered(&src, &d2, Some("docs/")).unwrap();
        assert_eq!((e, s), (2, 0));
        assert_eq!(fs::read_to_string(d2.join("docs/a.txt")).unwrap(), "a");
        assert_eq!(fs::read_to_string(d2.join("docs/sub/b.txt")).unwrap(), "b");
        assert!(!d2.join("other").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_git_porcelain_maps_all_status_codes() {
        let z = "?? untracked.txt\0M  staged_mod.txt\0 M worktree_mod.txt\0A  added.txt\0T  typechg.txt\0 D deleted_wt.txt\0D  deleted_idx.txt\0UU conflict.txt\0!! ignored.txt\0";
        let b = parse_git_porcelain(z);
        assert_eq!(b.get("untracked.txt"), Some(&'?'));
        assert_eq!(b.get("staged_mod.txt"), Some(&'A'));
        assert_eq!(b.get("worktree_mod.txt"), Some(&'M'));
        assert_eq!(b.get("added.txt"), Some(&'A'));
        assert_eq!(b.get("typechg.txt"), Some(&'A'));
        assert_eq!(b.get("deleted_wt.txt"), Some(&'D'));
        assert_eq!(b.get("deleted_idx.txt"), Some(&'D'));
        assert_eq!(b.get("conflict.txt"), Some(&'M'));
        assert!(!b.contains_key("ignored.txt"));
    }

    #[test]
    fn parse_git_porcelain_rename_consumes_both_paths() {
        // Real -z output puts the NEW path first, the old path second.
        let z = "R  new.txt\0old.txt\0 M other.txt\0";
        let b = parse_git_porcelain(z);
        assert_eq!(b.get("new.txt"), Some(&'R'));
        assert!(!b.contains_key("old.txt"));
        assert_eq!(b.get("other.txt"), Some(&'M'));
    }

    #[test]
    fn parse_git_porcelain_aggregates_directory_badges() {
        let z = " M src/a.txt\0A  src/sub/b.txt\0?? other/c.txt\0";
        let b = parse_git_porcelain(z);
        assert_eq!(b.get("src/a.txt"), Some(&'M'));
        assert_eq!(b.get("src/sub/b.txt"), Some(&'A'));
        // Highest priority among descendants wins: A (staged) beats M.
        assert_eq!(b.get("src"), Some(&'A'));
        assert_eq!(b.get("src/sub"), Some(&'A'));
        assert_eq!(b.get("other"), Some(&'?'));
        assert_eq!(b.get("other/c.txt"), Some(&'?'));
    }

    #[test]
    fn parse_git_porcelain_empty_and_garbage() {
        assert!(parse_git_porcelain("").is_empty());
        assert!(parse_git_porcelain("\0\0").is_empty());
    }
}
