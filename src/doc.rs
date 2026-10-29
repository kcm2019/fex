//! Word (.docx) viewer/editor.
//!
//! Opened with `e` on a `.docx` file. The document is a zip package; fex
//! keeps every part byte-for-byte and only ever rewrites
//! `word/document.xml`. Paragraphs and runs keep their original XML for
//! anything unedited, and an edited run keeps its original `w:rPr`
//! property element, so saving preserves all formatting the view doesn't
//! explicitly change — images, headers/footers, styles, numbering and
//! parts fex doesn't model at all included.
//!
//! Model: the body is a list of blocks (paragraphs, tables, and raw XML
//! for anything else, kept in order). A paragraph is a list of items —
//! text runs, hyperlink groups, and zero-width anchors (bookmarks etc.)
//! kept verbatim. A paragraph containing tracked changes, content
//! controls or fields is "locked": shown, but not text-editable, so its
//! raw XML is never regenerated.
//!
//! Formatting display resolves the OOXML cascade: document defaults →
//! paragraph style chain → character style → direct run properties.

use std::collections::HashMap;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Where the cursor is: a body block, optionally a table cell within it,
/// a paragraph within that container, and a char offset in the
/// paragraph's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loc {
    pub block: usize,
    pub cell: Option<(usize, usize)>,
    pub para: usize,
    pub offset: usize,
}

/// One run of text sharing a single `w:rPr` property element (kept as
/// ordered raw children so formatting toggles can edit it in place).
#[derive(Debug, Clone)]
pub struct Run {
    /// (tag, raw element XML) children of `w:rPr`, in document order.
    pub rpr: Vec<(String, String)>,
    pub text: String,
    /// Character style id from `w:rStyle`, if any.
    pub char_style: Option<String>,
    /// Inside a `w:hyperlink` (rendering only).
    pub link: bool,
}

impl Run {
    fn plain(text: &str) -> Run {
        Run {
            rpr: Vec::new(),
            text: text.to_string(),
            char_style: None,
            link: false,
        }
    }
}

/// A non-run child of a paragraph, kept verbatim.
#[derive(Debug, Clone)]
pub enum Item {
    Run(Run),
    /// `w:hyperlink` with its original attributes; runs stay editable.
    Link {
        attrs: String,
        runs: Vec<Run>,
    },
    /// Zero-width XML kept in place (bookmarks, proofing marks...).
    Anchor(String),
    /// A run we don't model (image, field, symbol): raw XML plus the
    /// placeholder text to display for it.
    Opaque {
        raw: String,
        label: String,
    },
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    /// Original `<w:p>` attributes (rsids etc.), without angle brackets.
    pub attrs: String,
    /// (tag, raw element XML) children of `w:pPr`, in document order.
    pub ppr: Vec<(String, String)>,
    pub items: Vec<Item>,
    /// Original paragraph XML; used verbatim while `modified` is false.
    pub raw: Option<String>,
    pub modified: bool,
    /// Contains tracked changes / content controls / fields: display
    /// only, so the raw XML is never regenerated.
    pub locked: bool,
}

impl Paragraph {
    fn new_empty() -> Paragraph {
        Paragraph {
            attrs: String::new(),
            ppr: Vec::new(),
            items: Vec::new(),
            raw: None,
            modified: true,
            locked: false,
        }
    }

    /// The paragraph's editable text: all run texts concatenated
    /// (opaque runs and anchors contribute nothing).
    pub fn text(&self) -> String {
        let mut s = String::new();
        for item in &self.items {
            match item {
                Item::Run(r) => s.push_str(&r.text),
                Item::Link { runs, .. } => {
                    for r in runs {
                        s.push_str(&r.text);
                    }
                }
                _ => {}
            }
        }
        s
    }

    pub fn char_len(&self) -> usize {
        self.text().chars().count()
    }

    /// Paragraph style id from `w:pStyle`, if set.
    pub fn style_id(&self) -> Option<String> {
        self.ppr
            .iter()
            .find(|(t, _)| t == "w:pStyle")
            .and_then(|(_, raw)| attr_val(raw, "w:val"))
    }

    /// Justification from `w:jc` (left/center/right/both...), if set.
    pub fn alignment(&self) -> Option<String> {
        self.ppr
            .iter()
            .find(|(t, _)| t == "w:jc")
            .and_then(|(_, raw)| attr_val(raw, "w:val"))
    }

    /// List info from `w:numPr`: (level, numId), if this paragraph is a
    /// list item by its own properties.
    pub fn num_pr(&self) -> Option<(usize, u32)> {
        let (_, raw) = self.ppr.iter().find(|(t, _)| t == "w:numPr")?;
        let ilvl = child_attr_val(raw, "w:ilvl", "w:val")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let num = child_attr_val(raw, "w:numId", "w:val").and_then(|v| v.parse().ok())?;
        Some((ilvl, num))
    }
}

#[derive(Debug, Clone)]
pub struct Cell {
    /// Raw `w:tcPr` element, kept verbatim.
    pub tcpr: String,
    pub paras: Vec<Paragraph>,
    /// Whole-cell raw XML when the cell holds something unmodelled
    /// (a nested table, a content control...): display-only cell.
    pub raw: Option<String>,
    pub modified: bool,
}

#[derive(Debug, Clone)]
pub struct Row {
    /// Raw `w:trPr` element, kept verbatim.
    pub trpr: String,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub attrs: String,
    /// Raw `w:tblPr` element, kept verbatim.
    pub tblpr: String,
    /// Raw `w:tblGrid` element plus its column widths (twips).
    pub grid_raw: String,
    pub grid: Vec<u32>,
    pub rows: Vec<Row>,
    pub raw: Option<String>,
    pub modified: bool,
}

#[derive(Debug, Clone)]
pub enum Block {
    Para(Paragraph),
    Table(Table),
    /// Body-level XML we don't model (section properties, bookmarks,
    /// body-level content controls...), kept in order, not displayed.
    Raw(String),
}

/// Effective character formatting after the style cascade.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffFmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<(u8, u8, u8)>,
    pub highlight: Option<(u8, u8, u8)>,
    /// Font size in points.
    pub size_pt: Option<f32>,
    pub font: Option<String>,
    pub caps: bool,
    /// "superscript" / "subscript".
    pub vert: Option<String>,
}

/// One styled piece of a display line.
#[derive(Debug, Clone)]
pub struct DocSpan {
    pub text: String,
    pub fmt: EffFmt,
    pub link: bool,
    /// Placeholder for an opaque run (image/field): occupies display
    /// width but no character offsets.
    pub placeholder: bool,
    /// Char range [start, end) within the paragraph's text.
    pub start: usize,
    pub end: usize,
}

/// A display line produced by layout: either a (wrapped) paragraph line,
/// one horizontal slice of a table row, or a table border rule.
#[derive(Debug, Clone)]
pub enum DocLine {
    Para {
        block: usize,
        cell: Option<(usize, usize)>,
        para: usize,
        /// Char range of the paragraph text on this line.
        start: usize,
        end: usize,
        spans: Vec<DocSpan>,
        /// List marker + indent prefix, drawn before the spans.
        prefix: String,
        align: String,
        heading: Option<u8>,
    },
    TableSlice {
        block: usize,
        row: usize,
        /// Per column: (paragraph index in cell, char range, spans).
        /// A None entry is an empty continuation (cell has fewer lines).
        cells: Vec<Option<(usize, usize, usize, Vec<DocSpan>)>>,
    },
    Rule(String),
}

/// One entry of the parsed styles.xml.
#[derive(Debug, Clone, Default)]
pub struct StyleInfo {
    pub name: String,
    pub based_on: Option<String>,
    /// Character properties as (tag → raw attr value summary): we keep
    /// the same (tag, raw) child list shape as runs for merging.
    pub rpr: Vec<(String, String)>,
    pub jc: Option<String>,
    pub num: Option<(usize, u32)>,
}

/// Parsed word/numbering.xml: numId → abstract id, abstract id → levels.
#[derive(Debug, Default)]
pub struct Numbering {
    pub nums: HashMap<u32, u32>,
    /// abstractNumId → ilvl → (format, level text template, start).
    pub levels: HashMap<u32, HashMap<usize, (String, String, i64)>>,
}

pub struct Doc {
    pub path: PathBuf,
    /// Every zip part, in original order: (name, bytes). document.xml is
    /// regenerated from the model on save; all other parts are written
    /// back byte-for-byte.
    parts: Vec<(String, Vec<u8>)>,
    /// Everything in document.xml before the first body child (the XML
    /// declaration, the `w:document` open tag, the `w:body` open tag).
    xml_head: String,
    /// Everything from `</w:body>` on.
    xml_tail: String,
    pub blocks: Vec<Block>,
    pub styles: HashMap<String, StyleInfo>,
    pub default_rpr: Vec<(String, String)>,
    pub default_style: Option<String>,
    pub numbering: Numbering,
    pub cursor: Loc,
    pub anchor: Option<Loc>,
    /// First visible layout line.
    pub top: usize,
    pub dirty: bool,
    pub message: String,
    pub confirm_discard: bool,
    /// Layout cache.
    layout: Vec<DocLine>,
    layout_width: usize,
    layout_stale: bool,
    /// Set by the renderer: where the cursor landed on screen.
    pub cursor_xy: Option<(u16, u16)>,
    pub view_h: usize,
}

// ---------------------------------------------------------------------------
// Small XML helpers
// ---------------------------------------------------------------------------

/// Extract an attribute value from a raw element string, e.g.
/// attr_val("<w:pStyle w:val=\"Heading1\"/>", "w:val") → "Heading1".
/// Values in captured raw XML are still XML-escaped; the attributes we
/// read (ids, enums, hex) never contain escapes in practice.
fn attr_val(raw: &str, attr: &str) -> Option<String> {
    let pat = format!("{attr}=\"");
    let start = raw.find(&pat)? + pat.len();
    let end = raw[start..].find('"')? + start;
    Some(raw[start..end].to_string())
}

/// Extract an attribute from a named child element inside a raw element,
/// e.g. child_attr_val(numPr_raw, "w:ilvl", "w:val").
fn child_attr_val(raw: &str, child: &str, attr: &str) -> Option<String> {
    let start = raw.find(&format!("<{child}"))?;
    attr_val(&raw[start..], attr)
}

/// Escape text for inclusion in an XML text node.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Serialize one element's events (already positioned at its Start or
/// Empty event) from `reader` into raw XML text, consuming through the
/// matching End event.
fn capture_element(
    reader: &mut Reader<&[u8]>,
    first: &Event,
    buf: &mut Vec<u8>,
) -> io::Result<String> {
    let mut out: Vec<u8> = Vec::new();
    {
        let mut w = Writer::new(&mut out);
        w.write_event(first.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        if matches!(first, Event::Start(_)) {
            let mut depth = 1usize;
            loop {
                buf.clear();
                let ev = reader
                    .read_event_into(buf)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                match &ev {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => {
                        depth -= 1;
                        w.write_event(ev.clone()).map_err(|e| {
                            io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                        })?;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    Event::Eof => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "unterminated element in document.xml",
                        ))
                    }
                    _ => {}
                }
                w.write_event(ev)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
            }
        }
    }
    String::from_utf8(out).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

/// The attributes of a start tag as a raw string (no angle brackets),
/// rebuilt from the parsed attributes so it round-trips faithfully.
fn start_attrs(e: &quick_xml::events::BytesStart) -> String {
    let mut s = String::new();
    for a in e.attributes().flatten() {
        s.push(' ');
        s.push_str(&String::from_utf8_lossy(a.key.as_ref()));
        s.push_str("=\"");
        s.push_str(&String::from_utf8_lossy(&a.value));
        s.push('"');
    }
    s
}

/// Split an element's raw XML into (element attrs, Vec<(child tag, child
/// raw)>) for property elements like w:rPr / w:pPr. Non-element content
/// (whitespace text) is dropped.
fn split_props(raw: &str) -> (String, Vec<(String, String)>) {
    let bytes = raw.as_bytes();
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let mut children = Vec::new();
    let mut attrs = String::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if children.is_empty() && attrs.is_empty() {
                    // The property element itself: record attrs, then
                    // capture each child.
                    attrs = start_attrs(&e);
                    let mut inner = Vec::new();
                    loop {
                        inner.clear();
                        match reader.read_event_into(&mut inner) {
                            Ok(Event::Start(c)) => {
                                let raw_child =
                                    capture_element(&mut reader, &Event::Start(c), &mut Vec::new())
                                        .unwrap_or_default();
                                // Tag name is recomputed below.
                                let tag = child_tag(&raw_child);
                                children.push((tag, raw_child));
                            }
                            Ok(Event::Empty(c)) => {
                                let raw_child = {
                                    let mut out = Vec::new();
                                    let mut w = Writer::new(&mut out);
                                    let _ = w.write_event(Event::Empty(c));
                                    String::from_utf8(out).unwrap_or_default()
                                };
                                let tag = child_tag(&raw_child);
                                children.push((tag, raw_child));
                            }
                            Ok(Event::End(_)) | Ok(Event::Eof) => break,
                            _ => {}
                        }
                    }
                    break;
                }
            }
            Ok(Event::Empty(e)) => {
                attrs = start_attrs(&e);
                break;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    (attrs, children)
}

