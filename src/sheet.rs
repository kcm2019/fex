//! CSV and Excel table viewer/editor.
//!
//! Opened with `e` on a `.csv` or `.xlsx` file. Arrow keys move between
//! cells, Enter edits the current cell, Ctrl+S saves, `a` adds a row, `A`
//! adds a column, Esc closes (asking about unsaved changes). CSV files
//! treat the first row as the header; xlsx sheets use column letters
//! (A, B, C, ...) as headers and show every row. In xlsx files `[` and `]`
//! switch between sheets.

use std::io;
use std::path::{Path, PathBuf};

use crate::editor::FindBar;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use chrono::NaiveDateTime;
use umya_spreadsheet::helper::date::excel_to_date_time_chrono;
use umya_spreadsheet::structs::Cell;
use umya_spreadsheet::{reader, writer, Workbook};

/// Longest cell content kept per column when laying out the table.
const MAX_COL_WIDTH: usize = 28;

/// Parse one CSV or xlsx file into a Sheet.
pub fn open_sheet(path: &Path) -> io::Result<Sheet> {
    let is_xlsx = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"));
    if is_xlsx {
        return open_xlsx(path);
    }
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let mut records: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        records.push(rec.iter().map(|s| s.to_string()).collect());
    }
    let headers = records.first().cloned().unwrap_or_default();
    let ncols = headers.len();
    let mut rows: Vec<Vec<String>> = records.into_iter().skip(1).collect();
    for row in &mut rows {
        row.resize(ncols, String::new());
    }
    if rows.is_empty() && ncols > 0 {
        rows.push(vec![String::new(); ncols]);
    }
    Ok(Sheet {
        path: path.to_path_buf(),
        headers,
        rows,
        row: 0,
        col: 0,
        off_row: 0,
        off_col: 0,
        view_h: 20,
        view_w: 80,
        dirty: false,
        message: String::new(),
        confirm_discard: false,
        view_x: 0,
        view_y: 0,
        kind: SheetKind::Csv,
        find: None,
        sel_anchor: None,
    })
}

/// What backs the grid: a plain CSV file or an xlsx workbook.
pub enum SheetKind {
    Csv,
    Xlsx(XlsxState),
}

/// An open xlsx workbook plus per-sheet view state. The `headers`/`rows`
/// fields on `Sheet` always mirror the active sheet's grid; `formulas`
/// flags the cells holding formulas parallel to `rows`.
pub struct XlsxState {
    book: Workbook,
    sheet_idx: usize,
    names: Vec<String>,
    formulas: Vec<Vec<bool>>,
}

/// Open an xlsx workbook: every sheet becomes switchable with `[`/`]`.
fn open_xlsx(path: &Path) -> io::Result<Sheet> {
    let book = reader::xlsx::read(path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Cannot read xlsx: {e}")))?;
    let names: Vec<String> = book
        .sheet_collection()
        .iter()
        .map(|s| s.name().to_string())
        .collect();
    if names.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Workbook has no sheets",
        ));
    }
    let mut sh = Sheet {
        path: path.to_path_buf(),
        headers: Vec::new(),
        rows: Vec::new(),
        row: 0,
        col: 0,
        off_row: 0,
        off_col: 0,
        view_h: 20,
        view_w: 80,
        dirty: false,
        message: String::new(),
        confirm_discard: false,
        view_x: 0,
        view_y: 0,
        kind: SheetKind::Xlsx(XlsxState {
            book,
            sheet_idx: 0,
            names,
            formulas: Vec::new(),
        }),
        find: None,
        sel_anchor: None,
    };
    sh.load_xlsx_sheet(0);
    Ok(sh)
}

