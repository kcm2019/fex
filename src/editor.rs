//! Built-in text editor with syntax highlighting.
//!
//! Opened with `e` on a text file. Ctrl+S saves, Esc closes
//! (asking about unsaved changes).

use crate::theme::Theme;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    style::{Modifier, Style},
    text::Span,
};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

/// Find the next `<style ...>` or `</style ...>` tag at or after byte
/// offset `from` (ASCII case-insensitive). Returns (tag_start, tag_end,
/// is_open). `<stylesheet>` and friends don't match: the character after
/// "style" must end the tag name.
fn find_style_tag(line: &str, from: usize) -> Option<(usize, usize, bool)> {
    let b = line.as_bytes();
    let mut i = from.min(b.len());
    while i < b.len() {
        if b[i] == b'<' {
            let mut j = i + 1;
            let is_close = if j < b.len() && b[j] == b'/' {
                j += 1;
                true
            } else {
                false
            };
            if b.len() >= j + 5 && b[j..j + 5].eq_ignore_ascii_case(b"style") {
                let k = j + 5;
                let name_ok = match b.get(k) {
                    Some(c) => c.is_ascii_whitespace() || *c == b'/' || *c == b'>',
                    None => false,
                };
                if name_ok {
                    if let Some(rel) = b[k..].iter().position(|&c| c == b'>') {
                        return Some((i, k + rel + 1, !is_close));
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Highlight one HTML line, running the CSS highlighter inside
/// `<style>...</style>` regions (case-insensitive). Returns the segments
/// plus (in_block_comment, in_style_block) carried into the next line.
///
/// A `<style` candidate that turns out to sit inside an HTML comment is
/// left alone: the tag only toggles the style state when the text before
/// it on this line ends outside a block comment. Multi-line CSS comments
/// inside a style block are not carried across lines.
fn scan_html_line(line: &str, in_block: bool, in_style: bool) -> (Vec<(Tok, String)>, bool, bool) {
    let mut segs = Vec::new();
    let mut ib = in_block;
    let mut is = in_style;
    let mut pos = 0;
    while let Some((ts, te, open)) = find_style_tag(line, pos) {
        let lang = if is { Lang::Css } else { Lang::Html };
        // CSS regions scan standalone: an HTML block comment can't be
        // open inside a style block (we only enter one outside comments).
        let (mut s, nib) = scan_line(&line[pos..ts], lang, ib && lang == Lang::Html);
        segs.append(&mut s);
        if lang == Lang::Html {
            ib = nib;
        }
        if ib {
            // Inside an HTML comment: the "tag" is comment text, no toggle.
            let (mut s2, nib2) = scan_line(&line[ts..te], Lang::Html, ib);
            segs.append(&mut s2);
            ib = nib2;
        } else {
            let (mut s2, nib2) = scan_line(&line[ts..te], Lang::Html, ib);
            segs.append(&mut s2);
            ib = nib2;
            is = open;
        }
        pos = te;
    }
    let lang = if is { Lang::Css } else { Lang::Html };
    let (mut s, nib) = scan_line(&line[pos..], lang, ib && lang == Lang::Html);
    segs.append(&mut s);
    if lang == Lang::Html {
        ib = nib;
    }
    (segs, ib, is)
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
    // Track the attribute name so `style="..."` values highlight as CSS.
    // `word` is the alphanumeric run in progress; `attr` is the name of
    // the attribute whose value may follow (set by `name =`).
    let mut word = String::new();
    let mut attr: Option<String> = None;
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
            let is_style = attr
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case("style"));
            attr = None;
            word.clear();
            if is_style && s.len() >= 2 {
                // Quotes stay strings; the declaration block scans as CSS.
                buf.feed(Tok::Str, &s[..1]);
                for (k, t) in scan_line(&s[1..s.len() - 1], Lang::Css, false).0 {
                    buf.feed(k, &t);
                }
                buf.feed(Tok::Str, &s[s.len() - 1..]);
            } else {
                buf.feed(Tok::Str, &s);
            }
        } else if c.is_ascii_alphanumeric() || c == '-' || c == ':' || c == '_' {
            word.push(c);
            buf.feed_ch(Tok::Normal, c);
        } else {
            if c == '=' {
                if !word.is_empty() {
                    attr = Some(std::mem::take(&mut word));
                }
                // `=` after whitespace (`style = "..."`): attr already set.
            } else if !c.is_whitespace() {
                word.clear();
                attr = None;
            }
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
/// Window in which consecutive character inserts share one undo entry: a
/// paste arriving as individual key events (no bracketed paste, e.g. under
/// tmux) and fast typing undo as a unit instead of character by character.
const INSERT_GROUP: Duration = Duration::from_secs(1);

/// Find-bar state for Ctrl+F in the editor: a small query field rendered in
/// the message line. Typing re-searches live from the text cursor.
/// Ctrl+R opens the same bar in replace mode, adding a second field.
#[derive(Debug, Clone)]
pub struct FindBar {
    /// The search query (chars), edited while the bar is open.
    pub query: String,
    /// Char index of the query cursor within `query`.
    pub cursor: usize,
    /// The last search found nothing (the bar shows "not found").
    pub not_found: bool,
    /// Replace mode (Ctrl+R): the bar also edits a replacement string.
    pub replace_mode: bool,
    /// The replacement text (chars), edited while the replace bar is open.
    pub replace: String,
    /// Char index of the replace cursor within `replace`.
    pub rcursor: usize,
    /// Which field the bar is editing: false = Find, true = Replace.
    pub replace_active: bool,
    /// Result count of the last replace-all, shown until the next edit.
    pub replace_done: Option<usize>,
}

impl FindBar {
    /// The field the bar is currently editing (query or replace), with its
    /// char cursor. The replace field is only active in replace mode.
    fn active_field(&mut self) -> (&mut String, &mut usize) {
        if self.replace_mode && self.replace_active {
            (&mut self.replace, &mut self.rcursor)
        } else {
            (&mut self.query, &mut self.cursor)
        }
    }

    /// True when the active field is the query (so edits re-search).
    fn active_is_query(&self) -> bool {
        !(self.replace_mode && self.replace_active)
    }

    /// Char offset of the cursor glyph from the start of the replace-mode
    /// bar text (see `Editor::find_bar_text`): the renderer puts the
    /// terminal cursor on the glyph so it picks up the cursor style.
    pub fn replace_cursor_offset(&self) -> usize {
        let base = "Find: ".chars().count();
        if self.replace_active {
            base + self.query.chars().count()
                + " → Replace: ".chars().count()
                + self.rcursor.min(self.replace.chars().count())
        } else {
            base + self.cursor.min(self.query.chars().count())
        }
    }
}

pub struct Editor {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub row: usize, // line index
    pub col: usize, // char index within the line
    /// First visible row, in *visual* rows (buffer lines split by word wrap).
    pub offset: usize,
    pub view_h: usize,
    /// Wrap width in characters for the text area (inside border + gutter),
    /// set by the renderer each frame (like `view_h`). `usize::MAX` before
    /// the first render means "no wrapping".
    pub wrap_w: usize,
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
    /// Find bar opened with Ctrl+F (`None` when closed).
    pub find: Option<FindBar>,
    /// Current find match as (row, start char col, end char col), painted
    /// while the find bar is open. Cleared when the bar closes.
    pub find_match: Option<(usize, usize, usize)>,
    /// Session-restore backup holding unsaved buffer content
    /// (`~/.config/fex/session/backup-N.txt`). `None` when the on-disk file
    /// is the source of truth. Deleted on save and when the tab closes.
    pub backup: Option<PathBuf>,
    lang: Lang,
    trailing_newline: bool,
    block_end: Vec<bool>, // in-block-comment at end of line i
    style_end: Vec<bool>, // inside <style>..</style> at end of line i (HTML)
    hl_valid: usize,      // rows [0, hl_valid) have valid block_end entries
    undo: Vec<UndoSnap>,
    redo: Vec<UndoSnap>,
    /// When the last character was inserted; rapid consecutive inserts
    /// share one undo entry (see INSERT_GROUP).
    last_insert: Option<Instant>,
}

/// One rendered visual row of the editor: the buffer line it comes from,
/// whether it is that line's first segment (only the first gets the line
/// number in the gutter), the segment's char range, and the highlighted
/// spans sliced to the segment.
pub struct WrappedRow {
    pub buf_row: usize,
    pub first: bool,
    pub seg: (usize, usize),
    pub spans: Vec<Span<'static>>,
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
            wrap_w: usize::MAX,
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
            find: None,
            find_match: None,
            backup: None,
            block_end: Vec::new(),
            style_end: Vec::new(),
            hl_valid: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_insert: None,
        })
    }

    pub fn save(&mut self) -> io::Result<()> {
        let mut content = self.lines.join("\n");
        if self.trailing_newline {
            content.push('\n');
        }
        fs::write(&self.path, content)?;
        self.dirty = false;
        self.clear_backup();
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
            wrap_w: usize::MAX,
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
            find: None,
            find_match: None,
            backup: None,
            block_end: Vec::new(),
            style_end: Vec::new(),
            hl_valid: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_insert: None,
        }
    }

    /// Delete the session backup file (if any) and forget it. The buffer is
    /// safe on disk now, so the backup must not resurrect stale content.
    pub fn clear_backup(&mut self) {
        if let Some(path) = self.backup.take() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// The exact file content this buffer would save, for session backups.
    pub fn backup_text(&self) -> String {
        let mut content = self.lines.join("\n");
        if self.trailing_newline {
            content.push('\n');
        }
        content
    }

    /// Rebuild an editor from session-backup text (unsaved work from a
    /// previous run). The buffer comes back dirty, backed by `backup`.
    pub fn from_backup(path: PathBuf, text: &str, backup: PathBuf) -> Editor {
        let mut ed = Editor::untitled();
        ed.path = path;
        let mut text = text.to_string();
        let mut trailing = false;
        if text.ends_with('\n') {
            trailing = true;
            text.pop();
        }
        ed.lines = text.split('\n').map(|s| s.to_string()).collect();
        if ed.lines.is_empty() {
            ed.lines.push(String::new());
        }
        ed.trailing_newline = trailing;
        ed.dirty = true;
        ed.backup = Some(backup);
        ed
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
        // A new mutation breaks any insert burst: the next character starts
        // a fresh undo group.
        self.last_insert = None;
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
        self.last_insert = None;
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
        // Characters arriving in quick succession (a paste delivered as
        // individual key events, or fast typing) share one undo entry, so
        // one Ctrl+Z removes the whole burst instead of one character.
        let grouped = matches!(self.last_insert, Some(t) if t.elapsed() < INSERT_GROUP);
        if !grouped {
            self.push_undo();
        }
        self.remove_selection();
        let b = char_idx_to_byte(&self.lines[self.row], self.col);
        self.lines[self.row].insert(b, ch);
        self.col += 1;
        self.dirty = true;
        self.invalidate_hl(self.row);
        self.last_insert = Some(Instant::now());
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

    // -- line operations --------------------------------------------------
    // Duplicate line (Ctrl+D), move line (Alt+↑/↓), indent/dedent (Tab with
    // a selection), word delete (Ctrl+Backspace / Ctrl+Delete). Each is one
    // undo entry and marks the buffer dirty.

    /// Buffer rows touched by the text selection, inclusive. A selection
    /// ending at column 0 of a later row doesn't touch that row.
    fn selected_line_range(&self) -> Option<(usize, usize)> {
        let ((sr, _), (er, ec)) = self.selection()?;
        let er = if ec == 0 && er > sr { er - 1 } else { er };
        Some((sr, er))
    }

    /// Duplicate the current line directly below it (cursor follows the
    /// duplicate); with a selection, duplicate the whole touched-line block.
    pub fn duplicate_line(&mut self) {
        let (sr, er) = match self.selected_line_range() {
            Some(r) => r,
            None => (self.row, self.row),
        };
        self.push_undo();
        let block: Vec<String> = self.lines[sr..=er].to_vec();
        let n = block.len();
        for (i, line) in block.into_iter().enumerate() {
            self.lines.insert(er + 1 + i, line);
        }
        self.row += n;
        if let Some((ar, ac)) = self.sel_anchor {
            self.sel_anchor = Some((ar + n, ac));
        }
        self.dirty = true;
        self.invalidate_hl(sr);
        self.ensure_visible();
    }

    /// Move the current line (or the selected line block) up (`delta` < 0)
    /// or down, swapping with the neighbor; the cursor follows the moved
    /// line(s). No-op at the top/bottom of the buffer.
    pub fn move_line(&mut self, delta: i32) {
        let (sr, er) = match self.selected_line_range() {
            Some(r) => r,
            None => (self.row, self.row),
        };
        if delta < 0 {
            if sr == 0 {
                return;
            }
            self.push_undo();
            let above = self.lines.remove(sr - 1);
            self.lines.insert(er, above);
            self.row -= 1;
            if let Some((ar, ac)) = self.sel_anchor {
                self.sel_anchor = Some((ar - 1, ac));
            }
            self.invalidate_hl(sr - 1);
        } else if delta > 0 {
            if er + 1 >= self.lines.len() {
                return;
            }
            self.push_undo();
            let below = self.lines.remove(er + 1);
            self.lines.insert(sr, below);
            self.row += 1;
            if let Some((ar, ac)) = self.sel_anchor {
                self.sel_anchor = Some((ar + 1, ac));
            }
            self.invalidate_hl(sr);
        } else {
            return;
        }
        self.clamp_col();
        self.dirty = true;
        self.ensure_visible();
    }

    /// Indent every line touched by the selection by 4 spaces (Tab).
    pub fn indent_selection(&mut self) {
        let Some((sr, er)) = self.selected_line_range() else {
            return;
        };
        self.push_undo();
        self.apply_indent(sr, er);
        if let Some((ar, ac)) = self.sel_anchor {
            self.sel_anchor = Some((ar, ac + 4));
        }
        self.col += 4;
        self.dirty = true;
        self.invalidate_hl(sr);
    }

    /// Remove up to 4 leading spaces from every touched line (Shift+Tab).
    pub fn dedent_selection(&mut self) {
        let Some((sr, er)) = self.selected_line_range() else {
            return;
        };
        self.push_undo();
        let removed = self.apply_dedent(sr, er);
        let unindent = |row: usize, col: usize| col.saturating_sub(removed[row - sr]);
        if let Some((ar, ac)) = self.sel_anchor {
            self.sel_anchor = Some((ar, unindent(ar, ac)));
        }
        self.col = unindent(self.row, self.col);
        self.dirty = true;
        self.invalidate_hl(sr);
    }

    /// Dedent the current line (Shift+Tab with no selection).
    pub fn dedent_line(&mut self) {
        let row = self.row;
        self.push_undo();
        let removed = self.apply_dedent(row, row);
        self.col = self.col.saturating_sub(removed[0]);
        self.dirty = true;
        self.invalidate_hl(row);
    }

    /// True when a non-empty text selection is active.
    pub fn has_selection(&self) -> bool {
        self.selection().is_some_and(|(a, b)| a != b)
    }

    /// Insert 4 spaces at the start of lines [sr..=er].
    fn apply_indent(&mut self, sr: usize, er: usize) {
        for r in sr..=er {
            self.lines[r].insert_str(0, "    ");
        }
    }

    /// Remove up to 4 leading spaces from lines [sr..=er]; returns the
    /// per-line counts removed (for cursor adjustment).
    fn apply_dedent(&mut self, sr: usize, er: usize) -> Vec<usize> {
        let mut removed = Vec::with_capacity(er - sr + 1);
        for r in sr..=er {
            let n = self.lines[r]
                .chars()
                .take_while(|c| *c == ' ')
                .take(4)
                .count();
            let b = char_idx_to_byte(&self.lines[r], n);
            self.lines[r].replace_range(..b, "");
            removed.push(n);
        }
        removed
    }

    /// Delete the char range [start, end), joining lines as needed.
    fn delete_range(&mut self, start: (usize, usize), end: (usize, usize)) {
        debug_assert!(start <= end);
        if start == end {
            return;
        }
        let (sr, sc) = start;
        let (er, ec) = end;
        self.push_undo();
        if sr == er {
            let b0 = char_idx_to_byte(&self.lines[sr], sc);
            let b1 = char_idx_to_byte(&self.lines[sr], ec);
            self.lines[sr].replace_range(b0..b1, "");
        } else {
            let b0 = char_idx_to_byte(&self.lines[sr], sc);
            let tail = self.lines[er][char_idx_to_byte(&self.lines[er], ec)..].to_string();
            self.lines[sr].truncate(b0);
            self.lines[sr].push_str(&tail);
            self.lines.drain(sr + 1..=er);
        }
        self.row = sr;
        self.col = sc;
        self.dirty = true;
        self.invalidate_hl(sr);
        self.ensure_visible();
    }

    /// Delete from the cursor back to the previous word start
    /// (Ctrl+Backspace); at a line start it joins with the previous line
    /// like Backspace. A selection deletes as usual.
    pub fn delete_word_back(&mut self) {
        if self.sel_anchor.is_some() {
            self.delete_selection();
            return;
        }
        if self.col == 0 {
            self.backspace();
            return;
        }
        let orig = (self.row, self.col);
        self.word_start();
        let target = (self.row, self.col);
        self.row = orig.0;
        self.col = orig.1;
        self.delete_range(target, orig);
    }

    /// Delete from the cursor forward to the next word end (Ctrl+Delete),
    /// crossing lines; a selection deletes as usual.
    pub fn delete_word_forward(&mut self) {
        if self.sel_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let orig = (self.row, self.col);
        self.word_end();
        let target = (self.row, self.col);
        self.row = orig.0;
        self.col = orig.1;
        self.delete_range(orig, target);
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

    /// Prepare for a cursor move with or without Shift held: with Shift,
    /// anchor the selection at the current cursor the first time (so the
    /// move extends it); without Shift, collapse any selection.
    pub fn shift_select(&mut self, shift: bool) {
        if shift {
            if self.sel_anchor.is_none() {
                self.sel_anchor = Some((self.row, self.col));
            }
        } else {
            self.sel_anchor = None;
        }
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

    /// Move the cursor to a terminal click at (x, y). The y coordinate maps
    /// to a visual (wrapped) row, the x coordinate to a column within that
    /// row's segment.
    pub fn click_at(&mut self, x: u16, y: u16) {
        let tx = self.view_x + 1 + self.gutter_w();
        let ty = self.view_y + 1;
        if x < tx || y < ty {
            return;
        }
        let vrow = self.offset + (y - ty) as usize;
        let vcol = (x - tx) as usize;
        self.set_cursor_visual(vrow, vcol);
    }

    /// Scroll the view by `delta` visual rows (mouse wheel).
    pub fn scroll_by(&mut self, delta: isize) {
        let max_off = self.visual_len().saturating_sub(1);
        let new = (self.offset as isize + delta).clamp(0, max_off as isize) as usize;
        self.offset = new;
        // Keep the cursor on screen, preserving its visual column.
        let (vrow, vcol) = self.cursor_visual();
        if vrow < self.offset || vrow >= self.offset + self.view_h.max(1) {
            self.set_cursor_visual(self.offset, vcol);
        }
    }

    // -- cursor movement ----------------------------------------------------

    /// Move the cursor to the given 1-based line (clamped), first column,
    /// scrolled into view.
    pub fn goto_line(&mut self, n: usize) {
        let max = self.lines.len().max(1);
        self.row = n.saturating_sub(1).min(max - 1);
        self.col = 0;
        self.ensure_visible();
    }

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
        // Scrolling is in visual (wrapped) rows.
        let (vrow, _) = self.cursor_visual();
        if vrow < self.offset {
            self.offset = vrow;
        } else if vrow >= self.offset + self.view_h {
            self.offset = vrow - self.view_h + 1;
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

    // -- find -----------------------------------------------------------------
    // Ctrl+F opens a small query field in the message line. Search is a
    // case-insensitive substring match, char-based like the text cursor.

    /// Open the find bar (empty query, no jump yet).
    pub fn open_find(&mut self) {
        if self.find.is_none() {
            self.find = Some(FindBar {
                query: String::new(),
                cursor: 0,
                not_found: false,
                replace_mode: false,
                replace: String::new(),
                rcursor: 0,
                replace_active: false,
                replace_done: None,
            });
            self.find_match = None;
        }
    }

    /// Open the replace bar (Ctrl+R). When a Ctrl+F bar is already open its
    /// query is kept; otherwise the bar starts empty like `open_find`.
    pub fn open_replace(&mut self) {
        if self.find.is_none() {
            self.open_find();
        }
        if let Some(find) = self.find.as_mut() {
            find.replace_mode = true;
        }
    }

    /// Close the find bar, leaving the text cursor at the last match.
    pub fn close_find(&mut self) {
        self.find = None;
        self.find_match = None;
    }

    /// All match starts as (row, char col), in buffer order.
    pub fn find_matches(&self) -> Vec<(usize, usize)> {
        let Some(find) = &self.find else {
            return Vec::new();
        };
        let q: Vec<char> = find.query.to_lowercase().chars().collect();
        if q.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (row, line) in self.lines.iter().enumerate() {
            let lc: Vec<char> = line.to_lowercase().chars().collect();
            if lc.len() < q.len() {
                continue;
            }
            for i in 0..=lc.len() - q.len() {
                if lc[i..i + q.len()] == q[..] {
                    out.push((row, i));
                }
            }
        }
        out
    }

    /// Re-search from the text cursor after the query changed: jump to the
    /// first match at/after the cursor, wrapping around the buffer. An
    /// empty query does nothing; no matches sets `not_found`.
    fn find_research(&mut self) {
        let matches = self.find_matches();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if find.query.is_empty() {
            find.not_found = false;
            self.find_match = None;
            return;
        }
        if matches.is_empty() {
            find.not_found = true;
            self.find_match = None;
            return;
        }
        find.not_found = false;
        // First match at/after the cursor, else wrap to the first.
        let mut target = matches[0];
        for &m in &matches {
            if m >= (self.row, self.col) {
                target = m;
                break;
            }
        }
        self.row = target.0;
        self.col = target.1;
        self.find_match = Some((target.0, target.1, target.1 + find.query.chars().count()));
        self.sel_anchor = None;
        self.ensure_visible();
    }

    /// Replace the highlighted match with the replace text (Ctrl+R, Enter).
    /// The text cursor lands after the inserted text, so the next Enter
    /// replaces the following match instead of re-matching the
    /// replacement. An empty query is a no-op. One undo entry per call.
    pub fn replace_current(&mut self) {
        let replacement: String;
        {
            let Some(find) = &self.find else {
                return;
            };
            if find.query.is_empty() {
                return;
            }
            replacement = find.replace.clone();
        }
        let Some((row, s, e)) = self.find_match else {
            if let Some(find) = self.find.as_mut() {
                find.not_found = true;
            }
            return;
        };
        self.push_undo();
        {
            let line = &mut self.lines[row];
            let mut lc: Vec<char> = line.chars().collect();
            lc.splice(s..e, replacement.chars());
            *line = lc.into_iter().collect();
            self.row = row;
            self.col = s + replacement.chars().count();
        }
        self.dirty = true;
        self.invalidate_hl(row);
        self.find_research();
    }

    /// Replace every non-overlapping case-insensitive occurrence of the
    /// query with the replace text across all lines. Returns the number of
    /// replacements; the whole batch is a single undo entry. An empty
    /// query is a no-op returning 0.
    pub fn replace_all(&mut self) -> usize {
        let (q, replacement) = {
            let Some(find) = &self.find else {
                return 0;
            };
            if find.query.is_empty() {
                return 0;
            }
            (find.query.to_lowercase(), find.replace.clone())
        };
        let qchars: Vec<char> = q.chars().collect();
        let rchars: Vec<char> = replacement.chars().collect();
        // Build the new lines first so a no-match run pushes no undo entry.
        let mut count = 0;
        let mut new_lines: Vec<String> = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            let lc: Vec<char> = line.to_lowercase().chars().collect();
            let mut starts: Vec<usize> = Vec::new();
            let mut i = 0;
            while i + qchars.len() <= lc.len() {
                if lc[i..i + qchars.len()] == qchars[..] {
                    starts.push(i);
                    i += qchars.len(); // non-overlapping
                } else {
                    i += 1;
                }
            }
            if starts.is_empty() {
                new_lines.push(line.clone());
                continue;
            }
            let orig: Vec<char> = line.chars().collect();
            let mut out: Vec<char> = Vec::with_capacity(orig.len());
            let mut prev = 0;
            for &m in &starts {
                out.extend_from_slice(&orig[prev..m]);
                out.extend_from_slice(&rchars);
                prev = m + qchars.len();
            }
            out.extend_from_slice(&orig[prev..]);
            new_lines.push(out.into_iter().collect());
            count += starts.len();
        }
        if count > 0 {
            self.push_undo();
            self.lines = new_lines;
            self.dirty = true;
            self.invalidate_hl(0);
        }
        if let Some(find) = self.find.as_mut() {
            find.replace_done = Some(count);
        }
        self.find_research();
        count
    }

    /// Jump to the next (`dir` > 0) or previous (`dir` < 0) match, wrapping
    /// around the buffer. The cursor lands on the match start, any mouse
    /// selection is cleared, and the view scrolls so it stays visible.
    pub fn find_jump(&mut self, dir: i8) {
        let matches = self.find_matches();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if matches.is_empty() {
            find.not_found = true;
            self.find_match = None;
            return;
        }
        find.not_found = false;
        let target = if dir < 0 {
            // Last match strictly before the cursor, else wrap to the last.
            let mut target = matches[matches.len() - 1];
            for &m in matches.iter().rev() {
                if m < (self.row, self.col) {
                    target = m;
                    break;
                }
            }
            target
        } else {
            // First match strictly after the cursor, else wrap to the first.
            let mut target = matches[0];
            for &m in &matches {
                if m > (self.row, self.col) {
                    target = m;
                    break;
                }
            }
            target
        };
        self.row = target.0;
        self.col = target.1;
        self.find_match = Some((target.0, target.1, target.1 + find.query.chars().count()));
        self.sel_anchor = None;
        self.ensure_visible();
    }

    /// Edit the find bar with one key while it is open. Returns true when the
    /// key was consumed (the bar eats every key so none reach the document).
    ///
    /// Editing the query re-searches from the text cursor; editing the
    /// replace field does not. In replace mode (Ctrl+R) Tab switches the
    /// Find/Replace fields and Ctrl+A replaces every match (in plain find
    /// mode Ctrl+A is eaten like any other Ctrl key — select-all only
    /// applies with the bar closed).
    pub fn find_input(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let mut query_changed = false;
        let mut do_replace_all = false;
        {
            let Some(find) = self.find.as_mut() else {
                return false;
            };
            match key.code {
                // Tab switches the Find/Replace fields (replace mode only;
                // in plain find mode Tab is eaten like today).
                KeyCode::Tab if find.replace_mode => {
                    find.replace_active = !find.replace_active;
                }
                // Ctrl+A replaces all matches (replace mode only).
                KeyCode::Char('a') | KeyCode::Char('A') if ctrl && find.replace_mode => {
                    do_replace_all = true;
                }
                KeyCode::Char(c) if !ctrl => {
                    let is_query = find.active_is_query();
                    {
                        let (field, fcursor) = find.active_field();
                        let len = field.chars().count();
                        let at = (*fcursor).min(len);
                        let mut chars: Vec<char> = field.chars().collect();
                        chars.insert(at, c);
                        *field = chars.into_iter().collect();
                        *fcursor = at + 1;
                    }
                    if is_query {
                        query_changed = true;
                    }
                    find.replace_done = None;
                }
                KeyCode::Backspace => {
                    let is_query = find.active_is_query();
                    let mut edited = false;
                    {
                        let (field, fcursor) = find.active_field();
                        if *fcursor > 0 {
                            let mut chars: Vec<char> = field.chars().collect();
                            chars.remove(*fcursor - 1);
                            *field = chars.into_iter().collect();
                            *fcursor -= 1;
                            edited = true;
                        }
                    }
                    if edited {
                        if is_query {
                            query_changed = true;
                        }
                        find.replace_done = None;
                    }
                }
                KeyCode::Delete => {
                    let is_query = find.active_is_query();
                    let mut edited = false;
                    {
                        let (field, fcursor) = find.active_field();
                        let len = field.chars().count();
                        if *fcursor < len {
                            let mut chars: Vec<char> = field.chars().collect();
                            chars.remove(*fcursor);
                            *field = chars.into_iter().collect();
                            edited = true;
                        }
                    }
                    if edited {
                        if is_query {
                            query_changed = true;
                        }
                        find.replace_done = None;
                    }
                }
                KeyCode::Left => {
                    let is_query = find.active_is_query();
                    {
                        let (_, fcursor) = find.active_field();
                        if *fcursor > 0 {
                            *fcursor -= 1;
                        }
                    }
                    if is_query {
                        query_changed = true;
                    }
                }
                KeyCode::Right => {
                    let is_query = find.active_is_query();
                    {
                        let (field, fcursor) = find.active_field();
                        let len = field.chars().count();
                        if *fcursor < len {
                            *fcursor += 1;
                        }
                    }
                    if is_query {
                        query_changed = true;
                    }
                }
                KeyCode::Home => {
                    let is_query = find.active_is_query();
                    {
                        let (_, fcursor) = find.active_field();
                        *fcursor = 0;
                    }
                    if is_query {
                        query_changed = true;
                    }
                }
                KeyCode::End => {
                    let is_query = find.active_is_query();
                    {
                        let (field, fcursor) = find.active_field();
                        *fcursor = field.chars().count();
                    }
                    if is_query {
                        query_changed = true;
                    }
                }
                _ => {}
            }
        }
        if do_replace_all {
            self.replace_all();
        } else if query_changed {
            self.find_research();
        }
        true
    }

    /// Text for the find bar. Plain find mode is unchanged: `Find: <query>
    /// [i/N]`, the not-found form, or just `Find: ` for an empty query.
    /// Replace mode (Ctrl+R) shows both fields — `Find: <q> → Replace: <r>
    /// [i/N]` — with a `█` block at the active field's cursor, the same
    /// `[i/N]` / `— not found` forms, and `— replaced {n}` after a
    /// replace-all.
    pub fn find_bar_text(&self) -> String {
        let Some(find) = &self.find else {
            return String::new();
        };
        if !find.replace_mode {
            if find.query.is_empty() {
                return "Find: ".to_string();
            }
            let matches = self.find_matches();
            if matches.is_empty() {
                return format!("Find: {} — not found", find.query);
            }
            return match matches.iter().position(|&m| m == (self.row, self.col)) {
                Some(i) => format!("Find: {} [{}/{}]", find.query, i + 1, matches.len()),
                None => format!("Find: {} [{} matches]", find.query, matches.len()),
            };
        }
        let q = Self::bar_field(&find.query, find.cursor, !find.replace_active);
        let r = Self::bar_field(&find.replace, find.rcursor, find.replace_active);
        let mut s = format!("Find: {q} → Replace: {r}");
        if !find.query.is_empty() {
            let matches = self.find_matches();
            if matches.is_empty() {
                s.push_str(" — not found");
            } else {
                match matches.iter().position(|&m| m == (self.row, self.col)) {
                    Some(i) => s.push_str(&format!(" [{}/{}]", i + 1, matches.len())),
                    None => s.push_str(&format!(" [{} matches]", matches.len())),
                }
            }
        }
        if let Some(n) = find.replace_done {
            s.push_str(&format!(" — replaced {n}"));
        }
        s
    }

    /// One find-bar field for `find_bar_text`: inserts a `█` block at the
    /// char cursor when it is the field being edited.
    fn bar_field(field: &str, cursor: usize, active: bool) -> String {
        if !active {
            return field.to_string();
        }
        let mut chars: Vec<char> = field.chars().collect();
        let at = cursor.min(chars.len());
        chars.insert(at, '█');
        chars.into_iter().collect()
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

    /// Up/Down move by visual (wrapped) row, keeping the visual column, so
    /// the cursor walks through a wrapped line's segments instead of
    /// jumping over them.
    pub fn move_up(&mut self) {
        let (vrow, vcol) = self.cursor_visual();
        if vrow > 0 {
            self.set_cursor_visual(vrow - 1, vcol);
        }
    }

    pub fn move_down(&mut self) {
        let (vrow, vcol) = self.cursor_visual();
        if vrow + 1 < self.visual_len() {
            self.set_cursor_visual(vrow + 1, vcol);
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
        let (vrow, vcol) = self.cursor_visual();
        self.offset = self.offset.saturating_sub(h);
        self.set_cursor_visual(vrow.saturating_sub(h), vcol);
    }

    pub fn page_down(&mut self) {
        let h = self.view_h.max(1);
        let (vrow, vcol) = self.cursor_visual();
        self.set_cursor_visual(vrow + h, vcol);
    }

    // -- word wrap ----------------------------------------------------------

    /// Split a buffer line into visual segments of at most `width` characters,
    /// breaking at the last space when possible and hard-breaking words that
    /// are longer than the width. Returns char-index ranges; an empty line
    /// yields one empty segment so it still takes a visual row.
    fn wrap_segments(line: &str, width: usize) -> Vec<(usize, usize)> {
        let width = width.max(1);
        let chars: Vec<char> = line.chars().collect();
        let mut segs = Vec::new();
        let mut start = 0;
        while start < chars.len() {
            let mut end = (start + width).min(chars.len());
            if end < chars.len() {
                // Prefer to break after the last space in the segment so the
                // next one starts cleanly; hard-break when there is no space.
                if let Some(sp) = chars[start..end].iter().rposition(|&c| c == ' ') {
                    end = start + sp + 1;
                }
            }
            segs.push((start, end));
            start = end;
        }
        if segs.is_empty() {
            segs.push((0, 0));
        }
        segs
    }

    /// Total visual (wrapped) rows in the buffer.
    fn visual_len(&self) -> usize {
        self.lines
            .iter()
            .map(|l| Self::wrap_segments(l, self.wrap_w).len())
            .sum()
    }

    /// Visual (row, column) of the buffer cursor.
    pub fn cursor_visual(&self) -> (usize, usize) {
        let mut vrow = 0;
        for line in self.lines.iter().take(self.row) {
            vrow += Self::wrap_segments(line, self.wrap_w).len();
        }
        let segs = Self::wrap_segments(&self.lines[self.row], self.wrap_w);
        for (k, &(s, e)) in segs.iter().enumerate() {
            // A cursor sitting exactly on a segment boundary belongs to the
            // next segment (except past the last one, which can't happen).
            if self.col < e || k + 1 == segs.len() {
                return (vrow + k, self.col - s);
            }
        }
        (vrow, self.col) // unreachable; keeps the compiler happy
    }

    /// Move the buffer cursor to visual position (`vrow`, `vcol`), clamping
    /// past the end of the buffer, and scroll it into view.
    fn set_cursor_visual(&mut self, vrow: usize, vcol: usize) {
        let mut v = 0;
        for (i, line) in self.lines.iter().enumerate() {
            let segs = Self::wrap_segments(line, self.wrap_w);
            if vrow < v + segs.len() {
                let (s, e) = segs[vrow - v];
                self.row = i;
                self.col = (s + vcol).min(e);
                self.ensure_visible();
                return;
            }
            v += segs.len();
        }
        self.row = self.lines.len().saturating_sub(1);
        self.col = self.lines[self.row].chars().count();
        self.ensure_visible();
    }

    /// Slice a line's styled spans down to the char range [start, end).
    fn slice_spans(spans: &[Span<'static>], start: usize, end: usize) -> Vec<Span<'static>> {
        let mut out = Vec::new();
        let mut pos = 0usize;
        for sp in spans {
            let n = sp.content.chars().count();
            let (s0, s1) = (pos, pos + n);
            pos = s1;
            if s1 <= start || s0 >= end {
                continue;
            }
            let chars: Vec<char> = sp.content.chars().collect();
            let a = start.saturating_sub(s0);
            let b = (end - s0).min(n);
            if a < b {
                out.push(Span::styled(
                    chars[a..b].iter().collect::<String>(),
                    sp.style,
                ));
            }
        }
        out
    }

    // -- highlighting -------------------------------------------------------

    fn ensure_hl(&mut self, upto: usize) {
        let upto = upto.min(self.lines.len());
        if self.block_end.len() < upto {
            self.block_end.resize(upto, false);
            self.style_end.resize(upto, false);
        }
        let mut state = if self.hl_valid == 0 {
            false
        } else {
            self.block_end[self.hl_valid - 1]
        };
        let mut sstate = if self.hl_valid == 0 {
            false
        } else {
            self.style_end[self.hl_valid - 1]
        };
        let mut i = self.hl_valid;
        while i < upto {
            if self.lang == Lang::Html {
                let (_, b, s) = scan_html_line(&self.lines[i], state, sstate);
                state = b;
                sstate = s;
                self.block_end[i] = b;
                self.style_end[i] = s;
            } else {
                let (_, b) = scan_line(&self.lines[i], self.lang, state);
                state = b;
                self.block_end[i] = b;
                self.style_end[i] = false;
            }
            i += 1;
        }
        self.hl_valid = upto.max(self.hl_valid);
    }

    /// One rendered visual row: the buffer line it comes from, whether it is
    /// that line's first segment (gets the line number in the gutter), the
    /// segment's char range, and the highlighted spans sliced to the segment.
    pub fn highlight_visible_wrapped(
        &mut self,
        theme: &Theme,
        offset: usize,
        height: usize,
    ) -> Vec<WrappedRow> {
        let end_vis = offset + height;
        // Buffer lines spanned by the visual window.
        let mut v = 0usize;
        let mut first = 0usize;
        let mut last = 0usize; // exclusive
        let mut found = false;
        for (i, line) in self.lines.iter().enumerate() {
            let n = Self::wrap_segments(line, self.wrap_w).len();
            if !found && v + n > offset {
                first = i;
                found = true;
            }
            if v >= end_vis {
                break;
            }
            last = i + 1;
            v += n;
        }
        if !found {
            return Vec::new();
        }
        self.ensure_hl(last);
        // Visual row of `first`.
        let mut v = 0usize;
        for line in self.lines.iter().take(first) {
            v += Self::wrap_segments(line, self.wrap_w).len();
        }
        let mut out = Vec::new();
        for i in first..last {
            let start_state = if i == 0 { false } else { self.block_end[i - 1] };
            let segs = if self.lang == Lang::Html {
                let start_style = if i == 0 { false } else { self.style_end[i - 1] };
                scan_html_line(&self.lines[i], start_state, start_style).0
            } else {
                scan_line(&self.lines[i], self.lang, start_state).0
            };
            let spans: Vec<Span<'static>> = segs
                .into_iter()
                .map(|(k, s)| span_for(theme, k, s))
                .collect();
            for (k, &(s, e)) in Self::wrap_segments(&self.lines[i], self.wrap_w)
                .iter()
                .enumerate()
            {
                if v >= offset && v < end_vis {
                    out.push(WrappedRow {
                        buf_row: i,
                        first: k == 0,
                        seg: (s, e),
                        spans: Self::slice_spans(&spans, s, e),
                    });
                }
                v += 1;
            }
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
    fn highlight_html_style_block_as_css() {
        // One-line block: tag names keep HTML highlighting, the CSS
        // inside scans as CSS (@media is a CSS keyword).
        let (segs, _, in_style) = scan_html_line("<style>@media x {</style>", false, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "style"));
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "@media"));
        assert!(!in_style, "closed on the same line");
        // Multi-line: the style state carries across lines.
        let (_, _, in_style) = scan_html_line("<style>", false, false);
        assert!(in_style);
        let (segs, _, in_style) = scan_html_line("@media x {", false, in_style);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "@media"));
        assert!(in_style);
        let (_, _, in_style) = scan_html_line("}", false, in_style);
        assert!(in_style, "still inside until </style>");
        let (segs, _, in_style) = scan_html_line("</style>", false, in_style);
        assert!(!in_style);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "style"));
        // Case-insensitive, attributes allowed on the tag.
        let (_, _, in_style) = scan_html_line("<STYLE type=\"text/css\">", false, false);
        assert!(in_style);
        // Outside the block, HTML highlighting is unchanged.
        let (segs, _, _) = scan_html_line("<div>hi</div>", false, false);
        assert!(segs.iter().any(|(k, s)| *k == Tok::Keyword && s == "div"));
        assert!(!segs
            .iter()
            .any(|(k, s)| *k == Tok::Keyword && s == "@media"));
        // A <style> inside an HTML comment does not toggle the state.
        let (segs, in_block, in_style) = scan_html_line("<!-- <style>", false, false);
        assert!(in_block);
        assert!(!in_style);
        assert!(segs.iter().all(|(k, _)| *k == Tok::Comment));
        // `<stylesheet>` is not a style tag.
        let (_, _, in_style) = scan_html_line("<stylesheet>", false, false);
        assert!(!in_style);
    }

    #[test]
    fn highlight_html_inline_style_attr_as_css() {
        // The declaration scans as CSS: /* */ becomes a comment, which
        // plain HTML highlighting would leave as normal text.
        let (segs, _) = scan_line(r#"<p style="/* hi */ color: red">x</p>"#, Lang::Html, false);
        assert!(segs
            .iter()
            .any(|(k, s)| *k == Tok::Comment && s == "/* hi */"));
        // Other attributes are untouched.
        let (segs, _) = scan_line(
            r#"<p class="x" STYLE="color: red">y</p>"#,
            Lang::Html,
            false,
        );
        assert!(segs.iter().any(|(k, s)| *k == Tok::Str && s == "\"x\""));
        assert!(!segs.iter().any(|(k, _)| *k == Tok::Comment));
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

    #[test]
    fn shift_select_anchors_extends_and_collapses() {
        let p = tmpfile("shift.txt", b"hello world\n");
        let mut ed = Editor::open(&p).unwrap();
        // Shift+Right anchors at (0,0) and extends.
        ed.shift_select(true);
        ed.move_right();
        ed.move_right();
        ed.move_right();
        assert_eq!(ed.selection(), Some(((0, 0), (0, 3))));
        // Releasing Shift and moving collapses the selection.
        ed.shift_select(false);
        ed.move_right();
        assert_eq!(ed.sel_anchor, None);
        assert_eq!((ed.row, ed.col), (0, 4));
        // Shift+Left from the middle anchors there and extends left.
        ed.shift_select(true);
        ed.move_left();
        assert_eq!(ed.selection(), Some(((0, 3), (0, 4))));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn burst_inserts_share_one_undo_step() {
        let p = tmpfile("burst.txt", b"\n");
        let mut ed = Editor::open(&p).unwrap();
        // Characters arriving in quick succession (a paste without
        // bracketed-paste support) undo as one unit.
        for ch in "pasted".chars() {
            ed.insert_char(ch);
        }
        assert_eq!(ed.lines[0], "pasted");
        ed.undo();
        assert_eq!(ed.lines[0], "");
        assert_eq!(ed.message, "Undone");
        // Redo restores the whole burst too.
        ed.redo();
        assert_eq!(ed.lines[0], "pasted");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn slow_inserts_are_separate_undo_steps() {
        let p = tmpfile("slow.txt", b"\n");
        let mut ed = Editor::open(&p).unwrap();
        ed.insert_char('a');
        // Simulate a pause longer than INSERT_GROUP between keystrokes.
        ed.last_insert = Some(Instant::now() - INSERT_GROUP - Duration::from_secs(1));
        ed.insert_char('b');
        assert_eq!(ed.lines[0], "ab");
        ed.undo();
        assert_eq!(ed.lines[0], "a");
        ed.undo();
        assert_eq!(ed.lines[0], "");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn other_edits_break_the_insert_burst() {
        let p = tmpfile("brk.txt", b"\n");
        let mut ed = Editor::open(&p).unwrap();
        ed.insert_char('a');
        ed.insert_char('b');
        ed.newline(); // a different edit starts a new undo group
        ed.insert_char('c');
        ed.undo();
        assert_eq!(ed.lines, vec!["ab", ""]);
        ed.undo();
        assert_eq!(ed.lines, vec!["ab"]);
        ed.undo();
        assert_eq!(ed.lines, vec![""]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn wrap_segments_breaks_at_spaces() {
        // "aa " | "bb " | "cc"
        assert_eq!(
            Editor::wrap_segments("aa bb cc", 4),
            vec![(0, 3), (3, 6), (6, 8)]
        );
    }

    #[test]
    fn wrap_segments_hard_breaks_long_words() {
        assert_eq!(
            Editor::wrap_segments("abcdefgh", 3),
            vec![(0, 3), (3, 6), (6, 8)]
        );
    }

    #[test]
    fn wrap_segments_short_and_empty_lines() {
        assert_eq!(Editor::wrap_segments("", 10), vec![(0, 0)]);
        assert_eq!(Editor::wrap_segments("hi", 10), vec![(0, 2)]);
    }

    #[test]
    fn cursor_visual_maps_through_wrapped_segments() {
        let mut ed = Editor::untitled();
        ed.lines = vec!["aa bb cc".to_string(), "z".to_string()];
        ed.wrap_w = 4;
        // Visual rows: "aa "(0), "bb "(1), "cc"(2), "z"(3).
        ed.row = 0;
        ed.col = 4; // start of "bb"
        assert_eq!(ed.cursor_visual(), (1, 1));
        // A cursor exactly on a segment boundary belongs to the next segment.
        ed.row = 0;
        ed.col = 3;
        assert_eq!(ed.cursor_visual(), (1, 0));
        ed.row = 1;
        ed.col = 1;
        assert_eq!(ed.cursor_visual(), (3, 1));
    }

    #[test]
    fn move_up_down_walks_wrapped_segments() {
        let mut ed = Editor::untitled();
        ed.lines = vec!["aa bb cc".to_string()];
        ed.wrap_w = 4;
        ed.view_h = 24;
        ed.row = 0;
        ed.col = 0;
        ed.move_down();
        assert_eq!((ed.row, ed.col), (0, 3), "down to the bb segment");
        ed.move_down();
        assert_eq!((ed.row, ed.col), (0, 6), "down to the cc segment");
        ed.move_down();
        assert_eq!((ed.row, ed.col), (0, 6), "down at the end is a no-op");
        ed.move_up();
        assert_eq!((ed.row, ed.col), (0, 3), "up keeps the visual column");
        ed.move_up();
        assert_eq!((ed.row, ed.col), (0, 0));
        ed.move_up();
        assert_eq!((ed.row, ed.col), (0, 0), "up at the top is a no-op");
    }

    #[test]
    fn wrapped_highlight_slices_spans_per_segment() {
        let p = tmpfile("wrap.txt", b"aa bb cc\n");
        let mut ed = Editor::open(&p).unwrap();
        ed.wrap_w = 4;
        let theme = &crate::theme::THEMES[0];
        let rows = ed.highlight_visible_wrapped(theme, 0, 10);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].first);
        assert!(!rows[1].first);
        assert_eq!(rows[0].buf_row, 0);
        // The sliced spans still spell out the segment text.
        let text: String = rows
            .iter()
            .map(|r| {
                r.spans
                    .iter()
                    .map(|s| s.content.clone().into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(text, "aa |bb |cc");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    // -- find -----------------------------------------------------------------

    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

    fn find_key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: mods,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn find_matches_are_case_insensitive_and_char_based() {
        let p = tmpfile("f.txt", "abc ABC\nxabc\nnothing\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        assert!(ed.find.is_none(), "bar starts closed");
        ed.open_find();
        assert!(ed.find.is_some());
        assert_eq!(ed.find_bar_text(), "Find: ");
        ed.find.as_mut().unwrap().query = "ABC".to_string();
        // Uppercase query matches lowercase text; char-based columns.
        assert_eq!(ed.find_matches(), vec![(0, 0), (0, 4), (1, 1)]);
        assert!(!ed.find.as_ref().unwrap().not_found);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn find_typing_jumps_live_and_wraps() {
        let p = tmpfile("f.txt", "nothing here\nfoo bar\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_find();
        // Typing "foo" jumps live to the first match at/after the cursor,
        // wrapping past the end of the buffer.
        for c in "foo".chars() {
            assert!(ed.find_input(find_key(KeyCode::Char(c), KeyModifiers::NONE)));
        }
        assert_eq!(ed.find.as_ref().unwrap().query, "foo");
        assert_eq!((ed.row, ed.col), (1, 0));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn find_jump_next_prev_wrap() {
        let p = tmpfile("f.txt", "foo bar\nbaz foo\nfoo qux\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_find();
        ed.find.as_mut().unwrap().query = "foo".to_string();
        ed.find_research();
        assert_eq!((ed.row, ed.col), (0, 0));
        assert_eq!(ed.find_bar_text(), "Find: foo [1/3]");
        ed.find_jump(1);
        assert_eq!((ed.row, ed.col), (1, 4));
        assert_eq!(ed.find_bar_text(), "Find: foo [2/3]");
        ed.find_jump(1);
        assert_eq!((ed.row, ed.col), (2, 0));
        ed.find_jump(1); // wraps around
        assert_eq!((ed.row, ed.col), (0, 0));
        ed.find_jump(-1); // previous wraps to the last match
        assert_eq!((ed.row, ed.col), (2, 0));
        ed.find_jump(-1);
        assert_eq!((ed.row, ed.col), (1, 4));
        // Closing the bar leaves the text cursor at the last match.
        ed.close_find();
        assert!(ed.find.is_none());
        assert_eq!((ed.row, ed.col), (1, 4));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn find_no_match_sets_not_found() {
        let p = tmpfile("f.txt", "hello\nworld\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_find();
        ed.find.as_mut().unwrap().query = "zzz".to_string();
        ed.find_jump(1);
        assert!(ed.find.as_ref().unwrap().not_found);
        assert_eq!(ed.find_bar_text(), "Find: zzz — not found");
        // The cursor doesn't move when nothing matches.
        assert_eq!((ed.row, ed.col), (0, 0));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn find_match_range_tracks_and_clears() {
        let p = tmpfile("f.txt", "foo bar\nbaz foo\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        assert!(ed.find_match.is_none());
        ed.open_find();
        assert!(ed.find_match.is_none(), "no match before typing");
        ed.find.as_mut().unwrap().query = "foo".to_string();
        ed.find_research();
        assert_eq!(ed.find_match, Some((0, 0, 3)));
        ed.find_jump(1);
        assert_eq!(ed.find_match, Some((1, 4, 7)));
        // No match: range cleared.
        ed.find.as_mut().unwrap().query = "zzz".to_string();
        ed.find_research();
        assert!(ed.find_match.is_none());
        // Closing the bar clears the range but keeps the text cursor.
        ed.find.as_mut().unwrap().query = "foo".to_string();
        ed.find_research();
        assert!(ed.find_match.is_some());
        let (r, c) = (ed.row, ed.col);
        ed.close_find();
        assert!(ed.find_match.is_none());
        assert_eq!((ed.row, ed.col), (r, c));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn find_input_edits_query_and_clears_selection() {
        let p = tmpfile("f.txt", "hello\nworld hello\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_find();
        ed.sel_anchor = Some((0, 0)); // a mouse selection is cleared by a jump
        for c in "hello".chars() {
            ed.find_input(find_key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(ed.find.as_ref().unwrap().query, "hello");
        assert_eq!((ed.row, ed.col), (0, 0));
        assert!(ed.sel_anchor.is_none(), "jumping clears the selection");
        // Backspace edits the query (cursor moves with it); Left/Right,
        // Home, End move the query cursor.
        ed.find_input(find_key(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(ed.find.as_ref().unwrap().query, "hell");
        assert_eq!(ed.find.as_ref().unwrap().cursor, 4);
        ed.find_input(find_key(KeyCode::Left, KeyModifiers::NONE));
        ed.find_input(find_key(KeyCode::Char('p'), KeyModifiers::NONE));
        assert_eq!(ed.find.as_ref().unwrap().query, "helpl");
        ed.find_input(find_key(KeyCode::Home, KeyModifiers::NONE));
        assert_eq!(ed.find.as_ref().unwrap().cursor, 0);
        ed.find_input(find_key(KeyCode::End, KeyModifiers::NONE));
        assert_eq!(ed.find.as_ref().unwrap().cursor, 5);
        // Ctrl+chars are eaten (they never reach the document).
        assert!(ed.find_input(find_key(KeyCode::Char('s'), KeyModifiers::CONTROL)));
        assert_eq!(ed.find.as_ref().unwrap().query, "helpl");
        // With no bar open the key is not consumed.
        ed.close_find();
        assert!(!ed.find_input(find_key(KeyCode::Char('x'), KeyModifiers::NONE)));
        assert_eq!(
            ed.lines,
            vec!["hello".to_string(), "world hello".to_string()]
        );
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    // -- replace --------------------------------------------------------------

    #[test]
    fn open_replace_keeps_query_from_find_bar() {
        let p = tmpfile("rk.txt", "foo bar\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_find();
        for c in "foo".chars() {
            ed.find_input(find_key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(ed.find_bar_text(), "Find: foo [1/1]");
        // Ctrl+R switches the open bar into replace mode, keeping the query.
        ed.open_replace();
        let f = ed.find.as_ref().unwrap();
        assert!(f.replace_mode);
        assert_eq!(f.query, "foo");
        assert_eq!(f.replace, "");
        assert!(!f.replace_active, "Find field stays focused");
        assert_eq!(ed.find_match, Some((0, 0, 3)), "highlight survives");
        assert_eq!(ed.find_bar_text(), "Find: foo█ → Replace:  [1/1]");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn replace_tab_toggles_active_field() {
        let p = tmpfile("rt.txt", "hello\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_replace();
        assert!(!ed.find.as_ref().unwrap().replace_active);
        // Tab switches to the Replace field; typing lands there and does
        // not re-search.
        ed.find_input(find_key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(ed.find.as_ref().unwrap().replace_active);
        for c in "ab".chars() {
            ed.find_input(find_key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(ed.find.as_ref().unwrap().replace, "ab");
        assert_eq!(ed.find.as_ref().unwrap().query, "");
        assert!(ed.find_match.is_none(), "replace edits don't search");
        // Tab switches back; typing lands in the query and jumps live.
        ed.find_input(find_key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(!ed.find.as_ref().unwrap().replace_active);
        for c in "hell".chars() {
            ed.find_input(find_key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(ed.find.as_ref().unwrap().query, "hell");
        assert_eq!((ed.row, ed.col), (0, 0));
        assert_eq!(ed.find_match, Some((0, 0, 4)));
        // Backspace in the Replace field edits the replacement only.
        ed.find_input(find_key(KeyCode::Tab, KeyModifiers::NONE));
        ed.find_input(find_key(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(ed.find.as_ref().unwrap().replace, "a");
        assert_eq!(ed.find.as_ref().unwrap().query, "hell");
        // In plain find mode Tab is eaten (no field switching).
        ed.close_find();
        ed.open_find();
        ed.find_input(find_key(KeyCode::Tab, KeyModifiers::NONE));
        let f = ed.find.as_ref().unwrap();
        assert!(!f.replace_mode && !f.replace_active && f.query.is_empty());
        // Esc closes the bar (routed by the input layer to close_find).
        ed.close_find();
        assert!(ed.find.is_none());
        assert!(ed.find_match.is_none());
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn replace_current_replaces_highlighted_match_and_advances() {
        let p = tmpfile("r.txt", "foo bar\nfoo baz\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_replace();
        for c in "foo".chars() {
            ed.find_input(find_key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        ed.find_input(find_key(KeyCode::Tab, KeyModifiers::NONE));
        for c in "qux".chars() {
            ed.find_input(find_key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(ed.find_match, Some((0, 0, 3)));
        assert_eq!(ed.find_bar_text(), "Find: foo → Replace: qux█ [1/2]");
        ed.replace_current();
        assert_eq!(ed.lines, vec!["qux bar".to_string(), "foo baz".to_string()]);
        assert!(ed.dirty, "replace marks the buffer modified");
        // The cursor lands after the inserted text, on the next match —
        // the replacement itself is never re-matched.
        assert_eq!((ed.row, ed.col), (1, 0));
        assert_eq!(ed.find_match, Some((1, 0, 3)));
        assert_eq!(ed.find_bar_text(), "Find: foo → Replace: qux█ [1/1]");
        // One undo entry covers the replacement.
        assert_eq!(ed.undo.len(), 1);
        ed.undo();
        assert_eq!(ed.lines, vec!["foo bar".to_string(), "foo baz".to_string()]);
        ed.undo();
        assert_eq!(ed.message, "Nothing to undo");
        assert_eq!(ed.lines, vec!["foo bar".to_string(), "foo baz".to_string()]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn replace_all_counts_and_is_single_undo() {
        let p = tmpfile("ra.txt", "Foo bar\nfoo FOO\nnothing\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_replace();
        ed.find.as_mut().unwrap().query = "foo".to_string();
        ed.find.as_mut().unwrap().replace = "x".to_string();
        let n = ed.replace_all();
        assert_eq!(n, 3, "case-insensitive across lines");
        assert_eq!(
            ed.lines,
            vec![
                "x bar".to_string(),
                "x x".to_string(),
                "nothing".to_string()
            ]
        );
        assert!(ed.dirty);
        let text = ed.find_bar_text();
        assert!(text.contains("— not found"), "matches are gone: {text}");
        assert!(text.contains("— replaced 3"), "count is shown: {text}");
        // The whole batch is a single undo entry.
        assert_eq!(ed.undo.len(), 1);
        ed.undo();
        assert_eq!(
            ed.lines,
            vec![
                "Foo bar".to_string(),
                "foo FOO".to_string(),
                "nothing".to_string()
            ]
        );
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn replace_all_is_non_overlapping() {
        let p = tmpfile("ro.txt", "aaaa\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_replace();
        ed.find.as_mut().unwrap().query = "aa".to_string();
        ed.find.as_mut().unwrap().replace = "b".to_string();
        assert_eq!(ed.replace_all(), 2);
        assert_eq!(ed.lines, vec!["bb".to_string()]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn replace_done_clears_on_next_edit() {
        let p = tmpfile("rd.txt", "foo foo\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_replace();
        ed.find.as_mut().unwrap().query = "foo".to_string();
        ed.find.as_mut().unwrap().replace = "bar".to_string();
        assert_eq!(ed.replace_all(), 2);
        assert_eq!(ed.find.as_ref().unwrap().replace_done, Some(2));
        // Editing the replacement clears the count.
        ed.find_input(find_key(KeyCode::Tab, KeyModifiers::NONE));
        ed.find_input(find_key(KeyCode::Char('z'), KeyModifiers::NONE));
        assert_eq!(ed.find.as_ref().unwrap().replace, "zbar");
        assert_eq!(ed.find.as_ref().unwrap().replace_done, None);
        assert_eq!(ed.replace_all(), 0, "no matches left");
        assert_eq!(ed.find.as_ref().unwrap().replace_done, Some(0));
        // Editing the query clears it too.
        ed.find_input(find_key(KeyCode::Tab, KeyModifiers::NONE));
        ed.find_input(find_key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(ed.find.as_ref().unwrap().replace_done, None);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn replace_with_empty_query_is_noop() {
        let p = tmpfile("re.txt", "foo\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.open_replace();
        ed.replace_current();
        assert_eq!(ed.lines, vec!["foo".to_string()]);
        assert!(!ed.dirty);
        assert_eq!(ed.undo.len(), 0);
        assert_eq!(ed.replace_all(), 0);
        assert_eq!(ed.lines, vec!["foo".to_string()]);
        assert!(!ed.dirty);
        assert_eq!(ed.undo.len(), 0, "no undo entry for a no-op batch");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn duplicate_line_copies_below_and_follows() {
        let p = tmpfile("d.txt", "aaa\nbbb\nccc\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.row = 1;
        ed.col = 2;
        ed.duplicate_line();
        assert_eq!(ed.lines, vec!["aaa", "bbb", "bbb", "ccc"]);
        assert_eq!((ed.row, ed.col), (2, 2), "cursor follows the duplicate");
        assert!(ed.dirty);
        ed.undo();
        assert_eq!(ed.lines, vec!["aaa", "bbb", "ccc"], "one undo entry");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn duplicate_line_with_selection_duplicates_block() {
        let p = tmpfile("d2.txt", "aaa\nbbb\nccc\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        // Select all of lines 0..=1 (anchor at (0,0), cursor at (2,0)).
        ed.sel_anchor = Some((0, 0));
        ed.row = 2;
        ed.col = 0;
        ed.duplicate_line();
        assert_eq!(ed.lines, vec!["aaa", "bbb", "aaa", "bbb", "ccc"]);
        assert_eq!((ed.row, ed.col), (4, 0), "cursor moves with the block");
        assert_eq!(ed.sel_anchor, Some((2, 0)), "anchor moves with the block");
        ed.undo();
        assert_eq!(ed.lines, vec!["aaa", "bbb", "ccc"]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn move_line_up_down_and_boundaries() {
        let p = tmpfile("m.txt", "aaa\nbbb\nccc\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.row = 1;
        ed.col = 1;
        ed.move_line(-1);
        assert_eq!(ed.lines, vec!["bbb", "aaa", "ccc"]);
        assert_eq!((ed.row, ed.col), (0, 1), "cursor follows the moved line");
        ed.move_line(-1); // already at top: no-op
        assert_eq!(ed.lines, vec!["bbb", "aaa", "ccc"]);
        ed.move_line(1);
        ed.move_line(1);
        assert_eq!(ed.lines, vec!["aaa", "ccc", "bbb"]);
        ed.move_line(1); // already at bottom: no-op
        assert_eq!(ed.lines, vec!["aaa", "ccc", "bbb"]);
        assert!(ed.dirty);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn move_line_block_with_selection() {
        let p = tmpfile("m2.txt", "aaa\nbbb\nccc\nddd\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.sel_anchor = Some((1, 0));
        ed.row = 2;
        ed.col = 3;
        ed.move_line(1);
        assert_eq!(ed.lines, vec!["aaa", "ddd", "bbb", "ccc"]);
        assert_eq!((ed.row, ed.col), (3, 3));
        assert_eq!(ed.sel_anchor, Some((2, 0)), "selection rides along");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn indent_and_dedent_selection() {
        let p = tmpfile("i.txt", "aaa\n  bbb\nccc\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.sel_anchor = Some((0, 1));
        ed.row = 2;
        ed.col = 2;
        ed.indent_selection();
        assert_eq!(ed.lines, vec!["    aaa", "      bbb", "    ccc"]);
        assert_eq!(ed.sel_anchor, Some((0, 5)));
        assert_eq!((ed.row, ed.col), (2, 6));
        ed.dedent_selection();
        assert_eq!(ed.lines, vec!["aaa", "  bbb", "ccc"]);
        assert_eq!(ed.sel_anchor, Some((0, 1)));
        assert_eq!((ed.row, ed.col), (2, 2));
        // Dedent removes fewer than 4 spaces when there are fewer.
        ed.dedent_selection();
        assert_eq!(ed.lines, vec!["aaa", "bbb", "ccc"]);
        ed.undo();
        assert_eq!(ed.lines, vec!["aaa", "  bbb", "ccc"], "one undo entry");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn dedent_line_without_selection() {
        let p = tmpfile("i2.txt", "    aaa\nbbb\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.row = 0;
        ed.col = 6;
        ed.dedent_line();
        assert_eq!(ed.lines[0], "aaa");
        assert_eq!((ed.row, ed.col), (0, 2));
        assert!(ed.dirty);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn delete_word_back_and_forward() {
        let p = tmpfile("w.txt", "foo bar baz\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.row = 0;
        ed.col = 11; // end of "baz"
        ed.delete_word_back();
        assert_eq!(ed.lines[0], "foo bar ");
        assert_eq!((ed.row, ed.col), (0, 8));
        ed.delete_word_forward(); // at end of line: no-op
        assert_eq!(ed.lines[0], "foo bar ");
        ed.col = 4; // start of "bar"
        ed.delete_word_forward();
        assert_eq!(ed.lines[0], "foo  ");
        assert_eq!((ed.row, ed.col), (0, 4));
        ed.undo();
        assert_eq!(ed.lines[0], "foo bar ");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn delete_word_back_at_line_start_joins_lines() {
        let p = tmpfile("w2.txt", "foo\nbar\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.row = 1;
        ed.col = 0;
        ed.delete_word_back();
        assert_eq!(ed.lines, vec!["foobar"], "newline joined like Backspace");
        assert_eq!((ed.row, ed.col), (0, 3));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn delete_word_at_buffer_start_is_noop() {
        let p = tmpfile("w3.txt", "foo\n".as_bytes());
        let mut ed = Editor::open(&p).unwrap();
        ed.row = 0;
        ed.col = 0;
        ed.delete_word_back();
        assert_eq!(ed.lines, vec!["foo".to_string()]);
        assert_eq!(ed.undo.len(), 0, "no undo entry for a no-op");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
