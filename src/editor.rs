//! Built-in text editor with syntax highlighting.
//!
//! Opened with `e` on a text file. Ctrl+S saves, Esc closes
//! (asking about unsaved changes).

use crate::theme::Theme;
use ratatui::{
    style::{Modifier, Style},
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
    Go,
    C,
    Cpp,
    Java,
    CSharp,
    Ruby,
    Html,
    Css,
    Sql,
    Yaml,
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
            "go" => Lang::Go,
            "c" | "h" => Lang::C,
            "cpp" | "hpp" | "cc" | "cxx" => Lang::Cpp,
            "java" => Lang::Java,
            "cs" => Lang::CSharp,
            "rb" => Lang::Ruby,
            "html" | "htm" | "xml" | "svg" => Lang::Html,
            "css" => Lang::Css,
            "sql" => Lang::Sql,
            "yaml" | "yml" => Lang::Yaml,
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
                "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match",
                "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
                "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            Lang::Python => &[
                "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
                "continue", "def", "del", "elif", "else", "except", "finally", "for", "from",
                "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass",
                "raise", "return", "try", "while", "with", "yield",
            ],
            Lang::JavaScript => &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "false",
                "finally",
                "for",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "null",
                "of",
                "return",
                "static",
                "super",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "var",
                "void",
                "while",
                "with",
                "yield",
            ],
            Lang::Toml | Lang::Json => &["true", "false", "null"],
            Lang::Go => &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
                "true",
                "false",
                "nil",
            ],
            Lang::C => &[
                "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
                "else", "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long",
                "register", "restrict", "return", "short", "signed", "sizeof", "static", "struct",
                "switch", "typedef", "union", "unsigned", "void", "volatile", "while", "true",
                "false", "NULL",
            ],
            Lang::Cpp => &[
                "alignas",
                "alignof",
                "asm",
                "auto",
                "break",
                "case",
                "catch",
                "char",
                "class",
                "const",
                "concept",
                "consteval",
                "constexpr",
                "constinit",
                "continue",
                "decltype",
                "default",
                "delete",
                "do",
                "double",
                "else",
                "enum",
                "explicit",
                "export",
                "extern",
                "false",
                "float",
                "for",
                "friend",
                "goto",
                "if",
                "inline",
                "int",
                "long",
                "mutable",
                "namespace",
                "new",
                "noexcept",
                "nullptr",
                "operator",
                "private",
                "protected",
                "public",
                "register",
                "return",
                "short",
                "signed",
                "sizeof",
                "static",
                "struct",
                "switch",
                "template",
                "this",
                "throw",
                "true",
                "try",
                "typedef",
                "typeid",
                "typename",
                "union",
                "unsigned",
                "using",
                "virtual",
                "void",
                "volatile",
                "while",
                "NULL",
            ],
            Lang::Java => &[
                "abstract",
                "assert",
                "boolean",
                "break",
                "byte",
                "case",
                "catch",
                "char",
                "class",
                "const",
                "continue",
                "default",
                "do",
                "double",
                "else",
                "enum",
                "extends",
                "final",
                "finally",
                "float",
                "for",
                "goto",
                "if",
                "implements",
                "import",
                "instanceof",
                "int",
                "interface",
                "long",
                "native",
                "new",
                "package",
                "private",
                "protected",
                "public",
                "return",
                "short",
                "static",
                "strictfp",
                "super",
                "switch",
                "synchronized",
                "this",
                "throw",
                "throws",
                "transient",
                "try",
                "void",
                "volatile",
                "while",
                "true",
                "false",
                "null",
            ],
            Lang::CSharp => &[
                "abstract",
                "as",
                "base",
                "bool",
                "break",
                "byte",
                "case",
                "catch",
                "char",
                "checked",
                "class",
                "const",
                "continue",
                "decimal",
                "default",
                "delegate",
                "do",
                "double",
                "else",
                "enum",
                "event",
                "explicit",
                "extern",
                "false",
                "finally",
                "fixed",
                "float",
                "for",
                "foreach",
                "goto",
                "if",
                "implicit",
                "in",
                "int",
                "interface",
                "internal",
                "is",
                "lock",
                "long",
                "namespace",
                "new",
                "null",
                "object",
                "operator",
                "out",
                "override",
                "params",
                "private",
                "protected",
                "public",
                "readonly",
                "ref",
                "return",
                "sbyte",
                "sealed",
                "short",
                "sizeof",
                "stackalloc",
                "static",
                "string",
                "struct",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "uint",
                "ulong",
                "unchecked",
                "unsafe",
                "ushort",
                "using",
                "virtual",
                "void",
                "volatile",
                "while",
            ],
            Lang::Ruby => &[
                "BEGIN", "END", "alias", "and", "begin", "break", "case", "class", "def",
                "defined?", "do", "else", "elsif", "end", "ensure", "false", "for", "if", "in",
                "module", "next", "nil", "not", "or", "redo", "rescue", "retry", "return", "self",
                "super", "then", "true", "undef", "unless", "until", "when", "while", "yield",
            ],
            Lang::Sql => &[
                "select",
                "from",
                "where",
                "join",
                "inner",
                "outer",
                "left",
                "right",
                "full",
                "on",
                "group",
                "by",
                "order",
                "having",
                "limit",
                "offset",
                "insert",
                "into",
                "values",
                "update",
                "set",
                "delete",
                "create",
                "table",
                "index",
                "view",
                "alter",
                "drop",
                "as",
                "and",
                "or",
                "not",
                "null",
                "like",
                "in",
                "is",
                "between",
                "exists",
                "case",
                "when",
                "then",
                "else",
                "end",
                "distinct",
                "union",
                "all",
                "primary",
                "key",
                "foreign",
                "references",
                "default",
                "count",
                "sum",
                "avg",
                "min",
                "max",
            ],
            Lang::Css => &[
                "charset",
                "counter-style",
                "document",
                "font-face",
                "font-feature-values",
                "import",
                "keyframes",
                "media",
                "namespace",
                "page",
                "supports",
            ],
            Lang::Yaml => &["true", "false", "null", "yes", "no", "on", "off"],
            Lang::Html => &[],
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
            Lang::Rust
            | Lang::JavaScript
            | Lang::Go
            | Lang::C
            | Lang::Cpp
            | Lang::Java
            | Lang::CSharp
            | Lang::Css => Some("//"),
            Lang::Python | Lang::Shell | Lang::Toml | Lang::Ruby | Lang::Yaml => Some("#"),
            Lang::Sql => Some("--"),
            Lang::Json | Lang::Markdown | Lang::Html | Lang::Plain => None,
        }
    }

    fn block_comment(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Lang::Rust
            | Lang::JavaScript
            | Lang::Go
            | Lang::C
            | Lang::Cpp
            | Lang::Java
            | Lang::CSharp
            | Lang::Css
            | Lang::Sql => Some(("/*", "*/")),
            Lang::Python => Some(("'''", "'''")),
            Lang::Html => Some(("<!--", "-->")),
            _ => None,
        }
    }

    fn string_delims(&self) -> &'static [char] {
        match self {
            Lang::Rust => &['"'],
            Lang::Sql => &['\''],
            Lang::Markdown | Lang::Plain | Lang::Html => &['"'],
            Lang::JavaScript => &['"', '\'', '`'],
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
    /// Capitalized identifiers: types, classes, decorators, attributes.
    Type,
    /// Identifier immediately followed by `(`: a function/method call.
    Func,
}

