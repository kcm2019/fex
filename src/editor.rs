//! Built-in text editor with syntax highlighting.
//!
//! Opened with `e` on a text file. Ctrl+S saves, Esc closes
//! (asking about unsaved changes).

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Refuse to open files bigger than this in the editor.
const MAX_EDIT_BYTES: u64 = 512 * 1024;
/// Lines longer than this are shown without highlighting.
const MAX_HL_LINE_LEN: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    Toml,
    Json,
    Markdown,
    Shell,
    Plain,
}

impl Lang {
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "rs" => Lang::Rust,
            "py" => Lang::Python,
            "js" | "jsx" | "mjs" | "ts" | "tsx" => Lang::JavaScript,
            "toml" => Lang::Toml,
            "json" => Lang::Json,
            "md" | "markdown" => Lang::Markdown,
            "sh" | "bash" | "zsh" => Lang::Shell,
            _ => Lang::Plain,
        }
    }

    fn keywords(&self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop",
                "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static",
                "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            Lang::Python => &[
                "False", "None", "True", "and", "as", "assert", "async", "await", "break",
                "class", "continue", "def", "del", "elif", "else", "except", "finally", "for",
                "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not",
                "or", "pass", "raise", "return", "try", "while", "with", "yield",
            ],
            Lang::JavaScript => &[
                "async", "await", "break", "case", "catch", "class", "const", "continue",
                "debugger", "default", "delete", "do", "else", "export", "extends", "false",
                "finally", "for", "function", "if", "import", "in", "instanceof", "let",
                "new", "null", "of", "return", "static", "super", "switch", "this", "throw",
                "true", "try", "typeof", "var", "void", "while", "with", "yield",
            ],
            Lang::Toml | Lang::Json => &["true", "false", "null"],
            Lang::Shell => &[
                "break", "case", "continue", "do", "done", "elif", "else", "esac", "exit",
                "export", "fi", "for", "function", "if", "in", "local", "readonly", "return",
                "select", "set", "shift", "then", "unset", "while",
            ],
            Lang::Markdown | Lang::Plain => &[],
        }
    }

    fn line_comment(&self) -> Option<&'static str> {
        match self {
            Lang::Rust | Lang::JavaScript => Some("//"),
            Lang::Python | Lang::Shell | Lang::Toml => Some("#"),
            Lang::Json | Lang::Markdown | Lang::Plain => None,
        }
    }

    fn block_comment(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Lang::Rust | Lang::JavaScript => Some(("/*", "*/")),
            Lang::Python => Some(("'''", "'''")),
            _ => None,
        }
    }

    fn string_delims(&self) -> &'static [char] {
        match self {
            Lang::Rust => &['"'],
            Lang::Markdown | Lang::Plain => &['"'],
            _ => &['"', '\''],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Normal,
    Keyword,
    Str,
    Comment,
    Number,
}

/// Accumulates highlighted segments, merging runs of the same kind.
struct SegBuf {
    segs: Vec<(Tok, String)>,
    cur: String,
    kind: Tok,
}

impl SegBuf {
    fn new() -> Self {
        Self { segs: Vec::new(), cur: String::new(), kind: Tok::Normal }
    }

    fn feed(&mut self, kind: Tok, s: &str) {
        if s.is_empty() {
            return;
        }
        if self.kind != kind {
            if !self.cur.is_empty() {
                self.segs.push((self.kind, std::mem::take(&mut self.cur)));
            }
            self.kind = kind;
        }
        self.cur.push_str(s);
    }

    fn feed_ch(&mut self, kind: Tok, ch: char) {
        if self.kind != kind {
            if !self.cur.is_empty() {
                self.segs.push((self.kind, std::mem::take(&mut self.cur)));
            }
            self.kind = kind;
        }
        self.cur.push(ch);
    }

    fn finish(mut self) -> Vec<(Tok, String)> {
        if !self.cur.is_empty() {
            self.segs.push((self.kind, std::mem::take(&mut self.cur)));
        }
        self.segs
    }
}