/// The tag name of a raw element string ("<w:b w:val=.../>" → "w:b").
fn child_tag(raw: &str) -> String {
    let s = raw.strip_prefix('<').unwrap_or(raw);
    let end = s
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(s.len());
    s[..end].to_string()
}

/// Join property children back into an element string.
fn join_props(tag: &str, attrs: &str, children: &[(String, String)]) -> String {
    if children.is_empty() {
        return String::new();
    }
    let mut s = format!("<{tag}{attrs}>");
    for (_, raw) in children {
        s.push_str(raw);
    }
    s.push_str(&format!("</{tag}>"));
    s
}

/// Child order inside w:rPr (OOXML schema sequence), used when a
/// formatting toggle inserts a new property element.
const RPR_ORDER: &[&str] = &[
    "w:rStyle",
    "w:rFonts",
    "w:b",
    "w:bCs",
    "w:i",
    "w:iCs",
    "w:caps",
    "w:smallCaps",
    "w:strike",
    "w:dstrike",
    "w:outline",
    "w:shadow",
    "w:emboss",
    "w:imprint",
    "w:noProof",
    "w:snapToGrid",
    "w:vanish",
    "w:webHidden",
    "w:color",
    "w:spacing",
    "w:w",
    "w:kern",
    "w:position",
    "w:sz",
    "w:szCs",
    "w:highlight",
    "w:u",
    "w:effect",
    "w:bdr",
    "w:shd",
    "w:fitText",
    "w:vertAlign",
    "w:rtl",
    "w:cs",
    "w:em",
    "w:lang",
    "w:eastAsianLayout",
    "w:specVanish",
    "w:oMath",
];

/// Child order inside w:pPr (OOXML schema sequence).
const PPR_ORDER: &[&str] = &[
    "w:pStyle",
    "w:keepNext",
    "w:keepLines",
    "w:pageBreakBefore",
    "w:framePr",
    "w:widowControl",
    "w:numPr",
    "w:suppressLineNumbers",
    "w:pBdr",
    "w:shd",
    "w:tabs",
    "w:suppressAutoHyphens",
    "w:kinsoku",
    "w:wordWrap",
    "w:overflowPunct",
    "w:topLinePunct",
    "w:autoSpaceDE",
    "w:autoSpaceDN",
    "w:bidi",
    "w:adjustRightInd",
    "w:snapToGrid",
    "w:spacing",
    "w:ind",
    "w:contextualSpacing",
    "w:mirrorIndents",
    "w:suppressOverlap",
    "w:jc",
    "w:textDirection",
    "w:textAlignment",
    "w:textboxTightWrap",
    "w:outlineLvl",
    "w:divId",
    "w:cnfStyle",
    "w:rPr",
    "w:sectPr",
    "w:pPrChange",
];

/// Insert or replace a property child, keeping schema order.
fn set_prop(children: &mut Vec<(String, String)>, order: &[&str], tag: &str, raw: String) {
    if let Some(pos) = children.iter().position(|(t, _)| t == tag) {
        children[pos].1 = raw;
        return;
    }
    let my_rank = order.iter().position(|t| *t == tag).unwrap_or(usize::MAX);
    let pos = children
        .iter()
        .position(|(t, _)| order.iter().position(|x| x == t).unwrap_or(usize::MAX) > my_rank)
        .unwrap_or(children.len());
    children.insert(pos, (tag.to_string(), raw));
}

/// Remove a property child if present.
fn remove_prop(children: &mut Vec<(String, String)>, tag: &str) {
    children.retain(|(t, _)| t != tag);
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Read a text part (document.xml, styles.xml...) from the package.
fn part_text(parts: &[(String, Vec<u8>)], name: &str) -> Option<String> {
    parts
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
}

/// Open a .docx file into an editable Doc.
pub fn open_doc(path: &Path) -> io::Result<Doc> {
    let data = std::fs::read(path)?;
    let mut zin = zip::ZipArchive::new(Cursor::new(data))?;
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..zin.len() {
        let mut f = zin.by_index(i)?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)?;
        parts.push((f.name().to_string(), bytes));
    }
    let xml = part_text(&parts, "word/document.xml").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "not a Word document (no word/document.xml)",
        )
    })?;

    // Head = through the <w:body...> open tag; tail = from </w:body>.
    let body_at = find_tag(&xml, "<w:body")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "document.xml has no body"))?;
    let body_open_end = xml[body_at..]
        .find('>')
        .map(|i| body_at + i + 1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad body tag"))?;
    let tail_at = xml.rfind("</w:body>").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "document.xml body never closes")
    })?;
    let xml_head = xml[..body_open_end].to_string();
    let xml_tail = xml[tail_at..].to_string();
    let inner = &xml[body_open_end..tail_at];

    let mut blocks = Vec::new();
    let mut reader = Reader::from_str(inner);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let ev = Event::Start(e.to_owned());
                let raw = capture_element(&mut reader, &ev, &mut Vec::new())?;
                match tag.as_str() {
                    "w:p" => blocks.push(Block::Para(parse_paragraph(&raw))),
                    "w:tbl" => blocks.push(Block::Table(parse_table(&raw))),
                    _ => blocks.push(Block::Raw(raw)),
                }
            }
            Ok(Event::Empty(e)) => {
                let mut out = Vec::new();
                let mut w = Writer::new(&mut out);
                let _ = w.write_event(Event::Empty(e.to_owned()));
                blocks.push(Block::Raw(String::from_utf8_lossy(&out).into_owned()));
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("document.xml: {e}"),
                ))
            }
        }
    }

    let (styles, default_rpr, default_style) = match part_text(&parts, "word/styles.xml") {
        Some(s) => parse_styles(&s),
        None => (HashMap::new(), Vec::new(), None),
    };
    let numbering = match part_text(&parts, "word/numbering.xml") {
        Some(s) => parse_numbering(&s),
        None => Numbering::default(),
    };

    Ok(Doc {
        path: path.to_path_buf(),
        parts,
        xml_head,
        xml_tail,
        blocks,
        styles,
        default_rpr,
        default_style,
        numbering,
        cursor: Loc {
            block: 0,
            cell: None,
            para: 0,
            offset: 0,
        },
        anchor: None,
        top: 0,
        dirty: false,
        message: String::new(),
        confirm_discard: false,
        layout: Vec::new(),
        layout_width: 0,
        layout_stale: true,
        cursor_xy: None,
        view_h: 20,
    })
}

/// Find "<w:body" as a tag start (not e.g. "<w:bodyPr").
fn find_tag(xml: &str, pat: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = xml[from..].find(pat) {
        let at = from + i;
        let next = xml[at + pat.len()..].chars().next();
        match next {
            Some(c) if c == '>' || c == '/' || c.is_whitespace() => return Some(at),
            _ => from = at + pat.len(),
        }
    }
    None
}

/// Tags whose presence anywhere in a paragraph locks it against text
/// edits (tracked changes, content controls, simple fields): their
/// structure is more than runs, and regenerating would lose it.
const LOCK_TAGS: &[&str] = &[
    "<w:ins",
    "<w:del",
    "<w:moveFrom",
    "<w:moveTo",
    "<w:sdt",
    "<w:fldSimple",
    "<w:customXml",
];

fn paragraph_locked(raw: &str) -> bool {
    LOCK_TAGS.iter().any(|t| raw.contains(t))
}

fn parse_paragraph(raw: &str) -> Paragraph {
    let mut para = Paragraph {
        attrs: String::new(),
        ppr: Vec::new(),
        items: Vec::new(),
        raw: Some(raw.to_string()),
        modified: false,
        locked: paragraph_locked(raw),
    };
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    para.attrs = start_attrs(&e);
                    first = false;
                    continue;
                }
                let ev = Event::Start(e.to_owned());
                let child_raw =
                    capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                push_paragraph_child(&mut para, &tag, child_raw);
            }
            Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    // An empty <w:p/> element.
                    para.attrs = start_attrs(&e);
                    first = false;
                    continue;
                }
                let mut out = Vec::new();
                let mut w = Writer::new(&mut out);
                let _ = w.write_event(Event::Empty(e.to_owned()));
                let child_raw = String::from_utf8_lossy(&out).into_owned();
                push_paragraph_child(&mut para, &tag, child_raw);
            }
            Ok(Event::End(_)) | Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    // A locked paragraph whose structure we didn't model (tracked
    // changes, fields) still needs display text: gather every w:t /
    // w:delText into one read-only run. It is never serialized — the
    // paragraph is locked, so its raw XML is always used on save.
    if para.locked && para.text().is_empty() {
        let text = collect_texts(raw);
        if !text.is_empty() {
            para.items.push(Item::Run(Run::plain(&text)));
        }
    }
    para
}

/// All text inside w:t / w:delText elements of a raw XML fragment.
fn collect_texts(raw: &str) -> String {
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut inside = false;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let n = e.name();
                inside = n.as_ref() == b"w:t" || n.as_ref() == b"w:delText";
            }
            Ok(Event::End(_)) => inside = false,
            Ok(Event::Text(e)) => {
                if inside {
                    out.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    out
}

fn push_paragraph_child(para: &mut Paragraph, tag: &str, raw: String) {
    match tag {
        "w:pPr" => {
            let (_, children) = split_props(&raw);
            para.ppr = children;
        }
        "w:r" => para.items.push(parse_run(&raw, false)),
        "w:hyperlink" => {
            // Editable when every child is a plain run; else opaque.
            let (attrs, runs, ok) = parse_hyperlink(&raw);
            if ok {
                para.items.push(Item::Link { attrs, runs });
            } else {
                para.items.push(Item::Opaque {
                    label: String::from("[link]"),
                    raw,
                });
            }
        }
        _ => para.items.push(Item::Anchor(raw)),
    }
}

/// Parse a w:hyperlink element: (attrs, runs, all-plain-runs?).
fn parse_hyperlink(raw: &str) -> (String, Vec<Run>, bool) {
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut attrs = String::new();
    let mut runs = Vec::new();
    let mut ok = true;
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    attrs = start_attrs(&e);
                    first = false;
                    continue;
                }
                let ev = Event::Start(e.to_owned());
                let child_raw =
                    capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                if tag == "w:r" {
                    match parse_run(&child_raw, true) {
                        Item::Run(r) => runs.push(r),
                        _ => ok = false,
                    }
                } else {
                    ok = false;
                }
            }
            Ok(Event::Empty(e)) => {
                if first {
                    attrs = start_attrs(&e);
                    first = false;
                }
            }
            Ok(Event::End(_)) | Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    (attrs, runs, ok)
}

