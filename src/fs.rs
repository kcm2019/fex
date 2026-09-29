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
    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };
    for dir_entry in read_dir.flatten() {
        let path = dir_entry.path();
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        let is_hidden = name.starts_with('.');
        let metadata = dir_entry.metadata().ok();
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

const PREVIEW_MAX_BYTES: usize = 8192;
const PREVIEW_MAX_LINES: usize = 120;

/// Read a short text preview of a file, or an item count for directories.
pub fn read_preview(path: &Path, is_dir: bool) -> String {
    if is_dir {
        let count = fs::read_dir(path).map(|rd| rd.count()).unwrap_or(0);
        return if count == 1 {
            String::from("1 item")
        } else {
            format!("{count} items")
        };
    }
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return String::from("[cannot read file]"),
    };
    let mut buf = vec![0u8; PREVIEW_MAX_BYTES];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return String::from("[cannot read file]"),
    };
    buf.truncate(n);
    if n == 0 {
        return String::from("[empty file]");
    }
    if buf.contains(&0) {
        return String::from("[binary file]");
    }
    let text = String::from_utf8_lossy(&buf);
    text.lines().take(PREVIEW_MAX_LINES).collect::<Vec<_>>().join("\n")
}

/// Create an empty file inside `dir`. Errors if the name is taken.
pub fn create_file(dir: &Path, name: &str) -> io::Result<()> {
    let path = dir.join(name);
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "already exists"));
    }
    File::create(path)?;
    Ok(())
}

/// Create a directory inside `dir`. Errors if the name is taken.
pub fn create_dir(dir: &Path, name: &str) -> io::Result<()> {
    let path = dir.join(name);
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "already exists"));
    }
    fs::create_dir(path)?;
    Ok(())
}

/// Rename an entry, keeping it in the same directory. Errors if taken.
pub fn rename_entry(path: &Path, new_name: &str) -> io::Result<()> {
    let parent =
        path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "no parent directory"))?;
    let dest = parent.join(new_name);
    if dest.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "already exists"));
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

/// Copy text to the OS clipboard (macOS `pbcopy`). Best-effort: returns
/// false when no clipboard tool is available; callers keep working without it.
pub fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = match Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let wrote = child
        .stdin
        .as_mut()
        .is_some_and(|s| s.write_all(text.as_bytes()).is_ok());
    wrote && child.wait().is_ok_and(|s| s.success())
}

/// Read text from the OS clipboard (macOS `pbpaste`). None when unavailable.
pub fn read_clipboard() -> Option<String> {
    let out = Command::new("pbpaste").output().ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout).ok()
    } else {
        None
    }
}

/// Open a file with the OS default application.
pub fn open_with_default(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        Err(io::Error::new(io::ErrorKind::Unsupported, "unsupported platform"))
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
        assert!(read_preview(&dir.join("t.txt"), false).contains("hello"));
        fs::write(dir.join("b.bin"), [0u8, 1, 2, 3]).unwrap();
        assert_eq!(read_preview(&dir.join("b.bin"), false), "[binary file]");
        assert_eq!(read_preview(&dir, true), "2 items");
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
