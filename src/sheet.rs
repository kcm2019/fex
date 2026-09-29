//! CSV table viewer/editor.
//!
//! Opened with `e` on a `.csv` file. Arrow keys move between cells,
//! Enter edits the current cell, Ctrl+S saves, `a` adds a row, `A` adds a
//! column, Esc closes (asking about unsaved changes). The first row is
//! treated as the header.

use std::io;
use std::path::{Path, PathBuf};

/// Longest cell content kept per column when laying out the table.
const MAX_COL_WIDTH: usize = 28;

/// Parse one CSV file into a Sheet. The first record becomes the header.
pub fn open_sheet(path: &Path) -> io::Result<Sheet> {
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
    })
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
}

impl Sheet {
    pub fn ncols(&self) -> usize {
        self.headers.len()
    }

    pub fn save(&mut self) -> io::Result<()> {
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
        self.dirty = false;
        self.message = format!("Saved {}", self.path.display());
        Ok(())
    }

    pub fn set_cell(&mut self, value: String) {
        if self.rows.is_empty() || self.headers.is_empty() {
            return;
        }
        self.rows[self.row][self.col] = value;
        self.dirty = true;
    }

    /// Insert an empty row below the current one and select it.
    pub fn add_row(&mut self) {
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
    /// name, and select it. Rows stay rectangular.
    pub fn add_column(&mut self, name: String) {
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
        if self.col + 1 < self.ncols() {
            self.col += 1;
        } else if self.row + 1 < self.rows.len() {
            self.row += 1;
            self.col = 0;
        }
        self.ensure_visible();
    }

    pub fn prev_cell(&mut self) {
        if self.rows.is_empty() || self.headers.is_empty() {
            return;
        }
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
        let mut cx = self.view_x + 1;
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
        // Col 0 spans x=11..17 (│ + space + 3 + space).
        sh.click_at(12, 8);
        assert_eq!((sh.row, sh.col), (0, 0));
        // Col 1 spans x=17..23.
        sh.click_at(18, 9);
        assert_eq!((sh.row, sh.col), (1, 1));
        // Clicks on the header or the separator line select nothing.
        sh.click_at(12, 6);
        assert_eq!((sh.row, sh.col), (1, 1));
        sh.click_at(12, 7);
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
}