/// Parse a w:r element into an Item: a text Run, or Opaque when the run
/// carries content we don't model (drawings, fields, symbols...).
fn parse_run(raw: &str, link: bool) -> Item {
    let mut run = Run {
        rpr: Vec::new(),
        text: String::new(),
        char_style: None,
        link,
    };
    let mut opaque_label: Option<String> = None;
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    first = false;
                    continue;
                }
                let ev = Event::Start(e.to_owned());
                let child_raw =
                    capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                match tag.as_str() {
                    "w:rPr" => {
                        let (_, children) = split_props(&child_raw);
                        run.char_style = children
                            .iter()
                            .find(|(t, _)| t == "w:rStyle")
                            .and_then(|(_, r)| attr_val(r, "w:val"));
                        run.rpr = children;
                    }
                    "w:t" => {
                        run.text.push_str(&element_text(&child_raw));
                    }
                    "w:tab" => run.text.push('\t'),
                    "w:br" | "w:cr" => {
                        if attr_val(&child_raw, "w:type").as_deref() == Some("page") {
                            opaque_label = Some(String::from("[page break]"));
                        } else {
                            run.text.push('\n');
                        }
                    }
                    "w:drawing" | "w:pict" | "w:object" => {
                        opaque_label = Some(String::from("[image]"));
                    }
                    "w:fldChar" | "w:instrText" => {
                        opaque_label = Some(String::from("[field]"));
                    }
                    "w:sym" => {
                        opaque_label = Some(String::from("[symbol]"));
                    }
                    "w:footnoteReference" | "w:endnoteReference" => {
                        opaque_label = Some(String::from("[note]"));
                    }
                    "w:commentReference" => {
                        opaque_label = Some(String::from("[comment]"));
                    }
                    _ => {
                        opaque_label = Some(String::from("[…]"));
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    first = false;
                    continue;
                }
                match tag.as_str() {
                    "w:tab" => run.text.push('\t'),
                    "w:br" | "w:cr" => run.text.push('\n'),
                    "w:t" => {}
                    _ => {
                        // Self-closed content (w:drawing is never empty,
                        // but be safe): treat unknown empties as opaque
                        // only when they carry content semantics.
                        if tag != "w:rPr" {
                            opaque_label = Some(match tag.as_str() {
                                "w:fldChar" | "w:instrText" => String::from("[field]"),
                                _ => String::from("[…]"),
                            });
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                // Text directly under w:r only occurs inside w:t, which
                // is captured above; ignore stray text.
                let _ = e;
            }
            Ok(Event::End(_)) | Ok(Event::Eof) => {
                // Distinguish the run's own end from inner ends: the
                // reader is flat here because children were captured
                // whole, so any End before Eof is the run's own.
                if !first {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    match opaque_label {
        Some(label) => Item::Opaque {
            raw: raw.to_string(),
            label,
        },
        None => Item::Run(run),
    }
}

/// Text content of a simple element like <w:t>…</w:t>.
fn element_text(raw: &str) -> String {
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => out.push_str(&e.unescape().unwrap_or_default()),
            Ok(Event::CData(e)) => out.push_str(&String::from_utf8_lossy(&e)),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    out
}

fn parse_table(raw: &str) -> Table {
    let mut table = Table {
        attrs: String::new(),
        tblpr: String::new(),
        grid_raw: String::new(),
        grid: Vec::new(),
        rows: Vec::new(),
        raw: Some(raw.to_string()),
        modified: false,
    };
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    table.attrs = start_attrs(&e);
                    first = false;
                    continue;
                }
                let ev = Event::Start(e.to_owned());
                let child_raw =
                    capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                match tag.as_str() {
                    "w:tblPr" => table.tblpr = child_raw,
                    "w:tblGrid" => {
                        table.grid = grid_widths(&child_raw);
                        table.grid_raw = child_raw;
                    }
                    "w:tr" => table.rows.push(parse_row(&child_raw)),
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                if first {
                    table.attrs = start_attrs(&e);
                    first = false;
                }
            }
            Ok(Event::End(_)) | Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    table
}

fn grid_widths(grid_raw: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut rest = grid_raw;
    while let Some(i) = rest.find("<w:gridCol") {
        let seg = &rest[i..];
        if let Some(w) = attr_val(seg, "w:w").and_then(|v| v.parse().ok()) {
            out.push(w);
        } else {
            out.push(0);
        }
        rest = &seg[10..];
    }
    out
}

fn parse_row(raw: &str) -> Row {
    let mut row = Row {
        trpr: String::new(),
        cells: Vec::new(),
    };
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    first = false;
                    continue;
                }
                let ev = Event::Start(e.to_owned());
                let child_raw =
                    capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                match tag.as_str() {
                    "w:trPr" => row.trpr = child_raw,
                    "w:tc" => row.cells.push(parse_cell(&child_raw)),
                    _ => {}
                }
            }
            Ok(Event::Empty(_)) => {
                if first {
                    first = false;
                }
            }
            Ok(Event::End(_)) | Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    row
}

fn parse_cell(raw: &str) -> Cell {
    // A cell with a nested table or content control stays raw.
    let locked = raw.contains("<w:tbl") || raw.contains("<w:sdt");
    let mut cell = Cell {
        tcpr: String::new(),
        paras: Vec::new(),
        raw: if locked { Some(raw.to_string()) } else { None },
        modified: false,
    };
    if locked {
        // Still parse its paragraphs for display.
    }
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if first {
                    first = false;
                    continue;
                }
                let ev = Event::Start(e.to_owned());
                let child_raw =
                    capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                match tag.as_str() {
                    "w:tcPr" => cell.tcpr = child_raw,
                    "w:p" => cell.paras.push(parse_paragraph(&child_raw)),
                    _ => {}
                }
            }
            Ok(Event::Empty(_)) => {
                if first {
                    first = false;
                }
            }
            Ok(Event::End(_)) | Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    if cell.paras.is_empty() {
        cell.paras.push(Paragraph::new_empty());
    }
    cell
}