fn classify_word(run: &str, kws: &[&str]) -> Tok {
    if kws.contains(&run) {
        Tok::Keyword
    } else if run.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Tok::Number
    } else {
        Tok::Normal
    }
}

/// Split Normal segments into keyword/number/identifier runs.
fn postprocess(segs: Vec<(Tok, String)>, lang: Lang) -> Vec<(Tok, String)> {
    let kws = lang.keywords();
    let mut out = Vec::with_capacity(segs.len());
    for (kind, s) in segs {
        if kind != Tok::Normal || kws.is_empty() {
            out.push((kind, s));
            continue;
        }
        let mut run = String::new();
        let mut is_word = false;
        let mut started = false;
        for ch in s.chars() {
            let w = ch.is_alphanumeric() || ch == '_';
            if started && w == is_word {
                run.push(ch);
            } else {
                if started {
                    out.push((classify_word(&run, kws), std::mem::take(&mut run)));
                }
                started = true;
                is_word = w;
                run.push(ch);
            }
        }
        if started {
            out.push((classify_word(&run, kws), run));
        }
    }
    out
}

/// Highlight one line. Returns the segments and whether the line ends
/// inside a block comment (carried into the next line).
fn scan_line(line: &str, lang: Lang, in_block: bool) -> (Vec<(Tok, String)>, bool) {
    if line.len() > MAX_HL_LINE_LEN {
        return (vec![(Tok::Normal, line.to_string())], in_block);
    }
    // Markdown headers get a fast path.
    if lang == Lang::Markdown && line.trim_start().starts_with('#') {
        return (vec![(Tok::Keyword, line.to_string())], false);
    }

    let mut buf = SegBuf::new();
    let mut in_block = in_block;
    let lc = lang.line_comment();
    let bc = lang.block_comment();
    let delims = lang.string_delims();

    // Finish a block comment carried over from the previous line.
    let mut work = line;
    if in_block {
        match bc {
            Some((_, close)) => match work.find(close) {
                Some(p) => {
                    let e = p + close.len();
                    buf.feed(Tok::Comment, &work[..e]);
                    work = &work[e..];
                    in_block = false;
                }
                None => {
                    buf.feed(Tok::Comment, work);
                    return (postprocess(buf.finish(), lang), true);
                }
            },
            None => in_block = false,
        }
    }

    let idx: Vec<(usize, char)> = work.char_indices().collect();
    let n = idx.len();
    let mut k = 0;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    while k < n {
        let (b, ch) = idx[k];
        let rest = &work[b..];

        if let Some(q) = in_string {
            buf.feed_ch(Tok::Str, ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                in_string = None;
            }
            k += 1;
            continue;
        }

        if let Some(lc) = lc {
            if rest.starts_with(lc) {
                buf.feed(Tok::Comment, rest);
                break;
            }
        }

        if let Some((open, close)) = bc {
            if rest.starts_with(open) {
                match rest[open.len()..].find(close) {
                    Some(p) => {
                        let e = open.len() + p + close.len();
                        buf.feed(Tok::Comment, &rest[..e]);
                        k = idx.iter().position(|(bb, _)| *bb >= b + e).unwrap_or(n);
                        continue;
                    }
                    None => {
                        buf.feed(Tok::Comment, rest);
                        in_block = true;
                        break;
                    }
                }
            }
        }

        // Rust char literal: 'x' or '\n' (a bare ' would clash with lifetimes).
        if lang == Lang::Rust && ch == '\'' {
            let escaped_lit = idx.get(k + 1).map(|(_, c)| *c) == Some('\\')
                && idx.get(k + 3).map(|(_, c)| *c) == Some('\'');
            let plain_lit = idx.get(k + 2).map(|(_, c)| *c) == Some('\'');
            let take = if escaped_lit { 4 } else if plain_lit { 3 } else { 0 };
            if take > 0 {
                let end_b = if k + take < n { idx[k + take].0 } else { work.len() };
                buf.feed(Tok::Str, &work[b..end_b]);
                k += take;
                continue;
            }
            buf.feed_ch(Tok::Normal, ch);
            k += 1;
            continue;
        }

        if delims.contains(&ch) {
            in_string = Some(ch);
            buf.feed_ch(Tok::Str, ch);
            k += 1;
            continue;
        }

        buf.feed_ch(Tok::Normal, ch);
        k += 1;
    }

    (postprocess(buf.finish(), lang), in_block)
}