/// Accumulates highlighted segments, merging runs of the same kind.
struct SegBuf {
    segs: Vec<(Tok, String)>,
    cur: String,
    kind: Tok,
}

impl SegBuf {
    fn new() -> Self {
        Self {
            segs: Vec::new(),
            cur: String::new(),
            kind: Tok::Normal,
        }
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

fn classify_word(run: &str, lang: Lang, is_call: bool) -> Tok {
    let kws = lang.keywords();
    // SQL is conventionally written in any case; match keywords loosely.
    let is_kw = if lang == Lang::Sql {
        kws.iter().any(|k| k.eq_ignore_ascii_case(run))
    } else {
        kws.contains(&run)
    };
    if is_kw {
        Tok::Keyword
    } else if run.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Tok::Number
    } else if run.chars().next().is_some_and(|c| c.is_uppercase()) {
        Tok::Type
    } else if is_call {
        Tok::Func
    } else {
        Tok::Normal
    }
}

/// Split Normal segments into keyword/number/type/call/identifier runs.
/// A word run directly followed by `(` is a function call.
fn postprocess(segs: Vec<(Tok, String)>, lang: Lang) -> Vec<(Tok, String)> {
    if lang.keywords().is_empty() {
        return segs;
    }
    let mut out = Vec::with_capacity(segs.len());
    for (kind, s) in segs {
        if kind != Tok::Normal {
            out.push((kind, s));
            continue;
        }
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let w = chars[i].is_alphanumeric() || chars[i] == '_';
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') == w {
                j += 1;
            }
            let run: String = chars[i..j].iter().collect();
            let kind = if w {
                classify_word(&run, lang, chars.get(j) == Some(&'('))
            } else {
                Tok::Normal
            };
            out.push((kind, run));
            i = j;
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

    // C-style preprocessor directive at the start of the line: color the
    // `#directive` word as a keyword, scan the rest normally.
    if !in_block && matches!(lang, Lang::C | Lang::Cpp) {
        let trimmed = work.trim_start();
        if trimmed.starts_with('#') {
            let skip = work.len() - trimmed.len();
            let after_hash = &trimmed[1..];
            let dir_len = after_hash
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(after_hash.len());
            let e = skip + 1 + dir_len;
            buf.feed(Tok::Keyword, &work[skip..e]);
            work = &work[e..];
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

        // Python decorators / Java annotations: @name colored as a type.
        if matches!(lang, Lang::Python | Lang::Java) && ch == '@' {
            let mut e = k + 1;
            while e < n && (idx[e].1.is_alphanumeric() || idx[e].1 == '_') {
                e += 1;
            }
            if e > k + 1 {
                let end_b = if e < n { idx[e].0 } else { work.len() };
                buf.feed(Tok::Type, &work[b..end_b]);
                k = e;
                continue;
            }
        }

        // Rust attributes: #[...] colored as a type.
        if lang == Lang::Rust && ch == '#' {
            if let Some(after) = rest.strip_prefix("#[") {
                if let Some(p) = after.find(']') {
                    let e = b + 2 + p + 1;
                    buf.feed(Tok::Type, &work[b..e]);
                    k = idx.iter().position(|(bb, _)| *bb >= e).unwrap_or(n);
                    continue;
                }
            }
        }

        // CSS at-rules: @media, @import, ...
        if lang == Lang::Css && ch == '@' {
            let mut e = k + 1;
            while e < n && (idx[e].1.is_alphanumeric() || idx[e].1 == '_' || idx[e].1 == '-') {
                e += 1;
            }
            if e > k + 1 {
                let end_b = if e < n { idx[e].0 } else { work.len() };
                buf.feed(Tok::Keyword, &work[b..end_b]);
                k = e;
                continue;
            }
        }

        // HTML tags: <name ...> — brackets plain, tag name keyword,
        // quoted attribute values as strings. <!-- --> is handled by the
        // block-comment check above; <!DOCTYPE ...> becomes a comment.
        if lang == Lang::Html && ch == '<' {
            if rest.starts_with("<!") {
                match rest.find('>') {
                    Some(p) => {
                        let e = b + p + 1;
                        buf.feed(Tok::Comment, &work[b..e]);
                        k = idx.iter().position(|(bb, _)| *bb >= e).unwrap_or(n);
                    }
                    None => {
                        buf.feed(Tok::Comment, rest);
                        k = n;
                    }
                }
                continue;
            }
            let after_slash = rest[1..].strip_prefix('/').unwrap_or(&rest[1..]);
            if after_slash
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            {
                if let Some(p) = rest.find('>') {
                    feed_html_tag(&mut buf, &rest[..p + 1]);
                    let e = b + p + 1;
                    k = idx.iter().position(|(bb, _)| *bb >= e).unwrap_or(n);
                    continue;
                }
            }
        }

        // Rust char literal: 'x' or '\n' (a bare ' would clash with lifetimes).
        if lang == Lang::Rust && ch == '\'' {
            let escaped_lit = idx.get(k + 1).map(|(_, c)| *c) == Some('\\')
                && idx.get(k + 3).map(|(_, c)| *c) == Some('\'');
            let plain_lit = idx.get(k + 2).map(|(_, c)| *c) == Some('\'');
            let take = if escaped_lit {
                4
            } else if plain_lit {
                3
            } else {
                0
            };
            if take > 0 {
                let end_b = if k + take < n {
                    idx[k + take].0
                } else {
                    work.len()
                };
                buf.feed(Tok::Str, &work[b..end_b]);
                k += take;
                continue;
            }
            // Rust lifetime: 'a — color as a type.
            if idx
                .get(k + 1)
                .is_some_and(|(_, c)| c.is_alphanumeric() || *c == '_')
            {
                let mut e = k + 1;
                while e < n && (idx[e].1.is_alphanumeric() || idx[e].1 == '_') {
                    e += 1;
                }
                let end_b = if e < n { idx[e].0 } else { work.len() };
                buf.feed(Tok::Type, &work[b..end_b]);
                k = e;
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

/// Feed an HTML tag `<name attrs...>`: brackets plain, the tag name as a
/// keyword, quoted attribute values as strings.
fn feed_html_tag(buf: &mut SegBuf, tag: &str) {
    let mut chars = tag.chars().peekable();
    if chars.next() == Some('<') {
        buf.feed_ch(Tok::Normal, '<');
    }
    if chars.peek() == Some(&'/') {
        chars.next();
        buf.feed_ch(Tok::Normal, '/');
    }
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphanumeric() || c == '-' || c == ':' {
            name.push(c);
            chars.next();
        } else {
            break;
        }
    }
    buf.feed(Tok::Keyword, &name);
    while let Some(c) = chars.next() {
        if c == '"' || c == '\'' {
            let mut s = String::new();
            s.push(c);
            for d in chars.by_ref() {
                s.push(d);
                if d == c {
                    break;
                }
            }
            buf.feed(Tok::Str, &s);
        } else {
            buf.feed_ch(Tok::Normal, c);
        }
    }
}

fn span_for(theme: &Theme, kind: Tok, s: String) -> Span<'static> {
    let style = match kind {
        Tok::Normal => Style::default(),
        Tok::Keyword => Style::default()
            .fg(theme.syn_keyword)
            .add_modifier(Modifier::BOLD),
        Tok::Str => Style::default().fg(theme.syn_string),
        Tok::Comment => Style::default()
            .fg(theme.syn_comment)
            .add_modifier(Modifier::ITALIC),
        Tok::Number => Style::default().fg(theme.syn_number),
        Tok::Type => Style::default()
            .fg(theme.syn_type)
            .add_modifier(Modifier::BOLD),
        Tok::Func => Style::default().fg(theme.syn_func),
    };
    Span::styled(s, style)
}

fn char_idx_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// One undo/redo snapshot: the full buffer plus the cursor.
#[derive(Debug, Clone)]
struct UndoSnap {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

/// Cap on the undo history (each entry clones the buffer).
const MAX_UNDO: usize = 200;

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
    /// Exact text copied from a selection (pasted inline at the cursor).
    pub clipboard_inline: Option<String>,
    /// Mouse selection anchor (row, col); the cursor is the other end.
    /// `None` means no selection.
    pub sel_anchor: Option<(usize, usize)>,
    /// Top-left of the rendered text area, for mapping mouse clicks.
    pub view_x: u16,
    pub view_y: u16,
    /// Line-number gutter toggle (synced from App on open).
    pub show_line_numbers: bool,
    lang: Lang,
    trailing_newline: bool,
    block_end: Vec<bool>, // in-block-comment at end of line i
    hl_valid: usize,      // rows [0, hl_valid) have valid block_end entries
    undo: Vec<UndoSnap>,
    redo: Vec<UndoSnap>,
}

impl Editor {
    pub fn open(path: &Path) -> io::Result<Editor> {
        let meta = fs::metadata(path)?;
        if meta.len() > MAX_EDIT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "file too large to edit",
            ));
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
            clipboard_inline: None,
            sel_anchor: None,
            view_x: 0,
            view_y: 0,
            lang: Lang::from_path(path),
            trailing_newline,
            show_line_numbers: true,
            block_end: Vec::new(),
            hl_valid: 0,
            undo: Vec::new(),
            redo: Vec::new(),
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

    /// A blank, unsaved document (empty path). Used for "new text" tabs;
    /// the save dialog assigns a real path on first save.
    pub fn untitled() -> Editor {
        Editor {
            path: PathBuf::new(),
            lines: vec![String::new()],
            row: 0,
            col: 0,
            offset: 0,
            view_h: 24,
            dirty: false,
            message: String::new(),
            clipboard: Vec::new(),
            clipboard_inline: None,
            sel_anchor: None,
            view_x: 0,
            view_y: 0,
            lang: Lang::Plain,
            trailing_newline: true,
            show_line_numbers: true,
            block_end: Vec::new(),
            hl_valid: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Save to a new path (the save-as flow): adopts the path, picks up
    /// syntax highlighting from its extension, then saves normally.
    pub fn save_to(&mut self, path: &Path) -> io::Result<()> {
        self.path = path.to_path_buf();
        self.lang = Lang::from_path(path);
        self.hl_valid = 0;
        self.block_end.clear();
        self.save()
    }

    /// True for a never-saved document (no path yet).
    pub fn is_untitled(&self) -> bool {
        self.path.as_os_str().is_empty()
    }

    // -- undo / redo ------------------------------------------------------
    // Snapshot-based: every mutating operation pushes the pre-edit state.

    fn push_undo(&mut self) {
        let snap = UndoSnap {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
        };
        if let Some(top) = self.undo.last() {
            if top.lines == snap.lines && top.row == snap.row && top.col == snap.col {
                return;
            }
        }
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(snap);
        self.redo.clear();
    }

    fn restore(&mut self, snap: UndoSnap) {
        self.lines = snap.lines;
        self.row = snap.row;
        self.col = snap.col;
        self.sel_anchor = None;
        self.dirty = true;
        self.invalidate_hl(0);
        self.ensure_visible();
    }

    pub fn undo(&mut self) {
        let Some(snap) = self.undo.pop() else {
            self.message = String::from("Nothing to undo");
            return;
        };
        self.redo.push(UndoSnap {
            lines: std::mem::take(&mut self.lines),
            row: self.row,
            col: self.col,
        });
        self.restore(snap);
        self.message = String::from("Undone");
    }

    pub fn redo(&mut self) {
        let Some(snap) = self.redo.pop() else {
            self.message = String::from("Nothing to redo");
            return;
        };
        self.undo.push(UndoSnap {
            lines: std::mem::take(&mut self.lines),
            row: self.row,
            col: self.col,
        });
        self.restore(snap);
        self.message = String::from("Redone");
    }

    // -- editing ----------------------------------------------------------

    fn invalidate_hl(&mut self, row: usize) {
        self.hl_valid = self.hl_valid.min(row);
        self.block_end.truncate(self.hl_valid);
    }

    pub fn insert_char(&mut self, ch: char) {
        self.push_undo();
        self.remove_selection();
        let b = char_idx_to_byte(&self.lines[self.row], self.col);
        self.lines[self.row].insert(b, ch);
        self.col += 1;
        self.dirty = true;
        self.invalidate_hl(self.row);
    }

    /// Insert a whole string at the cursor at once (used for pasting:
    /// instant, one undo step, cancellable by nature).
    pub fn insert_text(&mut self, text: &str) {
        let text: String = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .take(500_000)
            .collect();
        if text.is_empty() {
            return;
        }
        self.push_undo();
        self.remove_selection();
        let start_row = self.row;
        let parts: Vec<&str> = text.split('\n').collect();
        let b = char_idx_to_byte(&self.lines[self.row], self.col);
        let tail = self.lines[self.row].split_off(b);
        self.lines[self.row].push_str(parts[0]);
        let mut row = self.row;
        for part in &parts[1..] {
            row += 1;
            self.lines.insert(row, part.to_string());
        }
        self.lines[row].push_str(&tail);
        self.row = row;
        self.col = parts.last().map(|s| s.chars().count()).unwrap_or(0);
        self.dirty = true;
        self.invalidate_hl(start_row);
        self.ensure_visible();
        self.message.clear();
    }

    pub fn newline(&mut self) {
        self.push_undo();
        self.remove_selection();
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
        if self.sel_anchor.is_some() {
            self.delete_selection();
            return;
        }
        if self.col > 0 {
            self.push_undo();
            self.col -= 1;
            let b = char_idx_to_byte(&self.lines[self.row], self.col);
            self.lines[self.row].remove(b);
            self.dirty = true;
            self.invalidate_hl(self.row);
        } else if self.row > 0 {
            self.push_undo();
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
        if self.sel_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let len = self.lines[self.row].chars().count();
        if self.col < len {
            self.push_undo();
            let b = char_idx_to_byte(&self.lines[self.row], self.col);
            self.lines[self.row].remove(b);
            self.dirty = true;
            self.invalidate_hl(self.row);
        } else if self.row + 1 < self.lines.len() {
            self.push_undo();
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
            self.dirty = true;
            self.invalidate_hl(self.row);
        }
    }

    // -- clipboard --------------------------------------------------------
    // Line-based copy/cut/paste, VS Code style: with no selection the whole
    // current line is copied. With a mouse selection, the exact selected
    // text is copied (never the line-number gutter). Also syncs with the OS
    // clipboard when available.

    /// Normalized selection ((start_row, start_col), (end_row, end_col)).
    pub fn selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let a = self.sel_anchor?;
        let b = (self.row, self.col);
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    pub fn clear_selection(&mut self) {
        self.sel_anchor = None;
    }

    /// Select the whole buffer: anchor at the start, cursor at the end.
    pub fn select_all(&mut self) {
        self.sel_anchor = Some((0, 0));
        let last = self.lines.len().saturating_sub(1);
        self.row = last;
        self.col = self.lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    }

    /// Text covered by the current selection.
    fn selected_text(&self) -> Option<String> {
        let ((sr, sc), (er, ec)) = self.selection()?;
        if (sr, sc) == (er, ec) {
            return None;
        }
        let mut parts = Vec::new();
        for r in sr..=er {
            let line = &self.lines[r];
            let len = line.chars().count();
            let s = sc.min(len);
            let e = if r == er { ec.min(len) } else { len };
            let s = if r == sr { s } else { 0 };
            let sb = char_idx_to_byte(line, s);
            let eb = char_idx_to_byte(line, e.max(s));
            parts.push(line[sb..eb].to_string());
        }
        Some(parts.join("\n"))
    }

    /// Delete the selected text (single undo step). Returns false when there
    /// is no selection.
    fn delete_selection(&mut self) -> bool {
        if self.selection().is_none() {
            self.sel_anchor = None;
            return false;
        }
        self.push_undo();
        self.remove_selection();
        true
    }

    /// Remove the selected text without recording undo (the caller already
    /// pushed a snapshot, e.g. when replacing a selection by typing).
    fn remove_selection(&mut self) {
        let ((sr, sc), (er, ec)) = match self.selection() {
            Some(sel) => sel,
            None => {
                self.sel_anchor = None;
                return;
            }
        };
        if (sr, sc) == (er, ec) {
            self.sel_anchor = None;
            return;
        }
        if sr == er {
            let line = &mut self.lines[sr];
            let len = line.chars().count();
            let sb = char_idx_to_byte(line, sc.min(len));
            let eb = char_idx_to_byte(line, ec.min(len).max(sc.min(len)));
            line.replace_range(sb..eb, "");
        } else {
            let tail: String = {
                let line = &self.lines[er];
                let len = line.chars().count();
                let eb = char_idx_to_byte(line, ec.min(len));
                line[eb..].to_string()
            };
            {
                let line = &mut self.lines[sr];
                let len = line.chars().count();
                let sb = char_idx_to_byte(line, sc.min(len));
                line.truncate(sb);
            }
            self.lines[sr].push_str(&tail);
            self.lines.drain(sr + 1..=er);
        }
        self.row = sr;
        self.col = sc.min(self.lines[sr].chars().count());
        self.sel_anchor = None;
        self.dirty = true;
        self.invalidate_hl(sr);
        self.ensure_visible();
    }

    /// Copy the selection if there is one, else the current line.
    /// The line-number gutter is display-only and never copied.
    pub fn copy_line(&mut self) {
        if let Some(text) = self.selected_text() {
            crate::fs::copy_to_clipboard(&text);
            self.clipboard_inline = Some(text);
            self.message = String::from("Copied selection");
            return;
        }
        let line = self.lines[self.row].clone();
        crate::fs::copy_to_clipboard(&line);
        self.clipboard = vec![line];
        self.clipboard_inline = None;
        self.message = String::from("Copied line");
    }

    /// Cut the selection if there is one, else the current line.
    pub fn cut_line(&mut self) {
        if let Some(text) = self.selected_text() {
            crate::fs::copy_to_clipboard(&text);
            self.clipboard_inline = Some(text.clone());
            self.delete_selection();
            self.message = String::from("Cut selection");
            return;
        }
        let line = self.lines[self.row].clone();
        crate::fs::copy_to_clipboard(&line);
        self.clipboard = vec![line];
        self.clipboard_inline = None;
        self.push_undo();
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

    /// Paste: exact selected text goes inline at the cursor; copied lines go
    /// below the current line (VS Code style). Falls back to the OS
    /// clipboard when the internal one is empty.
    pub fn paste(&mut self) {
        if let Some(text) = self.clipboard_inline.clone() {
            self.insert_text(&text);
            self.message = String::from("Pasted");
            return;
        }
        let clip: Vec<String> = if self.clipboard.is_empty() {
            match crate::fs::read_clipboard() {
                Some(s) if !s.trim().is_empty() => s.lines().map(|l| l.to_string()).collect(),
                _ => Vec::new(),
            }
        } else {
            self.clipboard.clone()
        };
        if clip.is_empty() {
            self.message = String::from("Clipboard is empty");
            return;
        }
        self.push_undo();
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

    // -- mouse --------------------------------------------------------------

    /// Width of the line-number gutter for the current buffer; zero when
    /// line numbers are toggled off.
    fn gutter_w(&self) -> u16 {
        if !self.show_line_numbers {
            return 0;
        }
        (self.lines.len().to_string().len().max(2) + 1) as u16
    }

    /// Move the cursor to a terminal click at (x, y).
    pub fn click_at(&mut self, x: u16, y: u16) {
        let tx = self.view_x + 1 + self.gutter_w();
        let ty = self.view_y + 1;
        if x < tx || y < ty {
            return;
        }
        let row = (self.offset + (y - ty) as usize).min(self.lines.len().saturating_sub(1));
        let col = (x - tx) as usize;
        self.row = row;
        self.col = col.min(self.lines[row].chars().count());
        self.ensure_visible();
    }

    /// Scroll the view by `delta` lines (mouse wheel).
    pub fn scroll_by(&mut self, delta: isize) {
        let max_off = self.lines.len().saturating_sub(1);
        let new = (self.offset as isize + delta).clamp(0, max_off as isize) as usize;
        self.offset = new;
        if self.row < self.offset || self.row >= self.offset + self.view_h.max(1) {
            self.row = self.offset;
            self.clamp_col();
        }
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

    /// Word characters for word-wise cursor motion (Ctrl+Left / Ctrl+Right).
    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// Move to the start of the previous word (Ctrl+Left), crossing lines.
    pub fn word_start(&mut self) {
        let (mut r, mut c) = (self.row, self.col);
        // Step one position left so repeated presses keep moving.
        if c > 0 {
            c -= 1;
        } else if r > 0 {
            r -= 1;
            c = self.lines[r].chars().count();
        } else {
            return;
        }
        loop {
            let line: Vec<char> = self.lines[r].chars().collect();
            while c > 0 && !Self::is_word_char(line[c - 1]) {
                c -= 1;
            }
            if c > 0 {
                while c > 0 && Self::is_word_char(line[c - 1]) {
                    c -= 1;
                }
                break;
            }
            if r == 0 {
                break;
            }
            r -= 1;
            c = self.lines[r].chars().count();
        }
        self.row = r;
        self.col = c;
        self.ensure_visible();
    }

    /// Move to the end of the next word (Ctrl+Right), crossing lines.
    pub fn word_end(&mut self) {
        let (mut r, mut c) = (self.row, self.col);
        let cur_is_word = self.lines[r].chars().nth(c).is_some_and(Self::is_word_char);
        if !cur_is_word {
            // Step one position right so a cursor on a separator or at a
            // word end advances to the following word.
            let line_len = self.lines[r].chars().count();
            if c < line_len {
                c += 1;
            } else if r + 1 < self.lines.len() {
                r += 1;
                c = 0;
            } else {
                return;
            }
        }
        loop {
            let line: Vec<char> = self.lines[r].chars().collect();
            while c < line.len() && !Self::is_word_char(line[c]) {
                c += 1;
            }
            if c < line.len() {
                let start = c;
                while c < line.len() && Self::is_word_char(line[c]) {
                    c += 1;
                }
                if c > start {
                    break;
                }
            }
            if r + 1 >= self.lines.len() {
                break;
            }
            r += 1;
            c = 0;
        }
        self.row = r;
        self.col = c;
        self.ensure_visible();
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
        let mut state = if self.hl_valid == 0 {
            false
        } else {
            self.block_end[self.hl_valid - 1]
        };
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
    pub fn highlight_visible(
        &mut self,
        theme: &Theme,
        offset: usize,
        height: usize,
    ) -> Vec<Vec<Span<'static>>> {
        let end = (offset + height).min(self.lines.len());
        self.ensure_hl(end);
        let mut out = Vec::new();
        for i in offset..end {
            let start_state = if i == 0 { false } else { self.block_end[i - 1] };
            let (segs, _) = scan_line(&self.lines[i], self.lang, start_state);
            out.push(
                segs.into_iter()
                    .map(|(k, s)| span_for(theme, k, s))
                    .collect(),
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpfile(name: &str, content: &[u8]) -> PathBuf {
        // Unique per call: tests run in parallel threads sharing one process.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fex-edtest-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
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
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "fn main() {}\nlet x = 1;\n"
        );
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
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Comment && s.contains("hello")));
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
    fn lang_from_path_covers_new_languages() {
        use std::path::PathBuf;
        let cases = [
            ("a.go", Lang::Go),
            ("a.c", Lang::C),
            ("a.h", Lang::C),
            ("a.cpp", Lang::Cpp),
            ("a.hpp", Lang::Cpp),
            ("a.java", Lang::Java),
            ("a.cs", Lang::CSharp),
            ("a.rb", Lang::Ruby),
            ("a.html", Lang::Html),
            ("a.xml", Lang::Html),
            ("a.css", Lang::Css),
            ("a.sql", Lang::Sql),
            ("a.yaml", Lang::Yaml),
            ("a.yml", Lang::Yaml),
        ];
        for (name, want) in cases {
            assert_eq!(Lang::from_path(&PathBuf::from(name)), want, "{name}");
        }
    }

    #[test]
    fn highlight_function_calls_and_types() {
        let (segs, _) = scan_line("foo(bar);", Lang::Rust, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Func && s == "foo"));
        assert!(!segs.iter().any(|(k, s)| *k == Tok::Func && s == "bar"));
        let (segs, _) = scan_line("let v = Vec::new();", Lang::Rust, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Type && s == "Vec"));
        // keywords still win over call position
        let (segs, _) = scan_line("if (x) {}", Lang::Rust, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "if"));
    }

    #[test]
    fn highlight_python_decorator() {
        let (segs, _) = scan_line("@property", Lang::Python, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Type && s == "@property"));
        let (segs, _) = scan_line("x = a @ b", Lang::Python, false);
        assert!(!segs.iter().any(|(k, _)| *k == Tok::Type));
    }

    #[test]
    fn highlight_rust_attribute_and_lifetime() {
        let (segs, _) = scan_line("#[derive(Debug)]", Lang::Rust, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Type && s == "#[derive(Debug)]"));
        let (segs, _) = scan_line("fn f<'a>(x: &'a str) {}", Lang::Rust, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Type && s == "'a"));
    }

    #[test]
    fn highlight_c_preprocessor() {
        let (segs, _) = scan_line("#include <stdio.h>", Lang::C, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "#include"));
        let (segs, _) = scan_line("int main() {}", Lang::C, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "int"));
        assert!(segs.iter().any(|(k, s)| *k == Tok::Func && s == "main"));
    }

    #[test]
    fn highlight_html_tags() {
        let (segs, _) = scan_line(r#"<div class="x">hi</div>"#, Lang::Html, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "div"));
        assert!(segs.iter().any(|(k, s)| *k == Tok::Str && s == r#""x""#));
        assert!(!segs.iter().any(|(k, _)| *k == Tok::Comment));
        // a < b in text is not a tag
        let (segs, _) = scan_line("a < b", Lang::Html, false);
        assert!(!segs.iter().any(|(k, _)| *k == Tok::Keyword));
        // comments still work, across lines too
        let (segs, in_block) = scan_line("<!-- hello", Lang::Html, false);
        assert!(in_block);
        assert!(segs.iter().any(|(k, _)| *k == Tok::Comment));
        let (segs, in_block) = scan_line("world -->", Lang::Html, in_block);
        assert!(!in_block);
        assert!(segs.iter().any(|(k, _)| *k == Tok::Comment));
    }

    #[test]
    fn highlight_sql_case_insensitive() {
        let (segs, _) = scan_line("SELECT * FROM t; -- hi", Lang::Sql, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "SELECT"));
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "FROM"));
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Comment && s.contains("hi")));
        let (segs, _) = scan_line("select 1", Lang::Sql, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "select"));
    }

    #[test]
    fn highlight_css_at_rule_and_comment() {
        let (segs, _) = scan_line("@media screen {", Lang::Css, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "@media"));
        let (segs, in_block) = scan_line("/* hi", Lang::Css, false);
        assert!(in_block);
        assert!(segs.iter().any(|(k, _)| *k == Tok::Comment));
    }

    #[test]
    fn highlight_go_and_ruby() {
        let (segs, _) = scan_line("package main // hi", Lang::Go, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "package"));
        assert!(segs.iter().any(|(k, _)| *k == Tok::Comment));
        let (segs, _) = scan_line("def foo", Lang::Ruby, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "def"));
        let (segs, _) = scan_line("foo(1) # hi", Lang::Ruby, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Func && s == "foo"));
        assert!(segs.iter().any(|(k, _)| *k == Tok::Comment));
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

    #[test]
    fn insert_text_paste_is_instant_and_single_undo() {
        let dir = std::env::temp_dir().join(format!("fex-edtest-paste-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("p.txt");
        std::fs::write(&p, b"ab\ncd\n").unwrap();
        let mut ed = Editor::open(&p).unwrap();
        // Cursor between 'a' and 'b', paste a multi-line block at once.
        ed.col = 1;
        ed.insert_text("X\nY\nZ");
        assert_eq!(ed.lines, vec!["aX", "Y", "Zb", "cd"]);
        assert_eq!((ed.row, ed.col), (2, 1));
        assert!(ed.dirty);
        // One undo restores the whole paste.
        ed.undo();
        assert_eq!(ed.lines, vec!["ab", "cd"]);
        assert_eq!((ed.row, ed.col), (0, 1));
        // Redo brings it back.
        ed.redo();
        assert_eq!(ed.lines, vec!["aX", "Y", "Zb", "cd"]);
        // Typing then undo/redo round-trips a single char.
        ed.insert_char('!');
        assert_eq!(ed.lines[2], "Z!b");
        ed.undo();
        assert_eq!(ed.lines[2], "Zb");
        ed.redo();
        assert_eq!(ed.lines[2], "Z!b");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn selection_copy_cut_paste_exact_text() {
        let dir = std::env::temp_dir().join(format!("fex-edtest-sel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("s.txt");
        std::fs::write(&p, b"hello world\nsecond line\n").unwrap();
        let mut ed = Editor::open(&p).unwrap();
        // Select "lo wo" across part of line 0.
        ed.sel_anchor = Some((0, 3));
        ed.row = 0;
        ed.col = 8;
        ed.copy_line();
        assert_eq!(ed.clipboard_inline.as_deref(), Some("lo wo"));
        // Pasting the selection inserts it inline at the cursor.
        ed.sel_anchor = None;
        ed.col = 0;
        ed.paste();
        assert_eq!(ed.lines[0], "lo wohello world");
        // Cut a multi-line selection.
        ed.sel_anchor = Some((0, 0));
        ed.row = 1;
        ed.col = 6;
        ed.cut_line();
        assert_eq!(
            ed.clipboard_inline.as_deref(),
            Some("lo wohello world\nsecond")
        );
        assert_eq!(ed.lines, vec![" line"]);
        // Undo restores the cut.
        ed.undo();
        assert_eq!(ed.lines.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn typing_replaces_selection_in_one_undo() {
        let dir = std::env::temp_dir().join(format!("fex-edtest-rep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("r.txt");
        std::fs::write(&p, b"abcdef\n").unwrap();
        let mut ed = Editor::open(&p).unwrap();
        ed.sel_anchor = Some((0, 1));
        ed.row = 0;
        ed.col = 4;
        ed.insert_char('X');
        assert_eq!(ed.lines[0], "aXef");
        assert!(ed.sel_anchor.is_none());
        ed.undo();
        assert_eq!(ed.lines[0], "abcdef");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn click_maps_to_cursor() {
        let dir = std::env::temp_dir().join(format!("fex-edtest-click-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("m.txt");
        std::fs::write(&p, b"ab\ncdef\n").unwrap();
        let mut ed = Editor::open(&p).unwrap();
        // Text area at (10, 5); gutter is 3 wide (num_w=2 + 1 space).
        ed.view_x = 10;
        ed.view_y = 5;
        ed.click_at(10 + 1 + 3 + 2, 5 + 1 + 1); // col 2 of row 1
        assert_eq!((ed.row, ed.col), (1, 2));
        // Clicking past end of line clamps.
        ed.click_at(10 + 1 + 3 + 99, 5 + 1 + 0);
        assert_eq!((ed.row, ed.col), (0, 2));
        // Clicking outside the text area is ignored.
        ed.click_at(0, 0);
        assert_eq!((ed.row, ed.col), (0, 2));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn select_all_covers_whole_buffer() {
        let p = tmpfile("sel.txt", b"hello\nworld\n");
        let mut ed = Editor::open(&p).unwrap();
        ed.select_all();
        assert_eq!(ed.selection(), Some(((0, 0), (1, 5))));
        assert_eq!(ed.selected_text().as_deref(), Some("hello\nworld"));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[test]
    fn word_end_jumps_to_word_ends() {
        let p = tmpfile("words.txt", b"hello world\nfoo_bar baz\n");
        let mut ed = Editor::open(&p).unwrap();
        ed.word_end();
        assert_eq!((ed.row, ed.col), (0, 5)); // end of "hello"
        ed.word_end();
        assert_eq!((ed.row, ed.col), (0, 11)); // end of "world"
        ed.word_end();
        assert_eq!((ed.row, ed.col), (1, 7)); // end of "foo_bar"
        ed.word_end();
        assert_eq!((ed.row, ed.col), (1, 11)); // end of "baz"
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[test]
    fn word_start_jumps_to_word_starts() {
        let p = tmpfile("words2.txt", b"hello world\nfoo_bar baz\n");
        let mut ed = Editor::open(&p).unwrap();
        ed.row = 1;
        ed.col = 11; // end of buffer
        ed.word_start();
        assert_eq!((ed.row, ed.col), (1, 8)); // start of "baz"
        ed.word_start();
        assert_eq!((ed.row, ed.col), (1, 0)); // start of "foo_bar"
        ed.word_start();
        assert_eq!((ed.row, ed.col), (0, 6)); // start of "world"
        ed.word_start();
        assert_eq!((ed.row, ed.col), (0, 0)); // start of "hello"
        ed.word_start();
        assert_eq!((ed.row, ed.col), (0, 0)); // already at buffer start
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