/// Parse word/styles.xml: style map, document-default run properties,
/// and the default paragraph style id.
fn parse_styles(
    xml: &str,
) -> (
    HashMap<String, StyleInfo>,
    Vec<(String, String)>,
    Option<String>,
) {
    let mut styles = HashMap::new();
    let mut defaults = Vec::new();
    let mut default_para: Option<String> = None;
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let ev = Event::Start(e.to_owned());
                match tag.as_str() {
                    "w:docDefaults" => {
                        let raw =
                            capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                        // docDefaults/rPrDefault/rPr children.
                        if let Some(rpr_raw) = find_element(&raw, "w:rPr") {
                            defaults = split_props(&rpr_raw).1;
                        }
                    }
                    "w:style" => {
                        let attrs = start_attrs(&e);
                        let raw = capture_element(
                            &mut reader,
                            &Event::Start(e.to_owned()),
                            &mut Vec::new(),
                        )
                        .unwrap_or_default();
                        let id = attr_val(&attrs, "w:styleId").unwrap_or_default();
                        let typ = attr_val(&attrs, "w:type").unwrap_or_default();
                        let is_default = attr_val(&attrs, "w:default").as_deref() == Some("1");
                        let mut info = StyleInfo::default();
                        info.name = find_element(&raw, "w:name")
                            .and_then(|r| attr_val(&r, "w:val"))
                            .unwrap_or_else(|| id.clone());
                        info.based_on =
                            find_element(&raw, "w:basedOn").and_then(|r| attr_val(&r, "w:val"));
                        if let Some(rpr_raw) = find_element(&raw, "w:rPr") {
                            info.rpr = split_props(&rpr_raw).1;
                        }
                        if let Some(ppr_raw) = find_element(&raw, "w:pPr") {
                            let (_, ppr) = split_props(&ppr_raw);
                            info.jc = ppr
                                .iter()
                                .find(|(t, _)| t == "w:jc")
                                .and_then(|(_, r)| attr_val(r, "w:val"));
                            info.num =
                                ppr.iter().find(|(t, _)| t == "w:numPr").and_then(|(_, r)| {
                                    let ilvl = child_attr_val(r, "w:ilvl", "w:val")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(0);
                                    let num = child_attr_val(r, "w:numId", "w:val")
                                        .and_then(|v| v.parse().ok())?;
                                    Some((ilvl, num))
                                });
                        }
                        if is_default && typ == "paragraph" {
                            default_para = Some(id.clone());
                        }
                        if !id.is_empty() {
                            styles.insert(id, info);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    (styles, defaults, default_para)
}

/// Find the first element with this tag in a raw XML fragment and return
/// its raw XML (start-tag-only scanning is enough for our flat lookups:
/// it returns the text from "<tag" through the matching close, using the
/// capture reader on a fresh parse).
fn find_element(raw: &str, tag: &str) -> Option<String> {
    let mut reader = Reader::from_str(raw);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == tag.as_bytes() {
                    let ev = Event::Start(e.to_owned());
                    return capture_element(&mut reader, &ev, &mut Vec::new()).ok();
                }
            }
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == tag.as_bytes() {
                    let mut out = Vec::new();
                    let mut w = Writer::new(&mut out);
                    let _ = w.write_event(Event::Empty(e.to_owned()));
                    return String::from_utf8(out).ok();
                }
            }
            Ok(Event::Eof) => return None,
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

/// Parse word/numbering.xml into the Numbering tables.
fn parse_numbering(xml: &str) -> Numbering {
    let mut numbering = Numbering::default();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let ev = Event::Start(e.to_owned());
                match tag.as_str() {
                    "w:num" => {
                        let attrs = start_attrs(&e);
                        let raw =
                            capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                        if let (Some(id), Some(abs)) = (
                            attr_val(&attrs, "w:numId").and_then(|v| v.parse().ok()),
                            find_element(&raw, "w:abstractNumId")
                                .and_then(|r| attr_val(&r, "w:val"))
                                .and_then(|v| v.parse().ok()),
                        ) {
                            numbering.nums.insert(id, abs);
                        }
                    }
                    "w:abstractNum" => {
                        let attrs = start_attrs(&e);
                        let raw =
                            capture_element(&mut reader, &ev, &mut Vec::new()).unwrap_or_default();
                        if let Some(abs) =
                            attr_val(&attrs, "w:abstractNumId").and_then(|v| v.parse().ok())
                        {
                            let mut levels = HashMap::new();
                            // Each w:lvl child: ilvl attr + numFmt/lvlText/start.
                            let mut sub = Reader::from_str(&raw);
                            let mut sbuf = Vec::new();
                            loop {
                                sbuf.clear();
                                match sub.read_event_into(&mut sbuf) {
                                    Ok(Event::Start(le)) => {
                                        if le.name().as_ref() == b"w:lvl" {
                                            let lattrs = start_attrs(&le);
                                            let lraw = capture_element(
                                                &mut sub,
                                                &Event::Start(le.to_owned()),
                                                &mut Vec::new(),
                                            )
                                            .unwrap_or_default();
                                            let ilvl = attr_val(&lattrs, "w:ilvl")
                                                .and_then(|v| v.parse().ok())
                                                .unwrap_or(0);
                                            let fmt = find_element(&lraw, "w:numFmt")
                                                .and_then(|r| attr_val(&r, "w:val"))
                                                .unwrap_or_else(|| String::from("bullet"));
                                            let text = find_element(&lraw, "w:lvlText")
                                                .and_then(|r| attr_val(&r, "w:val"))
                                                .unwrap_or_else(|| String::from("\u{2022}"));
                                            let start = find_element(&lraw, "w:start")
                                                .and_then(|r| attr_val(&r, "w:val"))
                                                .and_then(|v| v.parse().ok())
                                                .unwrap_or(1);
                                            levels.insert(ilvl, (fmt, text, start));
                                        }
                                    }
                                    Ok(Event::Eof) => break,
                                    Ok(_) => {}
                                    Err(_) => break,
                                }
                            }
                            numbering.levels.insert(abs, levels);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    numbering
}

// ---------------------------------------------------------------------------
// Serialization & save
// ---------------------------------------------------------------------------

fn run_xml(r: &Run) -> String {
    let mut s = String::from("<w:r>");
    s.push_str(&join_props("w:rPr", "", &r.rpr));
    // Text with tabs and line breaks as their own elements.
    let mut seg = String::new();
    let flush = |seg: &mut String, out: &mut String| {
        if !seg.is_empty() {
            out.push_str("<w:t xml:space=\"preserve\">");
            out.push_str(&escape_text(seg));
            out.push_str("</w:t>");
            seg.clear();
        }
    };
    for c in r.text.chars() {
        match c {
            '\t' => {
                flush(&mut seg, &mut s);
                s.push_str("<w:tab/>");
            }
            '\n' => {
                flush(&mut seg, &mut s);
                s.push_str("<w:br/>");
            }
            _ => seg.push(c),
        }
    }
    flush(&mut seg, &mut s);
    s.push_str("</w:r>");
    s
}

fn item_xml(item: &Item) -> String {
    match item {
        Item::Run(r) => run_xml(r),
        Item::Link { attrs, runs } => {
            let mut s = format!("<w:hyperlink{attrs}>");
            for r in runs {
                s.push_str(&run_xml(r));
            }
            s.push_str("</w:hyperlink>");
            s
        }
        Item::Anchor(raw) => raw.clone(),
        Item::Opaque { raw, .. } => raw.clone(),
    }
}

fn paragraph_xml(p: &Paragraph) -> String {
    if !p.modified {
        if let Some(raw) = &p.raw {
            return raw.clone();
        }
    }
    let mut s = format!("<w:p{}>", p.attrs);
    s.push_str(&join_props("w:pPr", "", &p.ppr));
    for item in &p.items {
        s.push_str(&item_xml(item));
    }
    s.push_str("</w:p>");
    s
}

fn cell_xml(c: &Cell) -> String {
    if !c.modified {
        if let Some(raw) = &c.raw {
            return raw.clone();
        }
    }
    let mut s = String::from("<w:tc>");
    s.push_str(&c.tcpr);
    for p in &c.paras {
        s.push_str(&paragraph_xml(p));
    }
    s.push_str("</w:tc>");
    s
}

fn table_xml(t: &Table) -> String {
    if !t.modified {
        if let Some(raw) = &t.raw {
            return raw.clone();
        }
    }
    let mut s = format!("<w:tbl{}>", t.attrs);
    s.push_str(&t.tblpr);
    s.push_str(&t.grid_raw);
    for row in &t.rows {
        s.push_str("<w:tr>");
        s.push_str(&row.trpr);
        for cell in &row.cells {
            s.push_str(&cell_xml(cell));
        }
        s.push_str("</w:tr>");
    }
    s.push_str("</w:tbl>");
    s
}

impl Doc {
    /// Rebuild word/document.xml from the model.
    fn document_xml(&self) -> String {
        let mut s = String::with_capacity(self.xml_head.len() + 4096);
        s.push_str(&self.xml_head);
        for block in &self.blocks {
            match block {
                Block::Para(p) => s.push_str(&paragraph_xml(p)),
                Block::Table(t) => s.push_str(&table_xml(t)),
                Block::Raw(raw) => s.push_str(raw),
            }
        }
        s.push_str(&self.xml_tail);
        s
    }

    /// Save: rewrite the package with a regenerated document.xml; every
    /// other part is copied byte-for-byte.
    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            self.message = String::from("No changes to save");
            return Ok(());
        }
        let doc_xml = self.document_xml();
        let mut out: Vec<u8> = Vec::new();
        {
            let mut zout = zip::ZipWriter::new(Cursor::new(&mut out));
            let opts = zip::write::SimpleFileOptions::default();
            for (name, bytes) in &self.parts {
                zout.start_file(name, opts)?;
                if name == "word/document.xml" {
                    zout.write_all(doc_xml.as_bytes())?;
                } else {
                    zout.write_all(bytes)?;
                }
            }
            zout.finish()?;
        }
        std::fs::write(&self.path, &out)?;
        // The in-memory parts copy now matches disk for document.xml.
        let doc_bytes = doc_xml.into_bytes();
        for (name, bytes) in &mut self.parts {
            if name == "word/document.xml" {
                *bytes = doc_bytes.clone();
            }
        }
        self.dirty = false;
        self.message = format!("Saved {}", self.path.display());
        Ok(())
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.layout_stale = true;
    }
}

// ---------------------------------------------------------------------------
// Formatting resolution
// ---------------------------------------------------------------------------

/// Merge one (tag, raw) property child into an EffFmt (later merges win).
fn merge_prop(fmt: &mut EffFmt, tag: &str, raw: &str) {
    match tag {
        "w:b" => fmt.bold = on_off(raw),
        "w:i" => fmt.italic = on_off(raw),
        "w:u" => {
            fmt.underline = attr_val(raw, "w:val").as_deref() != Some("none");
        }
        "w:strike" => fmt.strike = on_off(raw),
        "w:caps" => fmt.caps = on_off(raw),
        "w:color" => {
            if let Some(v) = attr_val(raw, "w:val") {
                fmt.color = parse_hex_color(&v).or(fmt.color);
            }
        }
        "w:highlight" => {
            if let Some(v) = attr_val(raw, "w:val") {
                fmt.highlight = highlight_rgb(&v).or(fmt.highlight);
            }
        }
        "w:sz" => {
            if let Some(v) = attr_val(raw, "w:val") {
                if let Ok(hp) = v.parse::<f32>() {
                    fmt.size_pt = Some(hp / 2.0);
                }
            }
        }
        "w:rFonts" => {
            if let Some(v) = attr_val(raw, "w:ascii") {
                fmt.font = Some(v);
            }
        }
        "w:vertAlign" => {
            fmt.vert = attr_val(raw, "w:val");
        }
        _ => {}
    }
}

/// A boolean OOXML property: present without w:val means on; w:val can
/// be false/0/off/no.
fn on_off(raw: &str) -> bool {
    match attr_val(raw, "w:val").as_deref() {
        None => true,
        Some("false") | Some("0") | Some("off") | Some("no") => false,
        _ => true,
    }
}

fn parse_hex_color(v: &str) -> Option<(u8, u8, u8)> {
    if v.len() != 6 || v == "auto" {
        return None;
    }
    let r = u8::from_str_radix(&v[0..2], 16).ok()?;
    let g = u8::from_str_radix(&v[2..4], 16).ok()?;
    let b = u8::from_str_radix(&v[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Word highlight color names → RGB.
fn highlight_rgb(name: &str) -> Option<(u8, u8, u8)> {
    Some(match name {
        "yellow" => (255, 255, 0),
        "green" => (0, 255, 0),
        "cyan" => (0, 255, 255),
        "magenta" => (255, 0, 255),
        "blue" => (0, 0, 255),
        "red" => (255, 0, 0),
        "darkBlue" => (0, 0, 139),
        "darkCyan" => (0, 139, 139),
        "darkGray" => (169, 169, 169),
        "darkGreen" => (0, 100, 0),
        "darkMagenta" => (139, 0, 139),
        "darkRed" => (139, 0, 0),
        "darkYellow" => (189, 183, 107),
        "lightGray" => (211, 211, 211),
        "black" => (0, 0, 0),
        _ => return None,
    })
}

impl Doc {
    /// The style chain for a paragraph style id, root ancestor first.
    fn style_chain(&self, id: &str) -> Vec<&StyleInfo> {
        let mut chain = Vec::new();
        let mut cur = Some(id.to_string());
        let mut guard = 0;
        while let Some(sid) = cur {
            guard += 1;
            if guard > 16 {
                break;
            }
            match self.styles.get(&sid) {
                Some(info) => {
                    cur = info.based_on.clone();
                    chain.push(info);
                }
                None => break,
            }
        }
        chain.reverse();
        chain
    }

    /// Effective formatting for a run in a paragraph: document defaults,
    /// then the paragraph style chain, then the character style, then
    /// the run's own properties.
    pub fn eff_fmt(&self, para: &Paragraph, run: &Run) -> EffFmt {
        let mut fmt = EffFmt::default();
        for (t, raw) in &self.default_rpr {
            merge_prop(&mut fmt, t, raw);
        }
        if let Some(sid) = para.style_id().or_else(|| self.default_style.clone()) {
            for info in self.style_chain(&sid) {
                for (t, raw) in &info.rpr {
                    merge_prop(&mut fmt, t, raw);
                }
            }
        }
        if let Some(csid) = &run.char_style {
            for info in self.style_chain(csid) {
                for (t, raw) in &info.rpr {
                    merge_prop(&mut fmt, t, raw);
                }
            }
        }
        for (t, raw) in &run.rpr {
            merge_prop(&mut fmt, t, raw);
        }
        fmt
    }

    /// Effective alignment: paragraph's own jc, else its style's.
    pub fn eff_alignment(&self, para: &Paragraph) -> String {
        if let Some(jc) = para.alignment() {
            return jc;
        }
        if let Some(sid) = para.style_id().or_else(|| self.default_style.clone()) {
            for info in self.style_chain(&sid) {
                if let Some(jc) = &info.jc {
                    return jc.clone();
                }
            }
        }
        String::from("left")
    }

    /// Effective list info: paragraph's own numPr, else its style's.
    pub fn eff_num(&self, para: &Paragraph) -> Option<(usize, u32)> {
        if let Some(n) = para.num_pr() {
            return Some(n);
        }
        if let Some(sid) = para.style_id().or_else(|| self.default_style.clone()) {
            for info in self.style_chain(&sid) {
                if let Some(n) = info.num {
                    return Some(n);
                }
            }
        }
        None
    }

    /// Heading level (1..=6) if the paragraph style is a heading style.
    pub fn heading_level(&self, para: &Paragraph) -> Option<u8> {
        let sid = para.style_id()?;
        let name = self
            .styles
            .get(&sid)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| sid.clone());
        let lower = name.to_lowercase();
        for lvl in 1..=6u8 {
            if lower == format!("heading {lvl}") || lower == format!("heading{lvl}") {
                return Some(lvl);
            }
        }
        None
    }

    pub fn style_name(&self, para: &Paragraph) -> String {
        match para.style_id().or_else(|| self.default_style.clone()) {
            Some(sid) => self.styles.get(&sid).map(|s| s.name.clone()).unwrap_or(sid),
            None => String::from("Normal"),
        }
    }
}

// ---------------------------------------------------------------------------
// Cursor, editing, layout
// ---------------------------------------------------------------------------

/// All paragraph addresses in document order (body paragraphs and table
/// cell paragraphs), for cursor movement.
fn para_addresses(blocks: &[Block]) -> Vec<(usize, Option<(usize, usize)>, usize)> {
    let mut out = Vec::new();
    for (bi, block) in blocks.iter().enumerate() {
        match block {
            Block::Para(_) => out.push((bi, None, 0)),
            Block::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        for pi in 0..cell.paras.len() {
                            out.push((bi, Some((r, c)), pi));
                        }
                    }
                }
            }
            Block::Raw(_) => {}
        }
    }
    out
}

impl Doc {
    pub fn para_at(&self, loc: Loc) -> Option<&Paragraph> {
        match self.blocks.get(loc.block)? {
            Block::Para(p) if loc.cell.is_none() => Some(p),
            Block::Table(t) => {
                let (r, c) = loc.cell?;
                t.rows.get(r)?.cells.get(c)?.paras.get(loc.para)
            }
            _ => None,
        }
    }

    pub fn para_at_mut(&mut self, loc: Loc) -> Option<&mut Paragraph> {
        match self.blocks.get_mut(loc.block)? {
            Block::Para(p) if loc.cell.is_none() => Some(p),
            Block::Table(t) => {
                let (r, c) = loc.cell?;
                t.rows.get_mut(r)?.cells.get_mut(c)?.paras.get_mut(loc.para)
            }
            _ => None,
        }
    }

    /// Mark a paragraph (and its cell/table) modified and the doc dirty.
    fn touch(&mut self, loc: Loc) {
        if let Some(p) = self.para_at_mut(loc) {
            p.modified = true;
        }
        if let Some((r, c)) = loc.cell {
            if let Some(Block::Table(t)) = self.blocks.get_mut(loc.block) {
                t.modified = true;
                if let Some(cell) = t.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)) {
                    cell.modified = true;
                }
            }
        }
        self.mark_dirty();
    }

    fn locked_msg(&mut self) {
        self.message = String::from("Paragraph has tracked changes or fields — it can't be edited");
    }

    /// The selection as (start, end) offsets when anchor and cursor are
    /// in the same paragraph; selections never span paragraphs.
    pub fn selection(&self) -> Option<(Loc, usize, usize)> {
        let a = self.anchor?;
        if (a.block, a.cell, a.para) != (self.cursor.block, self.cursor.cell, self.cursor.para) {
            return None;
        }
        let (lo, hi) = if a.offset <= self.cursor.offset {
            (a.offset, self.cursor.offset)
        } else {
            (self.cursor.offset, a.offset)
        };
        if lo == hi {
            None
        } else {
            Some((self.cursor, lo, hi))
        }
    }

    // -- run location ----------------------------------------------------

    /// Locate the run containing a char offset: returns (item index,
    /// run index within a link group or 0, char offset within the run).
    /// Boundary offsets prefer the run on the left (insertion goes after
    /// existing text), except offset 0 which takes the first run.
    fn run_locate(p: &Paragraph, offset: usize) -> Option<(usize, usize, usize)> {
        let mut base = 0usize;
        let mut last: Option<(usize, usize, usize)> = None;
        for (ii, item) in p.items.iter().enumerate() {
            let runs: &[Run] = match item {
                Item::Run(r) => std::slice::from_ref(r),
                Item::Link { runs, .. } => runs,
                _ => continue,
            };
            for (ri, r) in runs.iter().enumerate() {
                let len = r.text.chars().count();
                if offset == 0 && base == 0 {
                    return Some((ii, ri, 0));
                }
                if offset > base && offset <= base + len {
                    return Some((ii, ri, offset - base));
                }
                base += len;
                last = Some((ii, ri, len));
            }
        }
        // Offset at the very end (or no runs at all).
        if offset == base {
            return last;
        }
        None
    }

    /// Ensure a run boundary exists at `offset` inside the paragraph
    /// (splitting a run in two when the offset falls inside one).
    fn split_run_at(p: &mut Paragraph, offset: usize) {
        let mut base = 0usize;
        for ii in 0..p.items.len() {
            // Compute this item's runs' total and find a split point.
            let (is_link, total) = match &p.items[ii] {
                Item::Run(r) => (false, r.text.chars().count()),
                Item::Link { runs, .. } => {
                    (true, runs.iter().map(|r| r.text.chars().count()).sum())
                }
                _ => (false, 0),
            };
            if total == 0 {
                continue;
            }
            if offset > base && offset < base + total {
                let intra = offset - base;
                if is_link {
                    if let Item::Link { runs, .. } = &mut p.items[ii] {
                        let mut rbase = 0usize;
                        for ri in 0..runs.len() {
                            let len = runs[ri].text.chars().count();
                            if intra > rbase && intra < rbase + len {
                                let at = intra - rbase;
                                let tail: String = runs[ri].text.chars().skip(at).collect();
                                runs[ri].text = runs[ri].text.chars().take(at).collect();
                                let mut new_run = runs[ri].clone();
                                new_run.text = tail;
                                runs.insert(ri + 1, new_run);
                                return;
                            }
                            rbase += len;
                        }
                    }
                } else if let Item::Run(r) = &mut p.items[ii] {
                    let tail: String = r.text.chars().skip(intra).collect();
                    let mut new_run = r.clone();
                    new_run.text = tail;
                    r.text = r.text.chars().take(intra).collect();
                    p.items.insert(ii + 1, Item::Run(new_run));
                }
                return;
            }
            base += total;
        }
    }

    // -- text edits --------------------------------------------------------

    pub fn insert_char(&mut self, c: char) {
        if self.selection().is_some() {
            self.delete_selection();
        }
        let loc = self.cursor;
        let Some(p) = self.para_at(loc) else { return };
        if p.locked {
            self.locked_msg();
            return;
        }
        if c == '\n' {
            self.split_para();
            return;
        }
        let offset = loc.offset;
        match Self::run_locate(p, offset) {
            Some((ii, ri, intra)) => {
                let p = self.para_at_mut(loc).unwrap();
                let run = match &mut p.items[ii] {
                    Item::Run(r) => r,
                    Item::Link { runs, .. } => &mut runs[ri],
                    _ => unreachable!(),
                };
                run.text = insert_char_at(&run.text, intra, c);
            }
            None => {
                // No editable run yet: append a plain one.
                let p = self.para_at_mut(loc).unwrap();
                p.items.push(Item::Run(Run::plain(&c.to_string())));
            }
        }
        self.touch(loc);
        self.cursor.offset += 1;
        self.anchor = None;
    }

    pub fn insert_text(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '\r' => {}
                '\n' => self.split_para(),
                _ => self.insert_char(c),
            }
        }
    }

    pub fn delete_selection(&mut self) {
        let Some((loc, lo, hi)) = self.selection() else {
            return;
        };
        if self.para_at(loc).is_some_and(|p| p.locked) {
            self.locked_msg();
            return;
        }
        self.delete_range(loc, lo, hi);
        self.cursor.offset = lo;
        self.anchor = None;
    }

    /// Delete chars [lo, hi) of the paragraph at loc.
    fn delete_range(&mut self, loc: Loc, lo: usize, hi: usize) {
        Self::split_run_at(self.para_at_mut(loc).unwrap(), hi);
        Self::split_run_at(self.para_at_mut(loc).unwrap(), lo);
        // Now delete every run fully inside [lo, hi).
        let p = self.para_at_mut(loc).unwrap();
        let mut base = 0usize;
        for item in &mut p.items {
            let runs: Vec<&mut Run> = match item {
                Item::Run(r) => vec![r],
                Item::Link { runs, .. } => runs.iter_mut().collect(),
                _ => Vec::new(),
            };
            for r in runs {
                let len = r.text.chars().count();
                if base >= lo && base + len <= hi && len > 0 {
                    r.text.clear();
                }
                base += len;
            }
        }
        self.touch(loc);
    }

    pub fn backspace(&mut self) {
        if self.selection().is_some() {
            self.delete_selection();
            return;
        }
        let loc = self.cursor;
        let Some(p) = self.para_at(loc) else { return };
        if p.locked {
            self.locked_msg();
            return;
        }
        if loc.offset > 0 {
            self.delete_range(loc, loc.offset - 1, loc.offset);
            self.cursor.offset -= 1;
            self.anchor = None;
            return;
        }
        // Merge into the previous paragraph of the same container.
        let addrs = para_addresses(&self.blocks);
        let pos = addrs
            .iter()
            .position(|a| *a == (loc.block, loc.cell, loc.para));
        let Some(pos) = pos else { return };
        if pos == 0 {
            return;
        }
        let prev = addrs[pos - 1];
        if !same_container(prev, (loc.block, loc.cell, loc.para)) {
            self.message = String::from("Can't merge across a table boundary");
            return;
        }
        let prev_loc = Loc {
            block: prev.0,
            cell: prev.1,
            para: prev.2,
            offset: 0,
        };
        if self.para_at(prev_loc).is_some_and(|p| p.locked) {
            self.locked_msg();
            return;
        }
        let junction = self.para_at(prev_loc).map(|p| p.char_len()).unwrap_or(0);
        self.merge_paras(prev_loc, loc);
        self.cursor = Loc {
            offset: junction,
            ..prev_loc
        };
        self.anchor = None;
    }

    pub fn delete_forward(&mut self) {
        if self.selection().is_some() {
            self.delete_selection();
            return;
        }
        let loc = self.cursor;
        let Some(p) = self.para_at(loc) else { return };
        if p.locked {
            self.locked_msg();
            return;
        }
        let len = p.char_len();
        if loc.offset < len {
            self.delete_range(loc, loc.offset, loc.offset + 1);
            self.anchor = None;
            return;
        }
        // Merge the next paragraph of the same container into this one.
        let addrs = para_addresses(&self.blocks);
        let pos = addrs
            .iter()
            .position(|a| *a == (loc.block, loc.cell, loc.para));
        let Some(pos) = pos else { return };
        let Some(next) = addrs.get(pos + 1) else {
            return;
        };
        let next = *next;
        if !same_container((loc.block, loc.cell, loc.para), next) {
            self.message = String::from("Can't merge across a table boundary");
            return;
        }
        let next_loc = Loc {
            block: next.0,
            cell: next.1,
            para: next.2,
            offset: 0,
        };
        if self.para_at(next_loc).is_some_and(|p| p.locked) {
            self.locked_msg();
            return;
        }
        self.merge_paras(loc, next_loc);
        self.anchor = None;
    }

    /// Append `from`'s items to `into`'s paragraph and remove `from`.
    /// Both must be in the same container (body block or one cell).
    fn merge_paras(&mut self, into: Loc, from: Loc) {
        let from_para = match (into.cell, from.cell) {
            (None, None) => match self.blocks.remove(from.block) {
                Block::Para(p) => p,
                other => {
                    self.blocks.insert(from.block, other);
                    return;
                }
            },
            (Some(_), Some(_)) => {
                let Some(Block::Table(t)) = self.blocks.get_mut(from.block) else {
                    return;
                };
                let (r, c) = from.cell.unwrap();
                t.rows[r].cells[c].paras.remove(from.para)
            }
            _ => return,
        };
        if let Some(target) = self.para_at_mut(into) {
            target.items.extend(from_para.items);
        }
        self.touch(into);
        self.mark_dirty();
    }

    /// Enter: split the current paragraph at the cursor.
    pub fn split_para(&mut self) {
        if self.selection().is_some() {
            self.delete_selection();
        }
        let loc = self.cursor;
        let Some(p) = self.para_at(loc) else { return };
        if p.locked {
            self.locked_msg();
            return;
        }
        Self::split_run_at(self.para_at_mut(loc).unwrap(), loc.offset);
        let moved = {
            let p = self.para_at_mut(loc).unwrap();
            split_items_at(&mut p.items, loc.offset)
        };
        let mut new_para = Paragraph {
            attrs: String::new(),
            ppr: {
                let mut ppr = self.para_at(loc).map(|p| p.ppr.clone()).unwrap_or_default();
                strip_heading_style(&mut ppr, self);
                strip_sectpr(&mut ppr);
                ppr
            },
            items: moved,
            raw: None,
            modified: true,
            locked: false,
        };
        if new_para.items.is_empty() {
            new_para.items.push(Item::Run(Run::plain("")));
        }
        match loc.cell {
            None => {
                self.blocks.insert(loc.block + 1, Block::Para(new_para));
                self.cursor = Loc {
                    block: loc.block + 1,
                    cell: None,
                    para: 0,
                    offset: 0,
                };
            }
            Some((r, c)) => {
                if let Some(Block::Table(t)) = self.blocks.get_mut(loc.block) {
                    t.modified = true;
                    let cell = &mut t.rows[r].cells[c];
                    cell.modified = true;
                    cell.paras.insert(loc.para + 1, new_para);
                }
                self.cursor = Loc {
                    para: loc.para + 1,
                    offset: 0,
                    ..loc
                };
            }
        }
        self.touch(loc);
        self.mark_dirty();
        self.anchor = None;
    }

    // -- formatting (pick B) ------------------------------------------------

    /// Toggle bold/italic/underline over the selection, or the word at
    /// the cursor when nothing is selected.
    pub fn toggle_fmt(&mut self, kind: &str) {
        let loc = self.cursor;
        let (lo, hi) = match self.selection() {
            Some((_, lo, hi)) => (lo, hi),
            None => match word_range(self.para_at(loc), loc.offset) {
                Some(r) => r,
                None => {
                    self.message = String::from("Select text or put the cursor in a word first");
                    return;
                }
            },
        };
        if self.para_at(loc).is_some_and(|p| p.locked) {
            self.locked_msg();
            return;
        }
        // Effective state of every run overlapping [lo, hi).
        let p = self.para_at(loc).unwrap();
        let mut base = 0usize;
        let mut all_on = true;
        let mut any = false;
        for item in &p.items {
            let runs: &[Run] = match item {
                Item::Run(r) => std::slice::from_ref(r),
                Item::Link { runs, .. } => runs,
                _ => continue,
            };
            for r in runs {
                let len = r.text.chars().count();
                if base + len > lo && base < hi && len > 0 {
                    any = true;
                    let fmt = self.eff_fmt(p, r);
                    let on = match kind {
                        "b" => fmt.bold,
                        "i" => fmt.italic,
                        _ => fmt.underline,
                    };
                    if !on {
                        all_on = false;
                    }
                }
                base += len;
            }
        }
        if !any {
            self.message = String::from("Nothing to format there");
            return;
        }
        let target = !all_on;
        Self::split_run_at(self.para_at_mut(loc).unwrap(), hi);
        Self::split_run_at(self.para_at_mut(loc).unwrap(), lo);
        let mut base = 0usize;
        let p = self.para_at_mut(loc).unwrap();
        for item in &mut p.items {
            let runs: &mut [Run] = match item {
                Item::Run(r) => std::slice::from_mut(r),
                Item::Link { runs, .. } => runs,
                _ => continue,
            };
            for r in runs {
                let len = r.text.chars().count();
                if base >= lo && base + len <= hi && len > 0 {
                    match kind {
                        "b" => set_bool_prop(&mut r.rpr, "w:b", target),
                        "i" => set_bool_prop(&mut r.rpr, "w:i", target),
                        _ => {
                            if target {
                                set_prop(
                                    &mut r.rpr,
                                    RPR_ORDER,
                                    "w:u",
                                    String::from("<w:u w:val=\"single\"/>"),
                                );
                            } else {
                                set_prop(
                                    &mut r.rpr,
                                    RPR_ORDER,
                                    "w:u",
                                    String::from("<w:u w:val=\"none\"/>"),
                                );
                            }
                        }
                    }
                }
                base += len;
            }
        }
        self.touch(loc);
        self.message = format!(
            "{} {}",
            match kind {
                "b" => "Bold",
                "i" => "Italic",
                _ => "Underline",
            },
            if target { "on" } else { "off" }
        );
    }

    /// Set paragraph alignment: left / center / right / both.
    pub fn set_alignment(&mut self, val: &str) {
        let loc = self.cursor;
        if self.para_at(loc).is_some_and(|p| p.locked) {
            self.locked_msg();
            return;
        }
        let p = self.para_at_mut(loc).unwrap();
        if val == "left" {
            remove_prop(&mut p.ppr, "w:jc");
        } else {
            set_prop(
                &mut p.ppr,
                PPR_ORDER,
                "w:jc",
                format!("<w:jc w:val=\"{val}\"/>"),
            );
        }
        self.touch(loc);
        self.message = format!("Alignment: {val}");
    }

    // -- movement -----------------------------------------------------------

    /// Begin/extend/clear the selection around a movement.
    fn select_step(&mut self, selecting: bool) {
        if selecting {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        // Selections never span paragraphs: if movement landed in a
        // different paragraph than the anchor, clamp the anchor away —
        // the selection simply doesn't take effect there.
    }

    pub fn move_left(&mut self, selecting: bool) {
        self.select_step(selecting);
        if self.cursor.offset > 0 {
            self.cursor.offset -= 1;
        } else {
            self.goto_adjacent(-1, true);
        }
        self.finish_move(selecting);
    }

    pub fn move_right(&mut self, selecting: bool) {
        self.select_step(selecting);
        let len = self.para_at(self.cursor).map(|p| p.char_len()).unwrap_or(0);
        if self.cursor.offset < len {
            self.cursor.offset += 1;
        } else {
            self.goto_adjacent(1, false);
        }
        self.finish_move(selecting);
    }

    fn finish_move(&mut self, selecting: bool) {
        if selecting {
            if let Some(a) = self.anchor {
                if (a.block, a.cell, a.para)
                    != (self.cursor.block, self.cursor.cell, self.cursor.para)
                {
                    // Crossed a paragraph boundary: selection stays
                    // confined — drop the anchor (documented limit).
                    self.anchor = None;
                }
            }
        }
        self.layout_stale = true;
    }

    fn goto_adjacent(&mut self, dir: isize, to_end: bool) {
        let addrs = para_addresses(&self.blocks);
        let cur = (self.cursor.block, self.cursor.cell, self.cursor.para);
        let Some(pos) = addrs.iter().position(|a| *a == cur) else {
            return;
        };
        let next = pos as isize + dir;
        if next < 0 || next as usize >= addrs.len() {
            return;
        }
        let (b, c, p) = addrs[next as usize];
        self.cursor.block = b;
        self.cursor.cell = c;
        self.cursor.para = p;
        self.cursor.offset = if to_end {
            self.para_at(self.cursor).map(|x| x.char_len()).unwrap_or(0)
        } else {
            0
        };
    }

    pub fn move_word(&mut self, dir: isize, selecting: bool) {
        self.select_step(selecting);
        let loc = self.cursor;
        if let Some(p) = self.para_at(loc) {
            let text: Vec<char> = p.text().chars().collect();
            let mut i = loc.offset.min(text.len());
            if dir < 0 {
                while i > 0 && !text[i - 1].is_alphanumeric() {
                    i -= 1;
                }
                while i > 0 && text[i - 1].is_alphanumeric() {
                    i -= 1;
                }
            } else {
                while i < text.len() && text[i].is_alphanumeric() {
                    i += 1;
                }
                while i < text.len() && !text[i].is_alphanumeric() {
                    i += 1;
                }
            }
            self.cursor.offset = i;
        }
        self.finish_move(selecting);
    }

    pub fn move_home(&mut self, selecting: bool) {
        self.select_step(selecting);
        self.cursor.offset = 0;
        self.finish_move(selecting);
    }

    pub fn move_end(&mut self, selecting: bool) {
        self.select_step(selecting);
        self.cursor.offset = self.para_at(self.cursor).map(|p| p.char_len()).unwrap_or(0);
        self.finish_move(selecting);
    }

    /// Up/down by display lines (uses the cached layout).
    pub fn move_visual(&mut self, dlines: isize, selecting: bool) {
        self.select_step(selecting);
        let (line_idx, col) = match self.cursor_position() {
            Some(v) => v,
            None => return,
        };
        let target = (line_idx as isize + dlines).clamp(0, self.layout.len() as isize - 1) as usize;
        if let Some((loc, offset)) = self.offset_at_visual(target, col) {
            self.cursor = Loc { offset, ..loc };
        }
        self.finish_move(selecting);
    }

    /// Next/previous table cell (Tab navigation).
    pub fn tab_cell(&mut self, dir: isize) {
        let loc = self.cursor;
        if loc.cell.is_none() {
            return;
        }
        let addrs = para_addresses(&self.blocks);
        let cur = (loc.block, loc.cell, loc.para);
        let Some(pos) = addrs.iter().position(|a| *a == cur) else {
            return;
        };
        // Walk to the first paragraph of the next/previous cell of the
        // same table.
        let mut i = pos as isize + dir;
        while i >= 0 && (i as usize) < addrs.len() {
            let a = addrs[i as usize];
            if a.0 != loc.block {
                break;
            }
            if a.1 != loc.cell {
                self.cursor = Loc {
                    block: a.0,
                    cell: a.1,
                    para: a.2,
                    offset: 0,
                };
                self.anchor = None;
                return;
            }
            i += dir;
        }
    }
}

