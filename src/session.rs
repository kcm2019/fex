//! Session restore and favorites persistence.
//!
//! Everything lives under `~/.config/fex/`:
//! - `favorites`: one escaped absolute path per line.
//! - `session/tabs`: tab-separated tab descriptors, one per line.
//! - `session/backup-N.txt`: buffer backups for dirty/untitled editors,
//!   written on quit and destroyed when the tab is closed or the file saved.
//!
//! The format is deliberately dependency-free (tab-separated fields with
//! backslash escaping) rather than JSON.

use std::io;
use std::path::{Path, PathBuf};

/// `~/.config/fex`, or `None` when `HOME` is unset.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config").join("fex"))
}

fn session_subdir(dir: &Path) -> PathBuf {
    dir.join("session")
}

fn tabs_path_in(dir: &Path) -> PathBuf {
    session_subdir(dir).join("tabs")
}

fn favorites_path_in(dir: &Path) -> PathBuf {
    dir.join("favorites")
}

/// Escape `\`, tab, and newline so a value survives tab-separated storage.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// Inverse of [`esc`].
pub fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(x) => {
                    out.push('\\');
                    out.push(x);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split a line on unescaped tabs, unescaping each field.
fn split_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Keep the escape pair intact; `unesc` decodes it later.
            cur.push(c);
            if let Some(n) = chars.next() {
                cur.push(n);
            }
        } else if c == '\t' {
            fields.push(unesc(&cur));
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    fields.push(unesc(&cur));
    fields
}

/// A file-browser tab: where it was, how it looked, what was selected.
pub struct BrowserDesc {
    pub cwd: String,
    pub selected: String,
    pub view: String, // "list" | "columns"
    pub sort: String, // "name" | "size" | "modified" | "type"
    pub ascending: bool,
    pub show_hidden: bool,
}

/// An editor tab. `backup` is a file name inside the session dir holding
/// unsaved buffer content; empty when the on-disk file is enough.
pub struct EditorDesc {
    pub cwd: String,
    pub path: String, // "" for an untitled document
    pub backup: String,
    pub row: usize,
    pub col: usize,
}

/// A terminal tab. Only the working directory is restored — a fresh shell
/// starts there; a dead process can't be resurrected.
pub struct ShellDesc {
    pub cwd: String,
}

pub enum TabDesc {
    Browser(BrowserDesc),
    Editor(EditorDesc),
    Shell(ShellDesc),
}

/// Serialize the tab list. Pure function (no I/O) so it stays testable.
pub fn serialize_tabs(descs: &[TabDesc], active: usize) -> String {
    let mut s = String::from("v1\n");
    s.push_str(&format!("active\t{active}\n"));
    for d in descs {
        match d {
            TabDesc::Browser(b) => {
                s.push_str(&format!(
                    "browser\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    esc(&b.cwd),
                    esc(&b.selected),
                    b.view,
                    b.sort,
                    u8::from(b.ascending),
                    u8::from(b.show_hidden),
                ));
            }
            TabDesc::Editor(e) => {
                s.push_str(&format!(
                    "editor\t{}\t{}\t{}\t{}\t{}\n",
                    esc(&e.cwd),
                    esc(&e.path),
                    esc(&e.backup),
                    e.row,
                    e.col,
                ));
            }
            TabDesc::Shell(sh) => {
                s.push_str(&format!("shell\t{}\n", esc(&sh.cwd)));
            }
        }
    }
    s
}

/// Parse what [`serialize_tabs`] produced. `None` means corrupt or foreign —
/// the caller should start fresh rather than guess.
pub fn parse_tabs(text: &str) -> Option<(Vec<TabDesc>, usize)> {
    let mut lines = text.lines();
    if lines.next()? != "v1" {
        return None;
    }
    let active_fields = split_fields(lines.next()?);
    if active_fields.first().map(String::as_str) != Some("active") || active_fields.len() != 2 {
        return None;
    }
    let active: usize = active_fields[1].parse().ok()?;
    let mut descs = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let f = split_fields(line);
        match f.first().map(String::as_str) {
            Some("browser") if f.len() == 7 => descs.push(TabDesc::Browser(BrowserDesc {
                cwd: f[1].clone(),
                selected: f[2].clone(),
                view: f[3].clone(),
                sort: f[4].clone(),
                ascending: f[5] == "1",
                show_hidden: f[6] == "1",
            })),
            Some("editor") if f.len() == 6 => descs.push(TabDesc::Editor(EditorDesc {
                cwd: f[1].clone(),
                path: f[2].clone(),
                backup: f[3].clone(),
                row: f[4].parse().unwrap_or(0),
                col: f[5].parse().unwrap_or(0),
            })),
            Some("shell") if f.len() == 2 => {
                descs.push(TabDesc::Shell(ShellDesc { cwd: f[1].clone() }))
            }
            _ => return None,
        }
    }
    Some((descs, active))
}

/// Write the tab list under `dir` (best effort).
pub fn save_tabs_in(dir: &Path, descs: &[TabDesc], active: usize) -> io::Result<()> {
    let path = tabs_path_in(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serialize_tabs(descs, active))
}