fn span_for(kind: Tok, s: String) -> Span<'static> {
    let style = match kind {
        Tok::Normal => Style::default(),
        Tok::Keyword => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        Tok::Str => Style::default().fg(Color::Green),
        Tok::Comment => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        Tok::Number => Style::default().fg(Color::Magenta),
    };
    Span::styled(s, style)
}

fn char_idx_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

pub struct Editor {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub row: usize, // line index
    pub col: usize, // char index within the line
    pub offset: usize,
    pub view_h: usize,
    pub dirty: bool,
    pub message: String,
    pub clipboard: Vec<String>, // lines copied/cut for pasting
    lang: Lang,
    trailing_newline: bool,
    block_end: Vec<bool>, // in-block-comment at end of line i
    hl_valid: usize,      // rows [0, hl_valid) have valid block_end entries
}

impl Editor {
    pub fn open(path: &Path) -> io::Result<Editor> {
        let meta = fs::metadata(path)?;
        if meta.len() > MAX_EDIT_BYTES {
            return Err(io::Error::new(io::ErrorKind::Other, "file too large to edit"));
        }
        let content = fs::read_to_string(path)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "not valid UTF-8 text"))?;
        if content.contains('\0') {
            return Err(io::Error::new(io::ErrorKind::Other, "binary file"));
        }
        let trailing_newline = content.ends_with('\n');
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Ok(Editor {
            path: path.to_path_buf(),
            lines,
            row: 0,
            col: 0,
            offset: 0,
            view_h: 24,
            dirty: false,
            message: String::new(),
            clipboard: Vec::new(),
            lang: Lang::from_path(path),
            trailing_newline,
            block_end: Vec::new(),
            hl_valid: 0,
        })
    }

    pub fn save(&mut self) -> io::Result<()> {
        let mut content = self.lines.join("\n");
        if self.trailing_newline {
            content.push('\n');
        }
        fs::write(&self.path, content)?;
        self.dirty = false;
        self.message = format!("Saved {}", self.path.display());
        Ok(())
    }

    // -- editing ----------------------------------------------------------

    fn invalidate_hl(&mut self, row: usize) {
        self.hl_valid = self.hl_valid.min(row);
        self.block_end.truncate(self.hl_valid);
    }

    pub fn insert_char(&mut self, ch: char) {
        let b = char_idx_to_byte(&self.lines[self.row], self.col);
        self.lines[self.row].insert(b, ch);
        self.col += 1;
        self.dirty = true;
        self.invalidate_hl(self.row);
    }

    pub fn newline(&mut self) {
        let b = char_idx_to_byte(&self.lines[self.row], self.col);
        let rest = self.lines[self.row].split_off(b);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
        self.dirty = true;
        self.invalidate_hl(self.row - 1);
        self.ensure_visible();
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            self.col -= 1;
            let b = char_idx_to_byte(&self.lines[self.row], self.col);
            self.lines[self.row].remove(b);
            self.dirty = true;
            self.invalidate_hl(self.row);
        } else if self.row > 0 {
            let cur = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
            self.lines[self.row].push_str(&cur);
            self.dirty = true;
            self.invalidate_hl(self.row);
            self.ensure_visible();
        }
    }

    pub fn delete(&mut self) {
        let len = self.lines[self.row].chars().count();
        if self.col < len {
            let b = char_idx_to_byte(&self.lines[self.row], self.col);
            self.lines[self.row].remove(b);
            self.dirty = true;
            self.invalidate_hl(self.row);
        } else if self.row + 1 < self.lines.len() {
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
            self.dirty = true;
            self.invalidate_hl(self.row);
        }
    }

    // -- clipboard --------------------------------------------------------
    // Line-based copy/cut/paste, VS Code style: with no selection the whole
    // current line is copied. Also syncs with the OS clipboard when available.

    /// Copy the current line, without moving the cursor.
    pub fn copy_line(&mut self) {
        let line = self.lines[self.row].clone();
        crate::fs::copy_to_clipboard(&line);
        self.clipboard = vec![line];
        self.message = String::from("Copied line");
    }

    /// Cut the current line (copy it, then remove it).
    pub fn cut_line(&mut self) {
        let line = self.lines[self.row].clone();
        crate::fs::copy_to_clipboard(&line);
        self.clipboard = vec![line];
        if self.lines.len() > 1 {
            self.lines.remove(self.row);
            self.row = self.row.min(self.lines.len() - 1);
            self.clamp_col();
        } else {
            self.lines[0].clear();
            self.col = 0;
        }
        self.dirty = true;
        self.invalidate_hl(self.row);
        self.ensure_visible();
        self.message = String::from("Cut line");
    }

    /// Paste the copied lines below the current line. Falls back to the OS
    /// clipboard when the internal one is empty.
    pub fn paste(&mut self) {
        let clip: Vec<String> = if self.clipboard.is_empty() {
            match crate::fs::read_clipboard() {
                Some(s) if !s.trim().is_empty() => {
                    s.lines().map(|l| l.to_string()).collect()
                }
                _ => Vec::new(),
            }
        } else {
            self.clipboard.clone()
        };
        if clip.is_empty() {
            self.message = String::from("Clipboard is empty");
            return;
        }
        let n = clip.len();
        for (i, line) in clip.into_iter().enumerate() {
            self.lines.insert(self.row + 1 + i, line);
        }
        self.row += 1;
        self.col = 0;
        self.dirty = true;
        self.invalidate_hl(self.row - 1);
        self.ensure_visible();
        self.message = format!("Pasted {} line{}", n, if n == 1 { "" } else { "s" });
    }

    // -- cursor movement ----------------------------------------------------

    fn clamp_col(&mut self) {
        self.col = self.col.min(self.lines[self.row].chars().count());
    }

    fn ensure_visible(&mut self) {
        if self.view_h == 0 {
            self.view_h = 24;
        }
        let max_row = self.lines.len().saturating_sub(1);
        if self.row > max_row {
            self.row = max_row;
            self.clamp_col();
        }
        if self.row < self.offset {
            self.offset = self.row;
        } else if self.row >= self.offset + self.view_h {
            self.offset = self.row - self.view_h + 1;
        }
        if self.offset > self.row {
            self.offset = self.row;
        }
    }

    pub fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
            self.ensure_visible();
        }
    }

    pub fn move_right(&mut self) {
        if self.col < self.lines[self.row].chars().count() {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
            self.ensure_visible();
        }
    }

    pub fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.clamp_col();
            self.ensure_visible();
        }
    }

    pub fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.clamp_col();
            self.ensure_visible();
        }
    }

    pub fn home(&mut self) {
        self.col = 0;
    }

    pub fn end(&mut self) {
        self.col = self.lines[self.row].chars().count();
    }

    pub fn page_up(&mut self) {
        let h = self.view_h.max(1);
        self.row = self.row.saturating_sub(h);
        self.offset = self.offset.saturating_sub(h);
        self.clamp_col();
    }

    pub fn page_down(&mut self) {
        let h = self.view_h.max(1);
        self.row = (self.row + h).min(self.lines.len().saturating_sub(1));
        self.clamp_col();
        self.ensure_visible();
    }

    // -- highlighting -------------------------------------------------------

    fn ensure_hl(&mut self, upto: usize) {
        let upto = upto.min(self.lines.len());
        if self.block_end.len() < upto {
            self.block_end.resize(upto, false);
        }
        let mut state = if self.hl_valid == 0 { false } else { self.block_end[self.hl_valid - 1] };
        let mut i = self.hl_valid;
        while i < upto {
            let (_, end) = scan_line(&self.lines[i], self.lang, state);
            state = end;
            self.block_end[i] = state;
            i += 1;
        }
        self.hl_valid = upto.max(self.hl_valid);
    }

    /// Highlighted spans for the visible window [offset, offset+height).
    pub fn highlight_visible(&mut self, offset: usize, height: usize) -> Vec<Vec<Span<'static>>> {
        let end = (offset + height).min(self.lines.len());
        self.ensure_hl(end);
        let mut out = Vec::new();
        for i in offset..end {
            let start_state = if i == 0 { false } else { self.block_end[i - 1] };
            let (segs, _) = scan_line(&self.lines[i], self.lang, start_state);
            out.push(segs.into_iter().map(|(k, s)| span_for(k, s)).collect());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpfile(name: &str, content: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fex-edtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn edit_and_save_roundtrip() {
        let p = tmpfile("t.rs", b"fn main() {}\n");
        let mut ed = Editor::open(&p).unwrap();
        assert_eq!(ed.lang, Lang::Rust);
        assert!(!ed.dirty);
        ed.end();
        ed.newline();
        for ch in "let x = 1;".chars() {
            ed.insert_char(ch);
        }
        assert!(ed.dirty);
        ed.save().unwrap();
        assert!(!ed.dirty);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "fn main() {}\nlet x = 1;\n");
        // Backspace at line start merges with the previous line.
        ed.backspace(); // deletes ';'
        assert_eq!(ed.lines[1], "let x = 1");
        ed.col = 0;
        ed.backspace(); // merges line 1 into line 0
        assert_eq!(ed.lines.len(), 1);
        assert_eq!(ed.lines[0], "fn main() {}let x = 1");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn binary_and_oversize_rejected() {
        let p = tmpfile("b.bin", &[0u8, 1, 2, 3]);
        assert!(Editor::open(&p).is_err());
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn highlight_marks_keywords_strings_comments() {
        let (segs, _) = scan_line(r#"fn main() { // hello"#, Lang::Rust, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "fn"));
        assert!(segs.iter().any(|(k, s)| *k == Tok::Comment && s.contains("hello")));
        let (segs, _) = scan_line(r#"let s = "hi";"#, Lang::Rust, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Str && s == r#""hi""#));
        // // inside a string is not a comment
        let (segs, _) = scan_line(r#"let u = "http://x";"#, Lang::Rust, false);
        assert!(!segs.iter().any(|(k, _)| *k == Tok::Comment));
        // block comment carries across lines
        let (_, in_block) = scan_line("/* start", Lang::Rust, false);
        assert!(in_block);
        let (segs, in_block) = scan_line("still going */ let x = 1;", Lang::Rust, in_block);
        assert!(!in_block);
        assert!(segs.iter().any(|(k, _)| *k == Tok::Comment));
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "let"));
    }

    #[test]
    fn rust_lifetime_not_a_string() {
        let (segs, _) = scan_line("fn foo<'a>(x: &'a str) {}", Lang::Rust, false);
        assert!(!segs.iter().any(|(k, _)| *k == Tok::Str));
        let (segs, _) = scan_line("let c = 'x';", Lang::Rust, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Str && s == "'x'"));
    }

    #[test]
    fn copy_cut_paste_lines() {
        // Own temp dir: the shared `tmpfile` helper's dir is removed by other
        // tests running in parallel.
        let dir = std::env::temp_dir().join(format!("fex-edtest-clipboard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("c.txt");
        std::fs::write(&p, b"one\ntwo\nthree\n").unwrap();
        let mut ed = Editor::open(&p).unwrap();
        // Ctrl+C equivalent: copy current line, cursor stays put.
        ed.copy_line();
        assert_eq!(ed.clipboard, vec!["one".to_string()]);
        assert_eq!(ed.row, 0);
        // Paste below the current line.
        ed.move_down();
        ed.paste();
        assert_eq!(ed.lines, vec!["one", "two", "one", "three"]);
        assert_eq!(ed.row, 2);
        assert!(ed.dirty);
        // Cut removes the line and copies it.
        ed.cut_line();
        assert_eq!(ed.clipboard, vec!["one".to_string()]);
        assert_eq!(ed.lines, vec!["one", "two", "three"]);
        // A fresh paste still works after more edits.
        ed.move_up();
        ed.paste();
        assert_eq!(ed.lines, vec!["one", "two", "one", "three"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