/// Are two paragraph addresses in the same editing container — both at
/// body level, or both in the very same table cell?
fn same_container(
    a: (usize, Option<(usize, usize)>, usize),
    b: (usize, Option<(usize, usize)>, usize),
) -> bool {
    match (a.1, b.1) {
        (None, None) => true,
        (Some(ca), Some(cb)) => a.0 == b.0 && ca == cb,
        _ => false,
    }
}

/// Insert a char into a string at a char index.
fn insert_char_at(s: &str, at: usize, c: char) -> String {
    let mut out: String = s.chars().take(at).collect();
    out.push(c);
    out.extend(s.chars().skip(at));
    out
}

/// Set a boolean run property explicitly (so it can override a style).
fn set_bool_prop(rpr: &mut Vec<(String, String)>, tag: &str, on: bool) {
    if on {
        set_prop(rpr, RPR_ORDER, tag, format!("<{tag}/>"));
    } else {
        set_prop(rpr, RPR_ORDER, tag, format!("<{tag} w:val=\"false\"/>"));
    }
}

/// After Enter on a heading, the new paragraph drops the heading style
/// (Word behavior: Enter after a heading continues in Normal style).
fn strip_heading_style(ppr: &mut Vec<(String, String)>, doc: &Doc) {
    if let Some(sid) = ppr
        .iter()
        .find(|(t, _)| t == "w:pStyle")
        .and_then(|(_, r)| attr_val(r, "w:val"))
    {
        let fake = Paragraph {
            attrs: String::new(),
            ppr: vec![(
                String::from("w:pStyle"),
                format!("<w:pStyle w:val=\"{sid}\"/>"),
            )],
            items: Vec::new(),
            raw: None,
            modified: true,
            locked: false,
        };
        if doc.heading_level(&fake).is_some() {
            remove_prop(ppr, "w:pStyle");
        }
    }
}