/// Read the tab list back. `None` when there is nothing (or nothing usable).
pub fn load_tabs_in(dir: &Path) -> Option<(Vec<TabDesc>, usize)> {
    parse_tabs(&std::fs::read_to_string(tabs_path_in(dir)).ok()?)
}

/// Delete every `backup-*.txt` in the session dir. Called on quit before
/// fresh backups are written, so stale ones never linger.
pub fn clear_backups_in(dir: &Path) -> io::Result<()> {
    let sub = session_subdir(dir);
    std::fs::create_dir_all(&sub)?;
    for entry in std::fs::read_dir(&sub)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("backup-") && name.ends_with(".txt") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Full path of a backup file inside the session dir. Rejects empty names
/// and path separators so a corrupt session file can't escape the dir.
pub fn backup_file_in(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return None;
    }
    Some(session_subdir(dir).join(name))
}

/// Read a backup file's text. `None` when it doesn't exist or is unreadable.
pub fn read_backup_in(dir: &Path, name: &str) -> Option<(String, PathBuf)> {
    let path = backup_file_in(dir, name)?;
    std::fs::read_to_string(&path).ok().map(|text| (text, path))
}

/// Load the favorites list. Missing or unreadable file means no favorites.
pub fn load_favorites_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(favorites_path_in(dir)) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| Path::new(&unesc(l)).to_path_buf())
        .collect()
}

/// Persist the favorites list (best effort).
pub fn save_favorites_in(dir: &Path, favs: &[PathBuf]) -> io::Result<()> {
    let path = favorites_path_in(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text: Vec<String> = favs
        .iter()
        .map(|p| esc(&p.to_string_lossy()) + "\n")
        .collect();
    std::fs::write(&path, text.concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_unesc_round_trip() {
        for s in [
            "/plain/path",
            "/with space/and\ttab",
            "/new\nline",
            "/back\\slash",
            "C:\\win\\path",
            "trailing\\",
            "uni—codé★",
            "",
        ] {
            assert_eq!(unesc(&esc(s)), s, "round trip failed for {s:?}");
        }
    }

    #[test]
    fn tabs_round_trip() {
        let descs = vec![
            TabDesc::Browser(BrowserDesc {
                cwd: "/tmp/my dir".into(),
                selected: "we|ird\tname.txt".into(),
                view: "columns".into(),
                sort: "type".into(),
                ascending: true,
                show_hidden: true,
            }),
            TabDesc::Editor(EditorDesc {
                cwd: "/tmp".into(),
                path: "/tmp/no\ttab.txt".into(),
                backup: "backup-1.txt".into(),
                row: 12,
                col: 3,
            }),
            TabDesc::Editor(EditorDesc {
                cwd: "/tmp".into(),
                path: "".into(),
                backup: "backup-2.txt".into(),
                row: 0,
                col: 0,
            }),
            TabDesc::Shell(ShellDesc {
                cwd: "/home/user".into(),
            }),
        ];
        let text = serialize_tabs(&descs, 2);
        let (back, active) = parse_tabs(&text).expect("parse failed");
        assert_eq!(active, 2);
        assert_eq!(back.len(), 4);
        match &back[0] {
            TabDesc::Browser(b) => {
                assert_eq!(b.cwd, "/tmp/my dir");
                assert_eq!(b.selected, "we|ird\tname.txt");
                assert_eq!(b.view, "columns");
                assert_eq!(b.sort, "type");
                assert!(b.ascending);
                assert!(b.show_hidden);
            }
            _ => panic!("expected browser"),
        }
        match &back[1] {
            TabDesc::Editor(e) => {
                assert_eq!(e.path, "/tmp/no\ttab.txt");
                assert_eq!(e.backup, "backup-1.txt");
                assert_eq!(e.row, 12);
                assert_eq!(e.col, 3);
            }
            _ => panic!("expected editor"),
        }
        match &back[3] {
            TabDesc::Shell(s) => assert_eq!(s.cwd, "/home/user"),
            _ => panic!("expected shell"),
        }
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_tabs("").is_none());
        assert!(parse_tabs("v2\nactive\t0\n").is_none());
        assert!(parse_tabs("v1\nnope\n").is_none());
        assert!(parse_tabs("v1\nactive\tx\n").is_none());
        assert!(parse_tabs("v1\nactive\t0\nbrowser\t/a\n").is_none());
        assert!(parse_tabs("v1\nactive\t0\nmystery\tx\n").is_none());
    }

    #[test]
    fn backup_file_rejects_traversal() {
        let dir = Path::new("/tmp/fex-test-config");
        assert!(backup_file_in(dir, "").is_none());
        assert!(backup_file_in(dir, "../evil").is_none());
        assert!(backup_file_in(dir, "a\\b").is_none());
        let ok = backup_file_in(dir, "backup-0.txt").unwrap();
        assert!(ok.starts_with(session_subdir(dir)));
    }
}