/// 1-based column number to Excel letters: 1 -> A, 27 -> AA.
fn col_letter(mut n: usize) -> String {
    let mut s = Vec::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.push((b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s.iter().rev().collect()
}

/// Built-in Excel date/time number-format ids.
fn is_builtin_date_format(id: u32) -> bool {
    matches!(id, 14..=22 | 27..=36 | 45..=47 | 50 | 57)
}

/// Does this cell's number format look like a date/time format?
fn is_date_cell(cell: &Cell) -> bool {
    let Some(fmt) = cell.style().number_format() else {
        return false;
    };
    if is_builtin_date_format(fmt.number_format_id()) {
        return true;
    }
    // Custom format code: strip [conditions] and "quoted" literals, then
    // look for date/time tokens. `m` alone counts as a month token.
    let code = fmt.format_code();
    let mut stripped = String::with_capacity(code.len());
    let mut bracket = false;
    let mut quote = false;
    for c in code.chars() {
        match c {
            '[' => bracket = true,
            ']' => bracket = false,
            '"' => quote = !quote,
            '\\' => {}
            _ if !bracket && !quote => stripped.push(c),
            _ => {}
        }
    }
    let s = stripped.to_lowercase();
    s.contains('y') || s.contains('d') || s.contains('h') || s.contains('s') || s.contains('m')
}

/// Excel serial date number to a display string.
fn format_excel_date(serial: f64) -> String {
    let dt = excel_to_date_time_chrono(serial);
    let s = dt.format("%Y-%m-%d %H:%M").to_string();
    if s.ends_with(" 00:00") {
        s[..10].to_string()
    } else {
        s
    }
}

/// Parse what the user typed back into an Excel serial date number.
/// Accepts `YYYY-MM-DD` and `YYYY-MM-DD HH:MM`.
fn parse_excel_date(text: &str) -> Option<f64> {
    let t = text.trim();
    let dt = NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(&format!("{t} 00:00"), "%Y-%m-%d %H:%M"))
        .ok()?;
    // Mirror of excel_to_date_time_chrono: serial 1 = 1900-01-01, with the
    // 1900 leap-year quirk folded into the 1899-12-30 base for serials >= 61.
    let base = NaiveDateTime::parse_from_str("1899-12-30 00:00:00", "%Y-%m-%d %H:%M:%S").ok()?;
    let dur = dt - base;
    let serial = dur.num_days() as f64 + (dur.num_seconds() % 86_400) as f64 / 86_400.0;
    (serial >= 61.0).then_some(serial)
}

/// Display string and formula flag for one xlsx cell. Formula cells show
/// their cached value, or the formula itself when there is none.
fn xlsx_cell_display(cell: &Cell) -> (String, bool) {
    let formula = cell.formula();
    if !formula.is_empty() {
        let v = cell.value().to_string();
        if v.is_empty() {
            (format!("={formula}"), true)
        } else {
            (v, true)
        }
    } else if is_date_cell(cell) {
        if let Some(n) = cell.value_number() {
            (format_excel_date(n), false)
        } else {
            (cell.value().to_string(), false)
        }
    } else {
        (cell.value().to_string(), false)
    }
}

pub struct Sheet {
    pub path: PathBuf,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row: usize, // selected data row
    pub col: usize, // selected column
    pub off_row: usize,
    pub off_col: usize,
    pub view_h: usize, // visible data rows
    pub view_w: usize, // visible width in cells
    pub dirty: bool,
    pub message: String,
    pub confirm_discard: bool,
    /// Top-left of the rendered table, for mapping mouse clicks.
    pub view_x: u16,
    pub view_y: u16,
    pub kind: SheetKind,
    /// Find bar opened with Ctrl+F (`None` when closed).
    pub find: Option<FindBar>,
    /// Block-selection anchor for Shift+arrow selection (`None` when the
    /// cursor alone is selected). The block spans the anchor and the
    /// cursor; any plain cursor move clears it.
    pub sel_anchor: Option<(usize, usize)>,
}

impl Sheet {
    pub fn ncols(&self) -> usize {
        self.headers.len()
    }

    pub fn save(&mut self) -> io::Result<()> {
        match &mut self.kind {
            SheetKind::Csv => {
                let mut wtr = csv::WriterBuilder::new()
                    .from_path(&self.path)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                wtr.write_record(&self.headers)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                for row in &self.rows {
                    wtr.write_record(row)
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                }
                wtr.flush()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            }
            SheetKind::Xlsx(st) => {
                writer::xlsx::write(&st.book, &self.path).map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("Cannot write xlsx: {e}"))
                })?;
            }
        }
        self.dirty = false;
        self.message = format!("Saved {}", self.path.display());
        Ok(())
    }

    pub fn set_cell(&mut self, value: String) {
        let (row, col) = (self.row, self.col);
        self.set_cell_at(row, col, value);
    }

    /// Write one cell by coordinates, for paste. Grows the grid when the
    /// coordinates fall outside it (CSV extends headers/rows; xlsx grows
    /// through the workbook and reloads the display cache).
    pub fn set_cell_at(&mut self, row: usize, col: usize, value: String) {
        if self.headers.is_empty() {
            return;
        }
        if matches!(self.kind, SheetKind::Csv) {
            self.grow_to(row, col);
            self.rows[row][col] = value;
            self.dirty = true;
            return;
        }
        // xlsx: write through the workbook, then rebuild the display
        // cache when the write grew the sheet.
        let grew = row >= self.rows.len() || col >= self.ncols();
        let written = self.xlsx_write_cell(row, col, &value);
        let Some((display, is_formula)) = written else {
            return;
        };
        if grew {
            let sheet_idx = match &self.kind {
                SheetKind::Xlsx(st) => st.sheet_idx,
                SheetKind::Csv => unreachable!(),
            };
            let (save_row, save_col) = (self.row, self.col);
            self.load_xlsx_sheet(sheet_idx);
            self.row = save_row.min(self.rows.len().saturating_sub(1));
            self.col = save_col.min(self.ncols().saturating_sub(1));
        }
        if row < self.rows.len() && col < self.rows[row].len() {
            self.rows[row][col] = display;
            if let SheetKind::Xlsx(st) = &mut self.kind {
                if let Some(flags) = st.formulas.get_mut(row) {
                    if let Some(f) = flags.get_mut(col) {
                        *f = is_formula;
                    }
                }
            }
        }
        self.dirty = true;
    }

    /// Grow a CSV grid so (row, col) exists, extending headers and rows
    /// with blanks. Rows stay rectangular.
    fn grow_to(&mut self, row: usize, col: usize) {
        let ncols = self.ncols().max(col + 1);
        while self.headers.len() < ncols {
            self.headers.push(String::new());
        }
        for r in self.rows.iter_mut() {
            while r.len() < ncols {
                r.push(String::new());
            }
        }
        while self.rows.len() <= row {
            self.rows.push(vec![String::new(); ncols]);
        }
    }

    /// Write one xlsx cell through the workbook only (no display-cache
    /// reload; the caller reloads once after a batch). Returns the display
    /// text and whether the value is a formula.
    fn xlsx_write_cell(&mut self, row: usize, col: usize, value: &str) -> Option<(String, bool)> {
        let coord = ((col + 1) as u32, (row + 1) as u32);
        let SheetKind::Xlsx(st) = &mut self.kind else {
            return None;
        };
        let Ok(ws) = st.book.sheet_mut(st.sheet_idx) else {
            return None;
        };
        let cell = ws.cell_mut(coord);
        if let Some(f) = value.strip_prefix('=') {
            // Editing the formula itself; blank the cached
            // result so a stale value is never shown for the new
            // formula. fex has no calc engine: Excel
            // recalculates on open.
            cell.set_formula(f);
            cell.set_formula_result_blank();
            Some((value.to_string(), true))
        } else if is_date_cell(cell) && parse_excel_date(value).is_some() {
            let serial = parse_excel_date(value).unwrap_or(0.0);
            cell.set_value_number(serial);
            Some((value.to_string(), false))
        } else {
            // Smart typing: numbers/bools/empty become native types.
            cell.set_value(value.to_string());
            Some((cell.value().to_string(), false))
        }
    }

    /// Text to pre-fill the cell editor: `=formula` for formula cells so
    /// the user can edit the formula itself, the display text otherwise.
    pub fn edit_text(&self) -> Option<String> {
        match &self.kind {
            SheetKind::Csv => self.rows.get(self.row)?.get(self.col).cloned(),
            SheetKind::Xlsx(st) => {
                let is_formula = st
                    .formulas
                    .get(self.row)?
                    .get(self.col)
                    .copied()
                    .unwrap_or(false);
                if is_formula {
                    let ws = st.book.sheet(st.sheet_idx).ok()?;
                    let cell = ws.cell(((self.col + 1) as u32, (self.row + 1) as u32))?;
                    Some(format!("={}", cell.formula()))
                } else {
                    self.rows.get(self.row)?.get(self.col).cloned()
                }
            }
        }
    }

    /// Rebuild the grid from the active xlsx sheet: column letters become
    /// the headers and every Excel row becomes a data row.
    fn load_xlsx_sheet(&mut self, idx: usize) {
        let SheetKind::Xlsx(st) = &mut self.kind else {
            return;
        };
        st.sheet_idx = idx;
        let Ok(ws) = st.book.sheet(idx) else {
            return;
        };
        let (hc, hr) = ws.highest_column_and_row();
        let ncols = (hc as usize).max(1);
        let nrows = (hr as usize).max(1);
        self.headers = (1..=ncols).map(col_letter).collect();
        self.rows = Vec::with_capacity(nrows);
        st.formulas = Vec::with_capacity(nrows);
        for r in 1..=(nrows as u32) {
            let mut row = Vec::with_capacity(ncols);
            let mut flags = Vec::with_capacity(ncols);
            for c in 1..=(ncols as u32) {
                match ws.cell((c, r)) {
                    Some(cell) => {
                        let (text, is_formula) = xlsx_cell_display(cell);
                        row.push(text);
                        flags.push(is_formula);
                    }
                    None => {
                        row.push(String::new());
                        flags.push(false);
                    }
                }
            }
            self.rows.push(row);
            st.formulas.push(flags);
        }
        self.row = 0;
        self.col = 0;
        self.off_row = 0;
        self.off_col = 0;
    }

    /// Switch to the next/previous sheet (`delta` = +1/-1), wrapping.
    /// No-op for CSV files.
    pub fn switch_sheet(&mut self, delta: isize) {
        let (idx, name) = match &self.kind {
            SheetKind::Xlsx(st) => {
                let n = st.names.len();
                if n < 2 {
                    return;
                }
                let idx = (st.sheet_idx as isize + delta).rem_euclid(n as isize) as usize;
                (idx, st.names[idx].clone())
            }
            SheetKind::Csv => return,
        };
        self.load_xlsx_sheet(idx);
        self.message = format!("Sheet {name}");
    }

    pub fn is_xlsx(&self) -> bool {
        matches!(self.kind, SheetKind::Xlsx(_))
    }

    /// (names, active index) for the sheet tab strip; None for CSV.
    pub fn sheet_tabs(&self) -> Option<(&[String], usize)> {
        match &self.kind {
            SheetKind::Xlsx(st) => Some((&st.names, st.sheet_idx)),
            SheetKind::Csv => None,
        }
    }

    /// Insert an empty row below the current one and select it.
    pub fn add_row(&mut self) {
        if self.is_xlsx() {
            let (name, at, want_rows) = match &self.kind {
                SheetKind::Xlsx(st) => (
                    st.names[st.sheet_idx].clone(),
                    self.row + 1,
                    // 0-based index of the new row, and the row count the
                    // grid should have afterwards.
                    self.rows.len() + 1,
                ),
                SheetKind::Csv => unreachable!(),
            };
            // Excel rows are 1-based: insert below the current grid row.
            let idx = match &mut self.kind {
                SheetKind::Xlsx(st) => {
                    st.book.insert_new_row(&name, (at + 1) as u32, 1);
                    // insert_new_row only shifts existing cells: on a sparse
                    // or empty sheet the dimension doesn't grow, so pin a
                    // cell on the new last row.
                    if let Ok(ws) = st.book.sheet_mut(st.sheet_idx) {
                        let _ = ws.cell_mut((1u32, want_rows as u32));
                    }
                    st.sheet_idx
                }
                SheetKind::Csv => unreachable!(),
            };
            self.load_xlsx_sheet(idx);
            self.row = at.min(self.rows.len().saturating_sub(1));
            self.dirty = true;
            self.ensure_visible();
            self.message = String::from("Added row");
            return;
        }
        let ncols = self.ncols().max(1);
        if self.headers.is_empty() {
            self.headers = vec![String::new(); ncols];
        }
        let at = (self.row + 1).min(self.rows.len());
        self.rows.insert(at, vec![String::new(); ncols]);
        self.row = at;
        self.dirty = true;
        self.ensure_visible();
        self.message = String::from("Added row");
    }

    /// Insert a column right after the current one with the given header
    /// name, and select it. Rows stay rectangular. For xlsx the name is
    /// ignored: headers are always Excel column letters.
    pub fn add_column(&mut self, name: String) {
        if self.is_xlsx() {
            let (sheet_name, want_cols) = match &self.kind {
                SheetKind::Xlsx(st) => (
                    st.names[st.sheet_idx].clone(),
                    // The column count the grid should have afterwards.
                    self.ncols() + 1,
                ),
                SheetKind::Csv => unreachable!(),
            };
            // Insert before the 1-based index right after the current
            // column, i.e. directly after it.
            let idx = match &mut self.kind {
                SheetKind::Xlsx(st) => {
                    st.book
                        .insert_new_column_by_index(&sheet_name, (self.col + 2) as u32, 1);
                    // insert_new_column_by_index only shifts existing cells:
                    // on a sparse or empty sheet the dimension doesn't grow,
                    // so pin a cell on the new last column.
                    if let Ok(ws) = st.book.sheet_mut(st.sheet_idx) {
                        let _ = ws.cell_mut((want_cols as u32, 1u32));
                    }
                    st.sheet_idx
                }
                SheetKind::Csv => unreachable!(),
            };
            self.load_xlsx_sheet(idx);
            self.col = (self.col + 1).min(self.ncols().saturating_sub(1));
            self.dirty = true;
            self.ensure_visible();
            self.message = String::from("Added column");
            return;
        }
        let at = (self.col + 1).min(self.ncols());
        let ncols = self.ncols();
        self.headers.insert(at, name.clone());
        for row in &mut self.rows {
            row.resize(ncols, String::new());
            row.insert(at, String::new());
        }
        self.col = at;
        self.dirty = true;
        self.ensure_visible();
        self.message = format!("Added column {name}");
    }

    pub fn move_cell(&mut self, dr: isize, dc: isize) {
        self.sel_anchor = None;
        self.move_cursor(dr, dc);
    }

    /// Shift+arrow block selection: pin the anchor at the cursor, then
    /// move. The selected block spans the anchor and the new cursor.
    pub fn extend_selection(&mut self, dr: isize, dc: isize) {
        if self.rows.is_empty() || self.headers.is_empty() {
            return;
        }
        if self.sel_anchor.is_none() {
            self.sel_anchor = Some((self.row, self.col));
        }
        self.move_cursor(dr, dc);
    }

    /// Paste TSV from the system clipboard (e.g. cells copied in
    /// Excel/Sheets: tabs between columns, newlines between rows),
    /// filling the grid from the cursor cell right and down. Grows the
    /// grid when the block overflows it.
    pub fn paste_tsv(&mut self) {
        if self.headers.is_empty() {
            return;
        }
        let text = match crate::fs::read_clipboard() {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                self.message = String::from("Clipboard is empty");
                return;
            }
        };
        self.paste_text(&text);
    }

    /// Apply TSV text as a cell block at the cursor (see `paste_tsv`).
    pub fn paste_text(&mut self, text: &str) {
        if text.trim().is_empty() || self.headers.is_empty() {
            self.message = String::from("Clipboard is empty");
            return;
        }
        let block: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').collect()).collect();
        let nrows = block.len();
        let ncols = block.iter().map(|r| r.len()).max().unwrap_or(0);
        if nrows == 0 || ncols == 0 {
            self.message = String::from("Clipboard is empty");
            return;
        }
        let (r0, c0) = (self.row, self.col);
        if matches!(self.kind, SheetKind::Csv) {
            self.grow_to(r0 + nrows - 1, c0 + ncols - 1);
            for (dr, line) in block.iter().enumerate() {
                for (dc, cell) in line.iter().enumerate() {
                    self.rows[r0 + dr][c0 + dc] = (*cell).to_string();
                }
            }
        } else {
            for (dr, line) in block.iter().enumerate() {
                for (dc, cell) in line.iter().enumerate() {
                    // Workbook-only writes; the display cache rebuilds once
                    // below. Formulas flags refresh with the reload.
                    let _ = self.xlsx_write_cell(r0 + dr, c0 + dc, cell);
                }
            }
            let sheet_idx = match &self.kind {
                SheetKind::Xlsx(st) => st.sheet_idx,
                SheetKind::Csv => unreachable!(),
            };
            self.load_xlsx_sheet(sheet_idx);
            self.row = r0.min(self.rows.len().saturating_sub(1));
            self.col = c0.min(self.ncols().saturating_sub(1));
        }
        self.sel_anchor = None;
        self.dirty = true;
        self.ensure_visible();
        self.message = if nrows == 1 && ncols == 1 {
            String::from("Pasted cell")
        } else {
            format!("Pasted {nrows}×{ncols} cells")
        };
    }

    /// The selected block as ((r0, c0), (r1, c1)) with r0 <= r1, c0 <= c1,
    /// or `None` when only the cursor cell is selected.
    pub fn selection_rect(&self) -> Option<((usize, usize), (usize, usize))> {
        let (ar, ac) = self.sel_anchor?;
        Some((
            (ar.min(self.row), ac.min(self.col)),
            (ar.max(self.row), ac.max(self.col)),
        ))
    }

    /// TSV of the selected block (or the cursor cell when nothing is
    /// selected): tabs between columns, newlines between rows.
    pub fn selection_tsv(&self) -> Option<String> {
        if self.rows.is_empty() || self.headers.is_empty() {
            return None;
        }
        let ((r0, c0), (r1, c1)) = self
            .selection_rect()
            .unwrap_or(((self.row, self.col), (self.row, self.col)));
        let mut tsv = String::new();
        for r in r0..=r1 {
            for c in c0..=c1 {
                if c > c0 {
                    tsv.push('\t');
                }
                if let Some(cell) = self.rows.get(r).and_then(|row| row.get(c)) {
                    tsv.push_str(cell);
                }
            }
            tsv.push('\n');
        }
        Some(tsv)
    }

    /// Copy the selected block (or the cursor cell when nothing is
    /// selected) to the system clipboard as TSV, so it pastes straight
    /// into Excel/Sheets as cells.
    pub fn copy_block(&mut self) {
        match self.selection_tsv() {
            Some(tsv) if crate::fs::copy_to_clipboard(&tsv) => {
                let ((r0, c0), (r1, c1)) = self
                    .selection_rect()
                    .unwrap_or(((self.row, self.col), (self.row, self.col)));
                let n = r1 - r0 + 1;
                let m = c1 - c0 + 1;
                self.message = if n == 1 && m == 1 {
                    String::from("Copied cell")
                } else {
                    format!("Copied {n}×{m} cells")
                };
            }
            _ => {
                self.message = String::from("Clipboard unavailable");
            }
        }
    }

    fn move_cursor(&mut self, dr: isize, dc: isize) {
        if self.rows.is_empty() || self.headers.is_empty() {
            return;
        }
        let r = (self.row as isize + dr).clamp(0, self.rows.len() as isize - 1) as usize;
        let c = (self.col as isize + dc).clamp(0, self.ncols() as isize - 1) as usize;
        self.row = r;
        self.col = c;
        self.ensure_visible();
    }

    /// Tab to the next cell (wraps to the next row).
    pub fn next_cell(&mut self) {
        if self.rows.is_empty() || self.headers.is_empty() {
            return;
        }
        self.sel_anchor = None;
        if self.col + 1 < self.ncols() {
            self.col += 1;
        } else if self.row + 1 < self.rows.len() {
            self.row += 1;
            self.col = 0;
        }
        self.ensure_visible();
    }

    // -- find (Ctrl+F) ------------------------------------------------------

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
        }
    }

    pub fn close_find(&mut self) {
        self.find = None;
    }

    /// All cells whose display text contains the query (case-insensitive),
    /// as (row, col) in row-major order.
    pub fn find_matches(&self) -> Vec<(usize, usize)> {
        let Some(find) = &self.find else {
            return Vec::new();
        };
        let q: Vec<char> = find.query.to_lowercase().chars().collect();
        if q.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let lc: Vec<char> = cell.to_lowercase().chars().collect();
                if lc.len() >= q.len()
                    && (0..=lc.len() - q.len()).any(|i| lc[i..i + q.len()] == q[..])
                {
                    out.push((r, c));
                }
            }
        }
        out
    }

    /// Jump to the next (`dir` > 0) or previous (`dir` < 0) matching cell,
    /// wrapping around the grid. The cell cursor marks the match.
    pub fn find_jump(&mut self, dir: i8) {
        let matches = self.find_matches();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if matches.is_empty() {
            find.not_found = true;
            return;
        }
        find.not_found = false;
        let cur = (self.row, self.col);
        let target = if dir < 0 {
            let mut t = matches[matches.len() - 1];
            for &m in matches.iter().rev() {
                if m < cur {
                    t = m;
                    break;
                }
            }
            t
        } else {
            // While typing we land on the first match at/after the cursor;
            // Enter must advance strictly past it, so this is always strict.
            let mut t = matches[0];
            for &m in &matches {
                if m > cur {
                    t = m;
                    break;
                }
            }
            t
        };
        self.row = target.0;
        self.col = target.1.min(self.ncols().saturating_sub(1));
        self.ensure_visible();
    }

    /// Re-search from the cell cursor after the query changed: jump to the
    /// first match at/after the cursor, wrapping around the grid.
    fn find_research(&mut self) {
        let matches = self.find_matches();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if find.query.is_empty() {
            find.not_found = false;
            return;
        }
        if matches.is_empty() {
            find.not_found = true;
            return;
        }
        find.not_found = false;
        let cur = (self.row, self.col);
        let mut target = matches[0];
        for &m in &matches {
            if m >= cur {
                target = m;
                break;
            }
        }
        self.row = target.0;
        self.col = target.1.min(self.ncols().saturating_sub(1));
        self.ensure_visible();
    }

    /// Edit the query with one key while the find bar is open, then
    /// re-search. Returns true when the key was consumed (the bar eats
    /// every key so none reach the grid).
    pub fn find_input(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        {
            let Some(find) = self.find.as_mut() else {
                return false;
            };
            match key.code {
                KeyCode::Char(c) if !ctrl => {
                    let len = find.query.chars().count();
                    let at = find.cursor.min(len);
                    let mut chars: Vec<char> = find.query.chars().collect();
                    chars.insert(at, c);
                    find.query = chars.into_iter().collect();
                    find.cursor = at + 1;
                }
                KeyCode::Backspace => {
                    if find.cursor > 0 {
                        let mut chars: Vec<char> = find.query.chars().collect();
                        chars.remove(find.cursor - 1);
                        find.query = chars.into_iter().collect();
                        find.cursor -= 1;
                    }
                }
                KeyCode::Delete => {
                    let len = find.query.chars().count();
                    if find.cursor < len {
                        let mut chars: Vec<char> = find.query.chars().collect();
                        chars.remove(find.cursor);
                        find.query = chars.into_iter().collect();
                    }
                }
                KeyCode::Left => {
                    if find.cursor > 0 {
                        find.cursor -= 1;
                    }
                }
                KeyCode::Right => {
                    let len = find.query.chars().count();
                    if find.cursor < len {
                        find.cursor += 1;
                    }
                }
                KeyCode::Home => find.cursor = 0,
                KeyCode::End => find.cursor = find.query.chars().count(),
                _ => {}
            }
        }
        self.find_research();
        true
    }

    /// Text for the find bar: `Find: <query> [i/N]`, the not-found form,
    /// or just `Find: ` for an empty query.
    pub fn find_bar_text(&self) -> String {
        let Some(find) = &self.find else {
            return String::new();
        };
        if find.query.is_empty() {
            return "Find: ".to_string();
        }
        let matches = self.find_matches();
        if matches.is_empty() {
            return format!("Find: {} — not found", find.query);
        }
        match matches.iter().position(|&m| m == (self.row, self.col)) {
            Some(i) => format!("Find: {} [{}/{}]", find.query, i + 1, matches.len()),
            None => format!("Find: {} [{} matches]", find.query, matches.len()),
        }
    }

    pub fn prev_cell(&mut self) {
        if self.rows.is_empty() || self.headers.is_empty() {
            return;
        }
        self.sel_anchor = None;
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.ncols() - 1;
        }
        self.ensure_visible();
    }

    pub fn ensure_visible(&mut self) {
        if self.view_h == 0 {
            self.view_h = 20;
        }
        if self.row < self.off_row {
            self.off_row = self.row;
        } else if self.row >= self.off_row + self.view_h {
            self.off_row = self.row - self.view_h + 1;
        }
        // Horizontal: keep the selected column's rendered span on screen.
        // If the selected column is left of the window, jump the window to
        // it; if its right edge runs past the window, slide right until it
        // fits.
        let widths = self.col_widths();
        if self.col < self.off_col {
            self.off_col = self.col;
        } else {
            let mut right = 0usize;
            for (i, w) in widths.iter().enumerate().take(self.col + 1) {
                if i >= self.off_col {
                    right += w + 3; // │ + space + content + space
                }
            }
            while right > self.view_w.max(10) && self.off_col < self.col {
                right -= widths[self.off_col] + 3;
                self.off_col += 1;
            }
        }
    }

    /// Display width of each column, capped for layout.
    pub fn col_widths(&self) -> Vec<usize> {
        let n = self.ncols();
        let mut widths = vec![3usize; n];
        for (i, h) in self.headers.iter().enumerate().take(n) {
            widths[i] = widths[i].max(h.chars().count().min(MAX_COL_WIDTH));
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate().take(n) {
                widths[i] = widths[i].max(cell.chars().count().min(MAX_COL_WIDTH));
            }
        }
        widths
    }

    /// Move the selection to a mouse click at (x, y).
    pub fn click_at(&mut self, x: u16, y: u16) {
        if self.rows.is_empty() || self.headers.is_empty() {
            return;
        }
        // Header row at view_y+1, grid separator at view_y+2.
        let ty = self.view_y + 3;
        if y < ty {
            return;
        }
        let widths = self.col_widths();
        // The row-number gutter sits between the border and the first │.
        let gutter = (self.rows.len().to_string().len().max(1) + 1) as u16;
        let mut cx = self.view_x + 1 + gutter;
        let mut hit = None;
        for (i, w) in widths.iter().enumerate().skip(self.off_col) {
            let cw = *w as u16 + 3; // │ + space + content + space
            if x >= cx && x < cx + cw {
                hit = Some(i);
                break;
            }
            cx += cw;
        }
        let row = self.off_row + (y - ty) as usize;
        if let (Some(c), true) = (hit, row < self.rows.len()) {
            self.col = c;
            self.row = row;
            self.sel_anchor = None;
            self.ensure_visible();
        }
    }

    /// Scroll the view by `delta` rows (mouse wheel).
    pub fn scroll_by(&mut self, delta: isize) {
        let max_off = self.rows.len().saturating_sub(1);
        self.off_row = (self.off_row as isize + delta).clamp(0, max_off as isize) as usize;
        if self.row < self.off_row || self.row >= self.off_row + self.view_h.max(1) {
            self.row = self.off_row.min(max_off);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpcsv(name: &str, content: &str) -> PathBuf {
        // Unique per call: tests run in parallel threads sharing one process.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fex-sheettest-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn parse_edit_save_roundtrip() {
        let p = tmpcsv(
            "t.csv",
            "name,age,note\n\"doe, john\",30,\"says \"\"hi\"\"\"\nann,25,\n",
        );
        let mut sh = open_sheet(&p).unwrap();
        assert_eq!(sh.headers, vec!["name", "age", "note"]);
        assert_eq!(sh.rows.len(), 2);
        // Quoted comma and escaped quotes survive parsing.
        assert_eq!(sh.rows[0][0], "doe, john");
        assert_eq!(sh.rows[0][2], "says \"hi\"");
        // Edit a cell and add a row, then save.
        sh.row = 0;
        sh.col = 1;
        sh.set_cell("31".to_string());
        assert!(sh.dirty);
        sh.row = 1;
        sh.col = 0;
        sh.set_cell("bob".to_string());
        sh.add_row();
        assert_eq!(sh.rows.len(), 3);
        sh.save().unwrap();
        assert!(!sh.dirty);
        // Re-read: quoting is preserved through the round trip.
        let sh2 = open_sheet(&p).unwrap();
        assert_eq!(sh2.rows[0][0], "doe, john");
        assert_eq!(sh2.rows[0][1], "31");
        assert_eq!(sh2.rows[1][0], "bob");
        assert_eq!(sh2.rows[2], vec!["", "", ""]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn navigation_wraps_cells() {
        let p = tmpcsv("n.csv", "a,b\n1,2\n3,4\n");
        let mut sh = open_sheet(&p).unwrap();
        assert_eq!((sh.row, sh.col), (0, 0));
        sh.next_cell();
        assert_eq!((sh.row, sh.col), (0, 1));
        sh.next_cell(); // wraps to next row
        assert_eq!((sh.row, sh.col), (1, 0));
        sh.prev_cell();
        assert_eq!((sh.row, sh.col), (0, 1));
        sh.move_cell(5, 5); // clamps
        assert_eq!((sh.row, sh.col), (1, 1));
        sh.move_cell(-5, -5);
        assert_eq!((sh.row, sh.col), (0, 0));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn click_selects_cell() {
        let p = tmpcsv("c.csv", "a,bb\n1,22\n333,4\n");
        let mut sh = open_sheet(&p).unwrap();
        // widths: col0 = max(1,1,3)=3, col1 = max(3-min,2,2,1)=3
        assert_eq!(sh.col_widths(), vec![3, 3]);
        sh.view_x = 10;
        sh.view_y = 5;
        // Header at y=6, grid separator at y=7, first data row at y=8.
        // 2 data rows → 1-char gutter + space shifts col 0 to x=13..19.
        sh.click_at(14, 8);
        assert_eq!((sh.row, sh.col), (0, 0));
        // Col 1 spans x=19..25.
        sh.click_at(20, 9);
        assert_eq!((sh.row, sh.col), (1, 1));
        // Clicks on the header or the separator line select nothing.
        sh.click_at(14, 6);
        assert_eq!((sh.row, sh.col), (1, 1));
        sh.click_at(14, 7);
        assert_eq!((sh.row, sh.col), (1, 1));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[test]
    fn add_column_inserts_after_current_with_header() {
        let p = tmpcsv("ac.csv", "a,b\n1,2\n3,4\n");
        let mut sh = open_sheet(&p).unwrap();
        sh.col = 0;
        sh.add_column(String::from("c"));
        assert_eq!(sh.headers, vec!["a", "c", "b"]);
        assert_eq!(sh.rows[0], vec!["1", "", "2"]);
        assert_eq!(sh.rows[1], vec!["3", "", "4"]);
        assert_eq!(sh.col, 1); // new column is selected
        assert!(sh.dirty);
        // save round-trips the new column
        sh.save().unwrap();
        let sh2 = open_sheet(&p).unwrap();
        assert_eq!(sh2.headers, vec!["a", "c", "b"]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[test]
    fn col_letter_names() {
        assert_eq!(col_letter(1), "A");
        assert_eq!(col_letter(26), "Z");
        assert_eq!(col_letter(27), "AA");
        assert_eq!(col_letter(28), "AB");
    }

    fn tmpxlsx(name: &str) -> PathBuf {
        // Unique per call: tests run in parallel threads sharing one process.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1000);
        let dir = std::env::temp_dir().join(format!(
            "fex-sheettest-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        // Build a two-sheet workbook with umya itself.
        let mut book = umya_spreadsheet::new_file();
        let ws = book.sheet_by_name_mut("Sheet1").unwrap();
        ws.cell_mut("A1").set_value("Item");
        ws.cell_mut("B1").set_value("Qty");
        ws.cell_mut("A2").set_value("Apples");
        ws.cell_mut("B2").set_value_number(10);
        ws.cell_mut("C2").set_formula("B2*2");
        ws.cell_mut("A3").set_value("TRUE");
        ws.cell_mut("A3").set_value_bool(true);
        // Date cell: serial for 2026-09-30 with a date format.
        let d = ws.cell_mut("A4");
        d.set_value_number(46295.0);
        d.style_mut()
            .number_format_mut()
            .set_format_code("YYYY-MM-DD");
        book.new_sheet("Second").unwrap();
        let ws2 = book.sheet_by_name_mut("Second").unwrap();
        ws2.cell_mut("A1").set_value("other sheet");
        umya_spreadsheet::writer::xlsx::write(&book, &p).unwrap();
        p
    }

    #[test]
    fn xlsx_open_shows_sheets_values_dates_formulas() {
        let p = tmpxlsx("t.xlsx");
        let mut sh = open_sheet(&p).unwrap();
        assert!(sh.is_xlsx());
        // Headers are column letters; every Excel row is a data row.
        assert_eq!(sh.headers, vec!["A", "B", "C"]);
        assert_eq!(sh.rows.len(), 4);
        assert_eq!(sh.rows[0], vec!["Item", "Qty", ""]);
        assert_eq!(sh.rows[1][0], "Apples");
        assert_eq!(sh.rows[1][1], "10");
        // Formula cell with no cached value shows the formula.
        assert_eq!(sh.rows[1][2], "=B2*2");
        assert_eq!(sh.rows[2][0], "TRUE");
        // Date serial renders as a date.
        assert_eq!(sh.rows[3][0], "2026-09-30");
        // Sheet tabs.
        let (names, active) = sh.sheet_tabs().unwrap();
        assert_eq!(names, &["Sheet1".to_string(), "Second".to_string()]);
        assert_eq!(active, 0);
        // Switch sheets and back.
        sh.switch_sheet(1);
        assert_eq!(sh.sheet_tabs().unwrap().1, 1);
        assert_eq!(sh.rows[0][0], "other sheet");
        sh.switch_sheet(1); // wraps around
        assert_eq!(sh.sheet_tabs().unwrap().1, 0);
        assert_eq!(sh.rows[1][0], "Apples");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn xlsx_edit_save_roundtrip() {
        let p = tmpxlsx("e.xlsx");
        let mut sh = open_sheet(&p).unwrap();
        // Edit a plain cell: smart typing keeps it numeric.
        sh.row = 1;
        sh.col = 1;
        sh.set_cell("25".to_string());
        assert!(sh.dirty);
        assert_eq!(sh.rows[1][1], "25");
        // Edit the formula cell: pre-fill is the formula, commit keeps it.
        sh.row = 1;
        sh.col = 2;
        assert_eq!(sh.edit_text().unwrap(), "=B2*2");
        sh.set_cell("=B2*3".to_string());
        assert_eq!(sh.rows[1][2], "=B2*3");
        // Edit a date cell with date text: stays a date.
        sh.row = 3;
        sh.col = 0;
        sh.set_cell("2026-10-01".to_string());
        sh.save().unwrap();
        assert!(!sh.dirty);
        // Re-read the file with umya directly.
        let book = umya_spreadsheet::reader::xlsx::read(&p).unwrap();
        let ws = book.sheet_by_name("Sheet1").unwrap();
        assert_eq!(ws.cell("B2").unwrap().value_number(), Some(25.0));
        assert_eq!(ws.cell("C2").unwrap().formula(), "B2*3");
        let serial = ws.cell("A4").unwrap().value_number().unwrap();
        assert!((serial - 46296.0).abs() < 0.001, "serial was {serial}");
        // And through fex again.
        let sh2 = open_sheet(&p).unwrap();
        assert_eq!(sh2.rows[1][1], "25");
        assert_eq!(sh2.rows[1][2], "=B2*3");
        assert_eq!(sh2.rows[3][0], "2026-10-01");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn xlsx_add_row_and_column() {
        let p = tmpxlsx("a.xlsx");
        let mut sh = open_sheet(&p).unwrap();
        sh.row = 0;
        sh.col = 0;
        sh.add_row();
        assert_eq!(sh.rows.len(), 5);
        assert_eq!(sh.row, 1);
        sh.set_cell("new".to_string());
        sh.col = 0;
        sh.add_column("ignored".to_string());
        assert_eq!(sh.headers, vec!["A", "B", "C", "D"]);
        assert_eq!(sh.col, 1);
        sh.save().unwrap();
        let sh2 = open_sheet(&p).unwrap();
        assert_eq!(sh2.rows.len(), 5);
        assert_eq!(sh2.rows[1][0], "new");
        assert_eq!(sh2.ncols(), 4);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn sheet_find_jumps_and_wraps() {
        let p = tmpcsv(
            "find.csv",
            "name,city\nfoo spring,bar\nbaz,foo town\nqux,zzz\n",
        );
        let mut sh = open_sheet(&p).unwrap();
        let key = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        sh.open_find();
        for c in "foo".chars() {
            sh.find_input(key(c));
        }
        // Typing jumps live to the first match at/after the cursor.
        assert_eq!((sh.row, sh.col), (0, 0));
        assert_eq!(sh.find_bar_text(), "Find: foo [1/2]");
        sh.find_jump(1);
        assert_eq!((sh.row, sh.col), (1, 1));
        assert_eq!(sh.find_bar_text(), "Find: foo [2/2]");
        sh.find_jump(1); // wraps around
        assert_eq!((sh.row, sh.col), (0, 0));
        sh.find_jump(-1); // previous wraps to the last match
        assert_eq!((sh.row, sh.col), (1, 1));
        // Headers are not searched: "city" is only a header here.
        sh.find_input(key('x'));
        assert!(sh.find.as_ref().unwrap().not_found);
        assert_eq!(sh.find_bar_text(), "Find: foox — not found");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn selection_rect_pins_anchor_and_clears() {
        let p = tmpcsv("sel.csv", "a,b,c\n1,2,3\n4,5,6\n");
        let mut sh = open_sheet(&p).unwrap();
        assert!(sh.selection_rect().is_none());
        // Shift+Right, Shift+Down from (0,0): block is (0,0)-(1,1).
        sh.extend_selection(0, 1);
        sh.extend_selection(1, 0);
        assert_eq!(sh.selection_rect(), Some(((0, 0), (1, 1))));
        assert_eq!(sh.selection_tsv().as_deref(), Some("1\t2\n4\t5\n"));
        // Plain moves clear the anchor.
        sh.move_cell(0, 1);
        assert!(sh.selection_rect().is_none());
        assert_eq!(sh.selection_tsv().as_deref(), Some("6\n"));
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn paste_text_fills_from_cursor_and_grows() {
        let p = tmpcsv("paste.csv", "a,b\n1,2\n");
        let mut sh = open_sheet(&p).unwrap();
        // Paste a 2x3 block at (0,0): overwrites and grows the grid.
        sh.paste_text("x\ty\tz\n7\t8\t9");
        assert_eq!(sh.rows[0], vec!["x", "y", "z"]);
        assert_eq!(sh.rows[1], vec!["7", "8", "9"]);
        assert_eq!(sh.ncols(), 3);
        assert_eq!(sh.headers.len(), 3);
        assert!(sh.dirty);
        assert_eq!(sh.message, "Pasted 2×3 cells");
        // Rows stay rectangular.
        assert!(sh.rows.iter().all(|r| r.len() == 3));
        // Paste at the bottom grows rows.
        sh.row = 5;
        sh.col = 0;
        sh.paste_text("new");
        assert_eq!(sh.rows.len(), 6);
        assert_eq!(sh.rows[5][0], "new");
        assert_eq!(sh.message, "Pasted cell");
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn paste_text_empty_is_noop() {
        let p = tmpcsv("paste2.csv", "a,b\n1,2\n");
        let mut sh = open_sheet(&p).unwrap();
        sh.paste_text("   \n");
        assert!(!sh.dirty);
        assert_eq!(sh.message, "Clipboard is empty");
        assert_eq!(sh.rows[0], vec!["1", "2"]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn paste_xlsx_roundtrip() {
        // An xlsx paste writes through the workbook: save and re-read.
        let dir =
            std::env::temp_dir().join(format!("fex-sheettest-xlsxpaste-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("p.xlsx");
        crate::fs::create_file(&dir, "p.xlsx").unwrap();
        let mut sh = open_sheet(&p).unwrap();
        sh.paste_text("10\t20\n30\t40");
        sh.save().unwrap();
        let sh2 = open_sheet(&p).unwrap();
        assert_eq!(sh2.rows[0][0], "10");
        assert_eq!(sh2.rows[0][1], "20");
        assert_eq!(sh2.rows[1][0], "30");
        assert_eq!(sh2.rows[1][1], "40");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn find_matches_cells_across_grid() {
        let p = tmpcsv("findm.csv", "a,b\nfoo,bar\nbaz,foo\n");
        let mut sh = open_sheet(&p).unwrap();
        sh.open_find();
        let key = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        for c in "foo".chars() {
            sh.find_input(key(c));
        }
        let mut m = sh.find_matches();
        m.sort();
        assert_eq!(m, vec![(0, 0), (1, 1)]);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[test]
    fn add_row_and_column_grow_empty_xlsx() {
        // Regression: umya's insert only shifts existing cells, so on an
        // empty sheet add_row/add_column visibly did nothing.
        let dir = std::env::temp_dir().join("fex-test-xlsxgrow");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::fs::create_file(&dir, "t.xlsx").unwrap();
        let p = dir.join("t.xlsx");
        let mut sh = open_sheet(&p).unwrap();
        sh.add_row();
        assert_eq!(sh.rows.len(), 2);
        assert_eq!(sh.message, "Added row");
        sh.add_column("x".to_string());
        assert_eq!(sh.ncols(), 2);
        assert_eq!(sh.message, "Added column");
        assert!(sh.rows.iter().all(|r| r.len() == 2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn add_row_shifts_data_down_xlsx() {
        let dir = std::env::temp_dir().join("fex-test-xlsxshift");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::fs::create_file(&dir, "t.xlsx").unwrap();
        let p = dir.join("t.xlsx");
        let mut sh = open_sheet(&p).unwrap();
        sh.paste_text("a\nb\nc");
        sh.row = 0;
        sh.add_row();
        assert_eq!(sh.rows.len(), 4);
        assert_eq!(sh.rows[0][0], "a");
        assert_eq!(sh.rows[1][0], "");
        assert_eq!(sh.rows[2][0], "b");
        assert_eq!(sh.rows[3][0], "c");
        // The grid survives a save/reopen round trip.
        sh.save().unwrap();
        let sh2 = open_sheet(&p).unwrap();
        assert_eq!(sh2.rows.len(), 4);
        assert_eq!(sh2.rows[2][0], "b");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