fn strip_sectpr(ppr: &mut Vec<(String, String)>) {
    remove_prop(ppr, "w:sectPr");
}

/// The word range around a char offset: (start, end) of the maximal
/// alphanumeric run containing or adjacent to the offset.
fn word_range(p: Option<&Paragraph>, offset: usize) -> Option<(usize, usize)> {
    let p = p?;
    let text: Vec<char> = p.text().chars().collect();
    if text.is_empty() {
        return None;
    }
    let mut i = offset.min(text.len());
    if i == text.len() || !text[i].is_alphanumeric() {
        if i > 0 && text[i - 1].is_alphanumeric() {
            i -= 1;
        } else {
            return None;
        }
    }
    let mut lo = i;
    while lo > 0 && text[lo - 1].is_alphanumeric() {
        lo -= 1;
    }
    let mut hi = i;
    while hi < text.len() && text[hi].is_alphanumeric() {
        hi += 1;
    }
    if lo == hi {
        None
    } else {
        Some((lo, hi))
    }
}

/// Split a paragraph's items at a char offset, returning the items that
/// move to the new paragraph. Assumes split_run_at already ran, so the
/// offset falls on a run boundary (possibly inside a link group, between
/// two of its runs).
fn split_items_at(items: &mut Vec<Item>, offset: usize) -> Vec<Item> {
    let taken = std::mem::take(items);
    let mut left: Vec<Item> = Vec::new();
    let mut right: Vec<Item> = Vec::new();
    let mut base = 0usize;
    let mut crossed = false;
    for item in taken {
        if crossed {
            right.push(item);
            continue;
        }
        match item {
            Item::Run(r) => {
                let len = r.text.chars().count();
                if offset == base {
                    right.push(Item::Run(r));
                    crossed = true;
                } else {
                    // offset == base + len (boundary after this run) or
                    // the run is entirely before the offset.
                    left.push(Item::Run(r));
                    if offset == base + len {
                        crossed = true;
                    }
                }
                base += len;
            }
            Item::Link { attrs, runs } => {
                let mut lbase = base;
                let mut l_runs: Vec<Run> = Vec::new();
                let mut r_runs: Vec<Run> = Vec::new();
                for r in runs {
                    let len = r.text.chars().count();
                    if crossed || offset == lbase {
                        r_runs.push(r);
                        crossed = true;
                    } else {
                        l_runs.push(r);
                        if offset == lbase + len {
                            crossed = true;
                        }
                    }
                    lbase += len;
                }
                base = lbase;
                if !l_runs.is_empty() {
                    left.push(Item::Link {
                        attrs: attrs.clone(),
                        runs: l_runs,
                    });
                }
                if !r_runs.is_empty() {
                    right.push(Item::Link {
                        attrs,
                        runs: r_runs,
                    });
                }
            }
            other => {
                // Anchors and opaque runs have no chars: they stay with
                // the left paragraph unless the split already happened.
                left.push(other);
            }
        }
    }
    *items = left;
    right
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// A display piece before wrapping: display text plus its char range.
struct Piece {
    text: String,
    fmt: EffFmt,
    link: bool,
    placeholder: bool,
    start: usize,
    end: usize,
}

/// Build the styled pieces of a paragraph in order.
fn para_pieces(doc: &Doc, p: &Paragraph) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut base = 0usize;
    for item in &p.items {
        match item {
            Item::Run(r) => {
                push_run_pieces(&mut out, doc.eff_fmt(p, r), r, base);
                base += r.text.chars().count();
            }
            Item::Link { runs, .. } => {
                for r in runs {
                    let mut fmt = doc.eff_fmt(p, r);
                    fmt.underline = true; // links render underlined
                    push_run_pieces(&mut out, fmt, r, base);
                    base += r.text.chars().count();
                }
            }
            Item::Opaque { label, .. } => {
                out.push(Piece {
                    text: label.clone(),
                    fmt: EffFmt::default(),
                    link: false,
                    placeholder: true,
                    start: base,
                    end: base,
                });
            }
            Item::Anchor(_) => {}
        }
    }
    out
}

fn push_run_pieces(out: &mut Vec<Piece>, fmt: EffFmt, r: &Run, base: usize) {
    // Split the run text at tabs/newlines so wrapping can treat them
    // specially; each piece keeps its exact char range.
    let mut cur = String::new();
    let mut start = base;
    let mut i = base;
    for c in r.text.chars() {
        if c == '\t' || c == '\n' {
            if !cur.is_empty() {
                out.push(Piece {
                    text: std::mem::take(&mut cur),
                    fmt: fmt.clone(),
                    link: r.link,
                    placeholder: false,
                    start,
                    end: i,
                });
            }
            out.push(Piece {
                text: if c == '\t' {
                    String::from("    ")
                } else {
                    String::from("\n")
                },
                fmt: fmt.clone(),
                link: r.link,
                placeholder: false,
                start: i,
                end: i + 1,
            });
            i += 1;
            start = i;
        } else {
            cur.push(c);
            i += 1;
        }
    }
    if !cur.is_empty() {
        out.push(Piece {
            text: cur,
            fmt,
            link: r.link,
            placeholder: false,
            start,
            end: i,
        });
    }
}

/// Word-wrap styled pieces into lines of at most `width` display cells.
/// Returns lines of spans; each span keeps its char range. A forced
/// break piece ("\n") ends the line.
fn wrap_pieces(pieces: Vec<Piece>, width: usize) -> Vec<Vec<DocSpan>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<DocSpan>> = Vec::new();
    let mut line: Vec<DocSpan> = Vec::new();
    let mut line_w = 0usize;
    let push_line = |lines: &mut Vec<Vec<DocSpan>>, line: &mut Vec<DocSpan>| {
        lines.push(std::mem::take(line));
    };
    // Tokenize into words (with ranges): a token is a run of non-space
    // chars, a single space, or a forced break.
    struct Tok {
        text: String,
        fmt: EffFmt,
        link: bool,
        placeholder: bool,
        start: usize,
        end: usize,
    }
    let mut toks: Vec<Tok> = Vec::new();
    for p in pieces {
        if p.text == "\n" {
            toks.push(Tok {
                text: String::from("\n"),
                fmt: p.fmt,
                link: p.link,
                placeholder: false,
                start: p.start,
                end: p.end,
            });
            continue;
        }
        if p.placeholder {
            toks.push(Tok {
                text: p.text,
                fmt: p.fmt,
                link: false,
                placeholder: true,
                start: p.start,
                end: p.end,
            });
            continue;
        }
        // Split at spaces, tracking char offsets 1:1 (tabs were expanded
        // into a 4-space piece whose range is a single char: handle it
        // as one token so ranges stay exact).
        if p.end - p.start == 1 && p.text.chars().count() > 1 {
            toks.push(Tok {
                text: p.text,
                fmt: p.fmt,
                link: p.link,
                placeholder: false,
                start: p.start,
                end: p.end,
            });
            continue;
        }
        let mut cur = String::new();
        let mut cur_start = p.start;
        let mut i = p.start;
        for c in p.text.chars() {
            if c == ' ' {
                if !cur.is_empty() {
                    toks.push(Tok {
                        text: std::mem::take(&mut cur),
                        fmt: p.fmt.clone(),
                        link: p.link,
                        placeholder: false,
                        start: cur_start,
                        end: i,
                    });
                }
                toks.push(Tok {
                    text: String::from(" "),
                    fmt: p.fmt.clone(),
                    link: p.link,
                    placeholder: false,
                    start: i,
                    end: i + 1,
                });
                i += 1;
                cur_start = i;
            } else {
                cur.push(c);
                i += 1;
            }
        }
        if !cur.is_empty() {
            toks.push(Tok {
                text: cur,
                fmt: p.fmt,
                link: p.link,
                placeholder: false,
                start: cur_start,
                end: i,
            });
        }
    }

    let mut pending_space: Option<Tok> = None;
    for tok in toks {
        if tok.text == "\n" {
            pending_space = None;
            push_line(&mut lines, &mut line);
            line_w = 0;
            continue;
        }
        let tw = tok.text.chars().count();
        if tok.text == " " && !tok.placeholder {
            // A space only counts once something precedes it on the line.
            if line_w > 0 {
                pending_space = Some(tok);
            }
            continue;
        }
        let space_w = if pending_space.is_some() { 1 } else { 0 };
        if line_w + space_w + tw > width && line_w > 0 {
            // Wrap before this token.
            push_line(&mut lines, &mut line);
            line_w = 0;
            pending_space = None;
        }
        if tw > width {
            // Hard-split an overlong token across lines.
            let mut rest = tok;
            loop {
                let avail = width - line_w;
                if avail == 0 {
                    push_line(&mut lines, &mut line);
                    line_w = 0;
                    continue;
                }
                if rest.text.chars().count() <= avail {
                    line_w += rest.text.chars().count();
                    line.push(DocSpan {
                        text: rest.text,
                        fmt: rest.fmt,
                        link: rest.link,
                        placeholder: rest.placeholder,
                        start: rest.start,
                        end: rest.end,
                    });
                    break;
                }
                let head: String = rest.text.chars().take(avail).collect();
                let head_len = head.chars().count();
                line.push(DocSpan {
                    text: head,
                    fmt: rest.fmt.clone(),
                    link: rest.link,
                    placeholder: rest.placeholder,
                    start: rest.start,
                    end: rest.start + head_len,
                });
                rest.start += head_len;
                rest.text = rest.text.chars().skip(head_len).collect();
                push_line(&mut lines, &mut line);
                line_w = 0;
            }
            pending_space = None;
            continue;
        }
        if let Some(sp) = pending_space.take() {
            line.push(DocSpan {
                text: sp.text,
                fmt: sp.fmt,
                link: sp.link,
                placeholder: false,
                start: sp.start,
                end: sp.end,
            });
            line_w += 1;
        }
        line_w += tw;
        line.push(DocSpan {
            text: tok.text,
            fmt: tok.fmt,
            link: tok.link,
            placeholder: tok.placeholder,
            start: tok.start,
            end: tok.end,
        });
    }
    push_line(&mut lines, &mut line);
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

/// List counters while laying out: numId → per-level current values.
type Counters = HashMap<u32, Vec<Option<i64>>>;

fn list_marker(doc: &Doc, p: &Paragraph, counters: &mut Counters) -> (String, usize) {
    let Some((ilvl, num_id)) = doc.eff_num(p) else {
        return (String::new(), 0);
    };
    let Some(abs_id) = doc.numbering.nums.get(&num_id) else {
        return (String::from("• "), ilvl);
    };
    let Some(levels) = doc.numbering.levels.get(abs_id) else {
        return (String::from("• "), ilvl);
    };
    let Some((fmt, text, start)) = levels.get(&ilvl) else {
        return (String::from("• "), ilvl);
    };
    if fmt == "bullet" || fmt == "none" {
        let ch = bullet_char(text);
        return (format!("{ch} "), ilvl);
    }
    // Numbered: advance this level's counter, reset deeper levels.
    let entry = counters.entry(num_id).or_default();
    if entry.len() <= ilvl {
        entry.resize(ilvl + 1, None);
    }
    for deeper in entry.iter_mut().skip(ilvl + 1) {
        *deeper = None;
    }
    let val = match entry[ilvl] {
        Some(v) => v + 1,
        None => *start,
    };
    entry[ilvl] = Some(val);
    let mut label = text.clone();
    for lvl in 0..=ilvl {
        let v = entry
            .get(lvl)
            .copied()
            .flatten()
            .unwrap_or_else(|| levels.get(&lvl).map(|(_, _, s)| *s).unwrap_or(1));
        let f = levels
            .get(&lvl)
            .map(|(f, _, _)| f.as_str())
            .unwrap_or("decimal");
        label = label.replace(&format!("%{}", lvl + 1), &format_number(v, f));
    }
    (format!("{label} "), ilvl)
}

fn bullet_char(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return String::from("•");
    }
    // Symbol/Wingdings private-use bullets → a plain bullet.
    if t.chars().count() == 1 {
        let c = t.chars().next().unwrap();
        if ('\u{F000}'..='\u{F0FF}').contains(&c) {
            return String::from("•");
        }
        return t.to_string();
    }
    t.to_string()
}

fn format_number(v: i64, fmt: &str) -> String {
    match fmt {
        "lowerLetter" => letter_num(v, false),
        "upperLetter" => letter_num(v, true),
        "lowerRoman" => roman_num(v).to_lowercase(),
        "upperRoman" => roman_num(v),
        "decimalZero" => format!("{v:02}"),
        _ => format!("{v}"),
    }
}

fn letter_num(mut v: i64, upper: bool) -> String {
    if v < 1 {
        v = 1;
    }
    let mut s = String::new();
    let mut n = v;
    while n > 0 {
        n -= 1;
        s.push((b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    let s: String = s.chars().rev().collect();
    if upper {
        s.to_uppercase()
    } else {
        s
    }
}

fn roman_num(v: i64) -> String {
    if !(1..4000).contains(&v) {
        return format!("{v}");
    }
    let table = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = v;
    let mut s = String::new();
    for (val, sym) in table {
        while n >= val {
            s.push_str(sym);
            n -= val;
        }
    }
    s
}

impl Doc {
    /// True when no layout has been computed yet.
    pub fn layout_is_empty(&self) -> bool {
        self.layout.is_empty()
    }

    /// Rebuild the layout cache when stale or the width changed.
    pub fn layout(&mut self, width: usize) -> &[DocLine] {
        if !self.layout_stale && self.layout_width == width {
            return &self.layout;
        }
        let width = width.max(8);
        let mut lines: Vec<DocLine> = Vec::new();
        let mut counters: Counters = HashMap::new();
        // Compute with immutable borrows into a fresh vec.
        let blocks = std::mem::take(&mut self.blocks);
        for (bi, block) in blocks.iter().enumerate() {
            match block {
                Block::Para(p) => {
                    let (marker, ilvl) = list_marker(self, p, &mut counters);
                    let indent = "  ".repeat(ilvl);
                    let prefix_w = indent.chars().count() + marker.chars().count();
                    let pieces = para_pieces(self, p);
                    let wrapped = wrap_pieces(pieces, width.saturating_sub(prefix_w).max(1));
                    let total = wrapped.len();
                    for (i, spans) in wrapped.into_iter().enumerate() {
                        let start = spans.first().map(|s| s.start).unwrap_or(0);
                        let end = spans.iter().map(|s| s.end).max().unwrap_or(start);
                        let prefix = if i == 0 {
                            format!("{indent}{marker}")
                        } else {
                            " ".repeat(prefix_w)
                        };
                        let _ = total;
                        lines.push(DocLine::Para {
                            block: bi,
                            cell: None,
                            para: 0,
                            start: if i == 0 { 0 } else { start },
                            end,
                            spans,
                            prefix,
                            align: self.eff_alignment(p),
                            heading: self.heading_level(p),
                        });
                    }
                }
                Block::Table(t) => {
                    layout_table(self, t, bi, width, &mut lines);
                }
                Block::Raw(_) => {}
            }
        }
        self.blocks = blocks;
        self.layout = lines;
        self.layout_width = width;
        self.layout_stale = false;
        &self.layout
    }

    /// (line index, display column) of the cursor in the current layout.
    pub fn cursor_position(&self) -> Option<(usize, usize)> {
        let cur = self.cursor;
        for (li, line) in self.layout.iter().enumerate() {
            match line {
                DocLine::Para {
                    block,
                    cell,
                    para,
                    start,
                    end,
                    spans,
                    prefix,
                    ..
                } => {
                    if *block == cur.block && *cell == cur.cell && *para == cur.para {
                        let is_last_line_of_para = !self.layout.get(li + 1).is_some_and(|n| {
                            matches!(n, DocLine::Para { block: b2, cell: c2, para: p2, .. }
                                    if *b2 == cur.block && *c2 == cur.cell && *p2 == cur.para)
                        });
                        if (cur.offset >= *start && cur.offset < *end)
                            || (cur.offset == *end && is_last_line_of_para)
                            || (*start == *end && cur.offset == *start)
                        {
                            let mut col = prefix.chars().count();
                            for s in spans {
                                if cur.offset >= s.end {
                                    col += s.text.chars().count();
                                } else if cur.offset > s.start {
                                    col += cur.offset - s.start;
                                } else {
                                    break;
                                }
                            }
                            return Some((li, col));
                        }
                    }
                }
                DocLine::TableSlice { block, row, cells } => {
                    if *block == cur.block {
                        if let Some((cr, cc)) = cur.cell {
                            if *row == cr {
                                if let Some(Some((pi, start, end, spans))) = cells.get(cc) {
                                    if *pi == cur.para && cur.offset >= *start && cur.offset <= *end
                                    {
                                        // The column reported for a table
                                        // is the offset within the cell
                                        // segment; the renderer recomputes
                                        // the exact screen column from the
                                        // column widths.
                                        let _ = spans;
                                        return Some((li, cur.offset - start));
                                    }
                                }
                            }
                        }
                    }
                }
                DocLine::Rule(_) => {}
            }
        }
        // Fallback: the offset fell on a character the wrap consumed
        // (a space at a wrap point or a line-break char). Show the
        // cursor at the end of the last line of this paragraph that
        // starts at or before the offset.
        let mut best: Option<(usize, usize)> = None;
        for (li, line) in self.layout.iter().enumerate() {
            if let DocLine::Para {
                block,
                cell,
                para,
                start,
                spans,
                prefix,
                ..
            } = line
            {
                if *block == cur.block
                    && *cell == cur.cell
                    && *para == cur.para
                    && *start <= cur.offset
                {
                    let width = prefix.chars().count()
                        + spans.iter().map(|s| s.text.chars().count()).sum::<usize>();
                    best = Some((li, width));
                }
            }
        }
        best
    }

    /// The (Loc, offset) at a layout line + within-line char column,
    /// for Up/Down movement.
    fn offset_at_visual(&self, line_idx: usize, col: usize) -> Option<(Loc, usize)> {
        match self.layout.get(line_idx)? {
            DocLine::Para {
                block,
                cell,
                para,
                start,
                end,
                spans,
                prefix,
                ..
            } => {
                let inner = col.saturating_sub(prefix.chars().count());
                let mut used = 0usize;
                let mut offset = *end;
                for s in spans {
                    let w = s.text.chars().count();
                    if inner < used + w {
                        offset = if s.placeholder {
                            s.start
                        } else {
                            (s.start + (inner - used)).min(s.end)
                        };
                        break;
                    }
                    used += w;
                }
                if spans.is_empty() {
                    offset = *start;
                }
                Some((
                    Loc {
                        block: *block,
                        cell: *cell,
                        para: *para,
                        offset: 0,
                    },
                    offset,
                ))
            }
            DocLine::TableSlice { block, row, cells } => {
                // Land in the same column as the cursor when possible.
                let cur_col = self.cursor.cell.map(|(_, c)| c).unwrap_or(0);
                for (ci, entry) in cells.iter().enumerate() {
                    if ci != cur_col {
                        continue;
                    }
                    if let Some((pi, start, end, _)) = entry {
                        let offset = (start + col).min(*end);
                        return Some((
                            Loc {
                                block: *block,
                                cell: Some((*row, ci)),
                                para: *pi,
                                offset: 0,
                            },
                            offset,
                        ));
                    }
                }
                // Column empty on this slice: use the first filled cell.
                for (ci, entry) in cells.iter().enumerate() {
                    if let Some((pi, start, _, _)) = entry {
                        return Some((
                            Loc {
                                block: *block,
                                cell: Some((*row, ci)),
                                para: *pi,
                                offset: 0,
                            },
                            *start,
                        ));
                    }
                }
                None
            }
            DocLine::Rule(_) => None,
        }
    }

    /// Mouse-wheel scrolling: move the view, keeping the cursor in
    /// the document (it follows when the view would leave it behind).
    pub fn scroll_by(&mut self, delta: isize) {
        let max_top = self.layout.len().saturating_sub(1);
        let new_top = (self.top as isize + delta).clamp(0, max_top as isize);
        self.top = new_top as usize;
    }

    /// Ensure the cursor line is within [top, top+view_h).
    pub fn ensure_visible(&mut self) {
        if let Some((li, _)) = self.cursor_position() {
            if li < self.top {
                self.top = li;
            } else if li >= self.top + self.view_h.max(1) {
                self.top = li + 1 - self.view_h.max(1);
            }
        }
    }
}

/// Column widths for a table at a total display width, distributed by
/// the table grid proportions (shared by layout and the renderer).
pub fn table_col_widths(t: &Table, width: usize) -> Vec<usize> {
    let ncols = t
        .grid
        .len()
        .max(t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0));
    if ncols == 0 {
        return Vec::new();
    }
    let total: u64 = t.grid.iter().map(|g| *g as u64).sum::<u64>().max(1);
    let avail = width.saturating_sub(ncols + 1).max(ncols * 4);
    let mut widths: Vec<usize> = Vec::new();
    let mut used = 0usize;
    for i in 0..ncols {
        let g = t.grid.get(i).copied().unwrap_or(0) as u64;
        let w = if i + 1 == ncols {
            avail.saturating_sub(used)
        } else if g == 0 {
            avail / ncols
        } else {
            (avail as u64 * g / total) as usize
        };
        let w = w.max(3);
        used += w;
        widths.push(w);
    }
    widths
}

/// Lay out one table into display lines (border rules + row slices).
fn layout_table(doc: &Doc, t: &Table, bi: usize, width: usize, lines: &mut Vec<DocLine>) {
    let ncols = t
        .grid
        .len()
        .max(t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0));
    if ncols == 0 {
        return;
    }
    let widths = table_col_widths(t, width);
    let border = |left: &str, mid: &str, right: &str| -> String {
        let mut s = String::from(left);
        for (i, w) in widths.iter().enumerate() {
            if i > 0 {
                s.push_str(mid);
            }
            s.push_str(&"─".repeat(*w));
        }
        s.push_str(right);
        s
    };
    lines.push(DocLine::Rule(border("┌", "┬", "┐")));
    for (ri, row) in t.rows.iter().enumerate() {
        if ri > 0 {
            lines.push(DocLine::Rule(border("├", "┼", "┤")));
        }
        // Wrap every cell's paragraphs at its column width.
        let mut cell_lines: Vec<Vec<(usize, usize, usize, Vec<DocSpan>)>> =
            Vec::with_capacity(ncols);
        let mut height = 1usize;
        for ci in 0..ncols {
            let mut mine: Vec<(usize, usize, usize, Vec<DocSpan>)> = Vec::new();
            if let Some(cell) = row.cells.get(ci) {
                for (pi, p) in cell.paras.iter().enumerate() {
                    let pieces = para_pieces(doc, p);
                    for spans in wrap_pieces(pieces, widths[ci].max(1)) {
                        let start = spans.first().map(|s| s.start).unwrap_or(0);
                        let end = spans.iter().map(|s| s.end).max().unwrap_or(start);
                        mine.push((pi, start, end, spans));
                    }
                }
            }
            if mine.is_empty() {
                mine.push((0, 0, 0, Vec::new()));
            }
            height = height.max(mine.len());
            cell_lines.push(mine);
        }
        for s in 0..height {
            let cells: Vec<Option<(usize, usize, usize, Vec<DocSpan>)>> =
                cell_lines.iter().map(|cl| cl.get(s).cloned()).collect();
            lines.push(DocLine::TableSlice {
                block: bi,
                row: ri,
                cells,
            });
        }
    }
    lines.push(DocLine::Rule(border("└", "┴", "┘")));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: &str = r#"<?xml version="1.0"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="32"/><w:color w:val="2E74B5"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="ListBullet"><w:name w:val="List Bullet"/><w:basedOn w:val="Normal"/><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr></w:style>
<w:style w:type="character" w:styleId="Emph"><w:name w:val="Emphasis"/><w:rPr><w:i/></w:rPr></w:style>
</w:styles>"#;

    const NUMBERING: &str = r#"<?xml version="1.0"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/></w:lvl></w:abstractNum>
<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;

    const BODY: &str = r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title Here</w:t></w:r></w:p>
<w:p><w:r><w:t>Plain </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold bit</w:t></w:r><w:r><w:t> tail</w:t></w:r></w:p>
<w:p><w:r><w:t>See </w:t></w:r><w:hyperlink r:id="rId5"><w:r><w:rPr><w:color w:val="0563C1"/></w:rPr><w:t>link text</w:t></w:r></w:hyperlink></w:p>
<w:p><w:pPr><w:pStyle w:val="ListBullet"/></w:pPr><w:r><w:t>an item</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>numbered</w:t></w:r></w:p>
<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>A2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:ins w:id="1"><w:r><w:t>inserted text</w:t></w:r></w:ins></w:p>"#;

    fn make_docx(body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fex-doc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "t{}.docx",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let doc_xml = format!(
            "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}</w:body></w:document>"
        );
        let mut out = Vec::new();
        {
            let mut z = zip::ZipWriter::new(Cursor::new(&mut out));
            let opts = zip::write::SimpleFileOptions::default();
            for (name, data) in [
                ("word/document.xml", doc_xml.as_str()),
                ("word/styles.xml", STYLES),
                ("word/numbering.xml", NUMBERING),
                ("word/media/image1.png", "PNGBYTES"),
            ] {
                z.start_file(name, opts).unwrap();
                z.write_all(data.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        std::fs::write(&path, out).unwrap();
        path
    }

    fn saved_part(path: &Path, name: &str) -> String {
        let data = std::fs::read(path).unwrap();
        let mut zin = zip::ZipArchive::new(Cursor::new(data)).unwrap();
        let mut s = String::new();
        zin.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    }

    fn open(body: &str) -> Doc {
        open_doc(&make_docx(body)).unwrap()
    }

    fn para_texts(d: &Doc) -> Vec<String> {
        d.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Para(p) => Some(p.text()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parses_paragraphs_runs_and_styles() {
        let d = open(BODY);
        let texts = para_texts(&d);
        assert_eq!(
            texts,
            vec![
                "Title Here",
                "Plain bold bit tail",
                "See link text",
                "an item",
                "numbered",
                "inserted text"
            ]
        );
        // Heading resolves bold + size + color through the style cascade.
        let Block::Para(h) = &d.blocks[0] else {
            panic!("block 0 not a paragraph")
        };
        assert_eq!(d.heading_level(h), Some(1));
        let Item::Run(r) = &h.items[0] else {
            panic!("heading has no run")
        };
        let fmt = d.eff_fmt(h, r);
        assert!(fmt.bold);
        assert_eq!(fmt.size_pt, Some(16.0));
        assert_eq!(fmt.color, Some((0x2E, 0x74, 0xB5)));
        assert_eq!(fmt.font.as_deref(), Some("Calibri")); // docDefaults
                                                          // Direct bold run.
        let Block::Para(p2) = &d.blocks[1] else {
            panic!("block 1")
        };
        let Item::Run(rb) = &p2.items[1] else {
            panic!("bold run")
        };
        assert!(d.eff_fmt(p2, rb).bold);
        assert!(!d.eff_fmt(p2, rb).italic);
        // The tracked-changes paragraph is locked but displays its text.
        let Block::Para(last) = &d.blocks[6] else {
            panic!("block 7")
        };
        assert!(last.locked);
    }

    #[test]
    fn parses_table_cells() {
        let d = open(BODY);
        let Block::Table(t) = &d.blocks[5] else {
            panic!("block 5 not a table")
        };
        assert_eq!(t.grid, vec![2000, 2000]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0].cells[1].paras[0].text(), "B1");
        assert_eq!(t.rows[1].cells[0].paras[0].text(), "A2");
    }

    #[test]
    fn layout_shows_list_markers_and_table() {
        let mut d = open(BODY);
        let lines = d.layout(60).to_vec();
        let para_lines: Vec<&DocLine> = lines
            .iter()
            .filter(|l| matches!(l, DocLine::Para { .. }))
            .collect();
        // Bullet paragraph gets a bullet prefix; numbered gets "1.".
        let bullet = para_lines
            .iter()
            .find(|l| matches!(l, DocLine::Para { block: 3, .. }))
            .unwrap();
        let DocLine::Para { prefix, .. } = bullet else {
            unreachable!()
        };
        assert!(prefix.contains('•'), "prefix was {prefix:?}");
        let numbered = para_lines
            .iter()
            .find(|l| matches!(l, DocLine::Para { block: 4, .. }))
            .unwrap();
        let DocLine::Para { prefix, .. } = numbered else {
            unreachable!()
        };
        assert!(prefix.contains("1."), "prefix was {prefix:?}");
        // Table renders border rules and slices.
        assert!(lines.iter().any(|l| matches!(l, DocLine::Rule(_))));
        assert!(lines
            .iter()
            .any(|l| matches!(l, DocLine::TableSlice { .. })));
    }

    #[test]
    fn edit_and_save_preserves_everything_else() {
        let path = make_docx(BODY);
        let mut d = open_doc(&path).unwrap();
        // Type into the second paragraph at its start.
        d.cursor = Loc {
            block: 1,
            cell: None,
            para: 0,
            offset: 0,
        };
        d.insert_char('X');
        assert_eq!(para_texts(&d)[1], "XPlain bold bit tail");
        d.save().unwrap();
        assert!(!d.dirty);
        let xml = saved_part(&path, "word/document.xml");
        assert!(xml.contains("XPlain"), "edit missing: {xml}");
        assert!(xml.contains("<w:b/>"), "bold run lost: {xml}");
        assert!(xml.contains("Title Here"));
        assert!(xml.contains("<w:hyperlink r:id=\"rId5\">"), "link lost");
        assert!(xml.contains("inserted text"), "locked para changed");
        // Untouched parts are byte-identical.
        assert_eq!(saved_part(&path, "word/styles.xml"), STYLES);
        assert_eq!(saved_part(&path, "word/media/image1.png"), "PNGBYTES");
        // A fresh open sees the edit.
        let d2 = open_doc(&path).unwrap();
        assert_eq!(para_texts(&d2)[1], "XPlain bold bit tail");
    }

    #[test]
    fn split_and_merge_paragraphs() {
        let mut d = open(BODY);
        d.cursor = Loc {
            block: 1,
            cell: None,
            para: 0,
            offset: 6, // after "Plain "
        };
        d.split_para();
        let texts = para_texts(&d);
        assert_eq!(texts[1], "Plain ");
        assert_eq!(texts[2], "bold bit tail");
        assert_eq!(d.cursor.block, 2);
        // Backspace at the start merges them back.
        d.backspace();
        let texts = para_texts(&d);
        assert_eq!(texts[1], "Plain bold bit tail");
        assert_eq!(d.cursor.offset, 6);
    }

    #[test]
    fn toggle_bold_on_word() {
        let mut d = open(BODY);
        // Cursor inside "bold" (offset 6..10 of para 1 text): it's
        // already bold via direct rPr, so toggling turns it off with an
        // explicit w:val="false".
        d.cursor = Loc {
            block: 1,
            cell: None,
            para: 0,
            offset: 7,
        };
        d.toggle_fmt("b");
        let Block::Para(p) = &d.blocks[1] else {
            panic!()
        };
        let Item::Run(rb) = &p.items[1] else { panic!() };
        assert!(!d.eff_fmt(p, rb).bold, "bold should be off now");
        assert!(
            rb.rpr
                .iter()
                .any(|(_, raw)| raw.contains("w:val=\"false\"")),
            "expected explicit false in rPr: {:?}",
            rb.rpr
        );
        // Toggling a plain word turns bold on.
        d.cursor = Loc {
            block: 1,
            cell: None,
            para: 0,
            offset: 1,
        };
        d.toggle_fmt("b");
        let Block::Para(p) = &d.blocks[1] else {
            panic!()
        };
        let Item::Run(r0) = &p.items[0] else { panic!() };
        assert!(d.eff_fmt(p, r0).bold);
    }

    #[test]
    fn set_alignment_writes_jc() {
        let mut d = open(BODY);
        d.cursor = Loc {
            block: 1,
            cell: None,
            para: 0,
            offset: 0,
        };
        d.set_alignment("center");
        let Block::Para(p) = &d.blocks[1] else {
            panic!()
        };
        assert_eq!(p.alignment().as_deref(), Some("center"));
        assert_eq!(d.eff_alignment(p), "center");
        d.set_alignment("left");
        let Block::Para(p) = &d.blocks[1] else {
            panic!()
        };
        assert_eq!(p.alignment(), None);
    }

    #[test]
    fn locked_paragraph_refuses_edits() {
        let mut d = open(BODY);
        d.cursor = Loc {
            block: 6,
            cell: None,
            para: 0,
            offset: 0,
        };
        d.insert_char('Z');
        assert_eq!(para_texts(&d)[5], "inserted text");
        assert!(d.message.contains("tracked changes"));
        assert!(!d.dirty);
    }

    #[test]
    fn edit_inside_table_cell() {
        let path = make_docx(BODY);
        let mut d = open_doc(&path).unwrap();
        d.cursor = Loc {
            block: 5,
            cell: Some((0, 1)),
            para: 0,
            offset: 2,
        };
        d.insert_char('!');
        d.save().unwrap();
        let xml = saved_part(&path, "word/document.xml");
        assert!(xml.contains("B1!"), "cell edit missing: {xml}");
        assert!(xml.contains("<w:tblGrid>"), "grid lost");
    }

    #[test]
    fn tab_moves_between_cells() {
        let mut d = open(BODY);
        d.cursor = Loc {
            block: 5,
            cell: Some((0, 0)),
            para: 0,
            offset: 0,
        };
        d.tab_cell(1);
        assert_eq!(d.cursor.cell, Some((0, 1)));
        d.tab_cell(1);
        assert_eq!(d.cursor.cell, Some((1, 0)));
        d.tab_cell(-1);
        assert_eq!(d.cursor.cell, Some((0, 1)));
    }
}
