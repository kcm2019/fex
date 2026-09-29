//! Application state: directory listing, selection, filter, sort, UI modes,
//! and the multi-tab workspace (file-browser tabs plus terminal tabs).

use crate::editor::Editor;
use crate::fs::{self, Entry, Preview, SearchResult};
use crate::sheet::{self, Sheet};
use crate::shell::ShellTab;
use crate::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy)]
pub struct SortOrder {
    pub key: SortKey,
    pub ascending: bool,
}

impl SortOrder {
    pub fn label(&self) -> &'static str {
        match (self.key, self.ascending) {
            (SortKey::Name, true) => "Name ↑",
            (SortKey::Name, false) => "Name ↓",
            (SortKey::Size, true) => "Size ↑",
            (SortKey::Size, false) => "Size ↓",
            (SortKey::Modified, true) => "Modified ↑",
            (SortKey::Modified, false) => "Modified ↓",
        }
    }

    fn cycle_key(&mut self) {
        self.key = match self.key {
            SortKey::Name => SortKey::Size,
            SortKey::Size => SortKey::Modified,
            SortKey::Modified => SortKey::Name,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    NewFile,
    NewDir,
    Rename,
    Filter,
    CsvCell,
    CsvColumn,
}

impl InputKind {
    pub fn title(&self) -> &'static str {
        match self {
            InputKind::NewFile => "New file",
            InputKind::NewDir => "New directory",
            InputKind::Rename => "Rename",
            InputKind::Filter => "Filter (live — Esc clears)",
            InputKind::CsvCell => "Edit cell",
            InputKind::CsvColumn => "New column name",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Input(InputKind),
    ConfirmDelete,
    Help,
    Editor,
    ConfirmDiscard,
    Search,
    Sheet,
}

/// List view (file list + preview pane) or Miller-column view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Columns,
}

/// One directory column in column view.
#[derive(Debug, Clone)]
pub struct Column {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

impl Column {
    fn new(path: PathBuf, sort: SortOrder, show_hidden: bool) -> Self {
        let entries = fs::list_dir(&path);
        let idx = view_indices(&entries, "", sort, show_hidden);
        let mut sorted = Vec::with_capacity(idx.len());
        for i in idx {
            sorted.push(entries[i].clone());
        }
        Self {
            path,
            entries: sorted,
            selected: 0,
        }
    }

    fn select_name(&mut self, name: &str) {
        if let Some(pos) = self.entries.iter().position(|e| e.name == name) {
            self.selected = pos;
        }
    }
}

/// Indices into `entries` after hidden-filtering, name filtering, and sorting.
fn view_indices(entries: &[Entry], filter: &str, sort: SortOrder, show_hidden: bool) -> Vec<usize> {
    let needle = filter.to_lowercase();
    let mut idx: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            (show_hidden || !e.is_hidden)
                && (needle.is_empty() || e.name.to_lowercase().contains(&needle))
        })
        .map(|(i, _)| i)
        .collect();
    idx.sort_by(|&a, &b| {
        let ea = &entries[a];
        let eb = &entries[b];
        // Directories always come first.
        match (ea.is_dir, eb.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => {
                let ord = match sort.key {
                    SortKey::Name => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
                    SortKey::Size => ea.size.cmp(&eb.size),
                    SortKey::Modified => ea.modified.cmp(&eb.modified),
                };
                if sort.ascending {
                    ord
                } else {
                    ord.reverse()
                }
            }
        }
    });
    idx
}

pub struct App {
    pub cwd: PathBuf,
    entries: Vec<Entry>,
    visible: Vec<usize>,
    selected: usize,
    pub list_state: ListState,
    pub show_hidden: bool,
    pub filter: String,
    pub sort: SortOrder,
    pub mode: Mode,
    pub input: String,
    pub cursor: usize, // character index into `input`
    pub status: String,
    preview_cache: (PathBuf, bool, Preview),
    pub editor: Option<Editor>,
    pub view: ViewMode,
    pub columns: Vec<Column>,
    pub col_active: usize,
    pub search_results: Vec<SearchResult>,
    pub search_selected: usize,
    pub search_deep: bool,
    pub sheet: Option<Sheet>,
    /// Last-seen mtime of the current directory, for auto-refresh.
    dir_mtime: Option<SystemTime>,
    /// Top-left of the rendered file list, for mapping mouse clicks.
    pub list_origin: (u16, u16),
    /// Index into `theme::THEMES`.
    pub theme_idx: usize,
    /// Editor line-number gutter toggle (persisted).
    pub show_line_numbers: bool,
}

fn char_index_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

impl App {
    /// A fresh file-browser tab. Theme settings are synced by the
    /// workspace after construction.
    pub fn new(cwd: PathBuf) -> Self {
        let mut app = Self {
            cwd,
            entries: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            show_hidden: false,
            filter: String::new(),
            sort: SortOrder {
                key: SortKey::Name,
                ascending: true,
            },
            mode: Mode::Normal,
            input: String::new(),
            cursor: 0,
            status: String::new(),
            preview_cache: (PathBuf::new(), false, Preview::Text(String::new())),
            editor: None,
            view: ViewMode::List,
            columns: Vec::new(),
            col_active: 0,
            search_results: Vec::new(),
            search_selected: 0,
            search_deep: false,
            sheet: None,
            dir_mtime: None,
            list_origin: (0, 0),
            theme_idx: 0,
            show_line_numbers: true,
        };
        app.refresh();
        app
    }

    /// The active color theme (copy; cheap).
    pub fn theme(&self) -> theme::Theme {
        theme::THEMES[self.theme_idx % theme::THEMES.len()]
    }

    // -- view helpers -----------------------------------------------------

    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    pub fn visible_entries(&self) -> impl Iterator<Item = &Entry> {
        self.visible.iter().map(|&i| &self.entries[i])
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        if self.view == ViewMode::Columns {
            let col = self.columns.get(self.col_active)?;
            col.entries.get(col.selected)
        } else {
            self.visible.get(self.selected).map(|&i| &self.entries[i])
        }
    }

    fn clamp_selection(&mut self) {
        if self.visible.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.visible.len() {
            self.selected = self.visible.len() - 1;
        }
        self.list_state.select(Some(self.selected));
    }

    /// Re-read the current directory, preserving the selection when possible.
    /// In column view every column is rebuilt instead.
    pub fn refresh(&mut self) {
        if self.view == ViewMode::Columns {
            self.refresh_columns();
        } else {
            let selected_name = self.selected_entry().map(|e| e.name.clone());
            self.entries = fs::list_dir(&self.cwd);
            self.apply_view();
            if let Some(name) = selected_name {
                if let Some(pos) = self
                    .visible
                    .iter()
                    .position(|&i| self.entries[i].name == name)
                {
                    self.selected = pos;
                }
            }
            self.clamp_selection();
        }
        self.preview_cache.0.clear(); // invalidate preview
        self.note_dir_mtime();
    }

    fn note_dir_mtime(&mut self) {
        self.dir_mtime = std::fs::metadata(&self.cwd).and_then(|m| m.modified()).ok();
    }

    /// Called each event-loop tick: if something outside the app changed the
    /// directory (a download landing, another program), refresh the listing.
    /// Silent and selection-preserving. Returns true when a refresh happened.
    pub fn poll_external_changes(&mut self) -> bool {
        let mtime = std::fs::metadata(&self.cwd).and_then(|m| m.modified()).ok();
        if mtime != self.dir_mtime {
            self.refresh();
            true
        } else {
            false
        }
    }

    /// Rebuild every column's entries, preserving selections by name and
    /// dropping columns whose directory vanished.
    fn refresh_columns(&mut self) {
        for col in &mut self.columns {
            let sel_name = col.entries.get(col.selected).map(|e| e.name.clone());
            let fresh = Column::new(col.path.clone(), self.sort, self.show_hidden);
            *col = fresh;
            if let Some(name) = sel_name {
                col.select_name(&name);
            }
        }
        while self.columns.len() > 1 && !self.columns.last().is_some_and(|c| c.path.exists()) {
            self.columns.pop();
        }
        if self.columns.is_empty() {
            self.view = ViewMode::List;
            self.refresh();
            return;
        }
        self.col_active = self.col_active.min(self.columns.len() - 1);
        self.sync_columns();
        if let Some(col) = self.columns.get(self.col_active) {
            self.cwd = col.path.clone();
        }
        self.preview_cache.0.clear();
    }

    /// Rebuild the visible list from filter + sort settings.
    fn apply_view(&mut self) {
        self.visible = view_indices(&self.entries, &self.filter, self.sort, self.show_hidden);
    }

    // -- navigation -------------------------------------------------------

    pub fn move_selection(&mut self, delta: i32) {
        if self.view == ViewMode::Columns {
            self.col_move_selection(delta);
            return;
        }
        if self.visible.is_empty() {
            return;
        }
        let len = self.visible.len() as i32;
        self.selected = (self.selected as i32 + delta).clamp(0, len - 1) as usize;
        self.list_state.select(Some(self.selected));
    }

    pub fn jump_to(&mut self, index: usize) {
        if self.view == ViewMode::Columns {
            let Some(col) = self.columns.get_mut(self.col_active) else {
                return;
            };
            if col.entries.is_empty() {
                return;
            }
            col.selected = index.min(col.entries.len() - 1);
            self.sync_columns();
            return;
        }
        if self.visible.is_empty() {
            return;
        }
        self.selected = index.min(self.visible.len() - 1);
        self.list_state.select(Some(self.selected));
    }

    /// Select the file-list row under a mouse click at terminal (x, y).
    /// Clicks outside the list are ignored.
    pub fn click_select(&mut self, x: u16, y: u16) {
        if self.view != ViewMode::List {
            return;
        }
        let (ox, oy) = self.list_origin;
        if x < ox || y <= oy {
            return;
        }
        let j = (y - oy - 1) as usize;
        let idx = self.list_state.offset() + j;
        if idx < self.visible.len() {
            self.selected = idx;
            self.list_state.select(Some(idx));
        }
    }

    /// Enter a directory, or open a file with the OS default app.
    pub fn enter_selected(&mut self) {
        if self.view == ViewMode::Columns {
            self.col_go_right();
            return;
        }
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        if entry.is_dir {
            self.cwd = entry.path;
            self.selected = 0;
            self.refresh();
        } else if let Err(e) = fs::open_with_default(&entry.path) {
            self.status = format!("Could not open {}: {e}", entry.name);
        } else {
            self.status = format!("Opened {}", entry.name);
        }
    }

    /// Open the selected text file in the built-in editor.
    /// CSV files open in the table viewer instead.
    pub fn open_editor(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        if entry.is_dir {
            self.status = "Cannot edit a directory".to_string();
            return;
        }
        if entry
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
        {
            self.open_sheet();
            return;
        }
        let name = entry.name.clone();
        match Editor::open(&entry.path) {
            Ok(ed) => {
                self.editor = Some(ed);
                if let Some(ed) = self.editor.as_mut() {
                    ed.show_line_numbers = self.show_line_numbers;
                }
                self.mode = Mode::Editor;
            }
            Err(e) => self.status = format!("Cannot edit {name}: {e}"),
        }
    }

    /// Close the editor, discarding the buffer.
    pub fn close_editor(&mut self) {
        self.editor = None;
        self.mode = Mode::Normal;
        self.refresh(); // the file may have changed on disk
    }

    /// Open the selected CSV file in the table viewer.
    pub fn open_sheet(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        match sheet::open_sheet(&entry.path) {
            Ok(sh) => {
                self.sheet = Some(sh);
                self.mode = Mode::Sheet;
            }
            Err(e) => self.status = format!("Cannot open {}: {e}", entry.name),
        }
    }

    /// Close the table viewer, discarding its buffer.
    pub fn close_sheet(&mut self) {
        self.sheet = None;
        self.mode = Mode::Normal;
        self.refresh(); // the file may have changed on disk
    }

    /// Go to the parent directory, keeping the old directory selected.
    pub fn go_to_parent(&mut self) {
        if self.view == ViewMode::Columns {
            self.col_go_left();
            return;
        }
        let Some(parent) = self.cwd.parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let old_name = self
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        self.cwd = parent;
        self.selected = 0;
        self.refresh();
        if let Some(name) = old_name {
            if let Some(pos) = self
                .visible
                .iter()
                .position(|&i| self.entries[i].name == name)
            {
                self.selected = pos;
                self.list_state.select(Some(pos));
            }
        }
    }

    // -- column view ------------------------------------------------------

    /// Switch to Miller-column view, building the ancestor chain for the
    /// current directory and preserving the current selection.
    pub fn enter_column_mode(&mut self) {
        if self.view == ViewMode::Columns {
            return;
        }
        let sel = self.selected_entry().map(|e| e.name.clone());
        self.view = ViewMode::Columns;
        self.build_columns(sel);
        self.sync_columns();
    }

    pub fn exit_column_mode(&mut self) {
        if self.view == ViewMode::List {
            return;
        }
        self.view = ViewMode::List;
        self.refresh();
    }

    /// (Re)build the column chain from `self.cwd` upwards, selecting each
    /// child on the way down. `select_in_last` overrides the selection in
    /// the final (current directory) column.
    fn build_columns(&mut self, select_in_last: Option<String>) {
        let mut chain = vec![self.cwd.clone()];
        while chain.len() < 6 {
            let Some(parent) = chain.last().unwrap().parent() else {
                break;
            };
            chain.push(parent.to_path_buf());
        }
        chain.reverse();
        let mut columns: Vec<Column> = chain
            .into_iter()
            .map(|p| Column::new(p, self.sort, self.show_hidden))
            .collect();
        for w in 0..columns.len().saturating_sub(1) {
            let child: Option<String> = columns[w + 1]
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
            if let Some(child) = child {
                columns[w].select_name(&child);
            }
        }
        if let Some(name) = select_in_last {
            if let Some(last) = columns.last_mut() {
                last.select_name(&name);
            }
        }
        self.columns = columns;
        self.col_active = self.columns.len() - 1;
    }

    /// After the active column's selection changes: drop trailing columns
    /// and push a fresh one when the selected entry is a directory.
    fn sync_columns(&mut self) {
        self.columns.truncate(self.col_active + 1);
        let sel = self
            .columns
            .get(self.col_active)
            .and_then(|c| c.entries.get(c.selected))
            .cloned();
        if let Some(e) = sel {
            if e.is_dir {
                self.columns
                    .push(Column::new(e.path, self.sort, self.show_hidden));
            }
        }
        self.preview_cache.0.clear();
    }

    /// Move the selection inside the active column, then refresh trailing columns.
    pub fn col_move_selection(&mut self, delta: i32) {
        let Some(col) = self.columns.get_mut(self.col_active) else {
            return;
        };
        if col.entries.is_empty() {
            return;
        }
        let len = col.entries.len() as i32;
        col.selected = (col.selected as i32 + delta).clamp(0, len - 1) as usize;
        self.sync_columns();
    }

    /// Right: step into the trailing column, or open the selected file when
    /// there is none (the selection isn't a directory).
    pub fn col_go_right(&mut self) {
        if self.col_active + 1 < self.columns.len() {
            self.col_active += 1;
            self.cwd = self.columns[self.col_active].path.clone();
            self.sync_columns();
            return;
        }
        let Some(e) = self.selected_entry().cloned() else {
            return;
        };
        if e.is_dir {
            self.sync_columns();
        } else if let Err(err) = fs::open_with_default(&e.path) {
            self.status = format!("Could not open {}: {err}", e.name);
        } else {
            self.status = format!("Opened {}", e.name);
        }
    }

    /// Left: step back to the parent column, or to the filesystem parent
    /// when already at the leftmost shown column.
    pub fn col_go_left(&mut self) {
        if self.col_active > 0 {
            self.col_active -= 1;
            self.cwd = self.columns[self.col_active].path.clone();
            self.sync_columns();
            return;
        }
        let Some(parent) = self.cwd.parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let old_name = self
            .cwd
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        self.cwd = parent;
        self.build_columns(old_name);
        self.sync_columns();
    }

    // -- search -------------------------------------------------------------

    /// Enter filename search (Tab toggles deep/content search once inside).
    pub fn start_search(&mut self) {
        if self.view == ViewMode::Columns {
            self.exit_column_mode();
        }
        self.input.clear();
        self.cursor = 0;
        self.search_results.clear();
        self.search_selected = 0;
        self.search_deep = false;
        self.mode = Mode::Search;
    }

    /// Re-run the search for the current query.
    pub fn run_search(&mut self) {
        let q = self.input.trim().to_string();
        self.search_results = if self.search_deep {
            fs::search_contents(&self.cwd, &q)
        } else {
            fs::search_names(&self.cwd, &q)
        };
        self.search_selected = 0;
    }

    pub fn search_move(&mut self, delta: i32) {
        if self.search_results.is_empty() {
            return;
        }
        let len = self.search_results.len() as i32;
        self.search_selected = (self.search_selected as i32 + delta).clamp(0, len - 1) as usize;
    }

    pub fn toggle_search_deep(&mut self) {
        self.search_deep = !self.search_deep;
        self.run_search();
        self.status = if self.search_deep {
            String::from("Deep search: matching file contents")
        } else {
            String::from("Search: matching file names")
        };
    }

    /// Jump the file list to the selected hit's directory and select it.
    pub fn search_jump(&mut self) {
        let Some(hit) = self.search_results.get(self.search_selected).cloned() else {
            return;
        };
        let parent = hit.path.parent().map(|p| p.to_path_buf());
        let name = hit
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        let Some(parent) = parent else {
            return;
        };
        self.cwd = parent;
        self.mode = Mode::Normal;
        self.input.clear();
        self.cursor = 0;
        self.refresh();
        if let Some(name) = name {
            if let Some(pos) = self
                .visible
                .iter()
                .position(|&i| self.entries[i].name == name)
            {
                self.selected = pos;
                self.list_state.select(Some(pos));
            }
        }
        self.status = match hit.line_no {
            Some(n) => format!("Found {}:{}", hit.path.display(), n),
            None => format!("Found {}", hit.path.display()),
        };
    }

    pub fn cancel_search(&mut self) {
        self.mode = Mode::Normal;
        self.input.clear();
        self.cursor = 0;
    }

    // -- preview ------------------------------------------------------------

    /// Copy the preview pane's text (text, markdown, PDF, CSV) to the OS
    /// clipboard.
    pub fn copy_preview_text(&mut self) {
        let text = match self.preview() {
            Preview::Text(s) | Preview::Markdown(s) if !s.trim().is_empty() => {
                // Skip single-line bracketed placeholders like "[binary file]".
                let t = s.trim();
                if t.starts_with('[') && t.ends_with(']') && !t.contains('\n') {
                    None
                } else {
                    Some(s.clone())
                }
            }
            Preview::Csv(rows) => {
                let t = rows
                    .iter()
                    .map(|r| r.join(","))
                    .collect::<Vec<_>>()
                    .join("\n");
                if t.trim().is_empty() {
                    None
                } else {
                    Some(t)
                }
            }
            _ => None,
        };
        match text {
            Some(t) => {
                if fs::copy_to_clipboard(&t) {
                    self.status = String::from("Copied preview text");
                } else {
                    self.status = String::from("Clipboard unavailable");
                }
            }
            None => self.status = String::from("Nothing to copy"),
        }
    }

    // -- filter / sort / hidden -------------------------------------------

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
        self.selected = 0;
        self.apply_view();
        self.clamp_selection();
    }

    pub fn clear_filter(&mut self) {
        self.set_filter(String::new());
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.apply_view();
        self.clamp_selection();
        self.status = if self.show_hidden {
            String::from("Showing hidden files")
        } else {
            String::from("Hiding hidden files")
        };
    }

    pub fn cycle_sort_key(&mut self) {
        self.sort.cycle_key();
        self.reapply_sort();
        self.status = format!("Sort: {}", self.sort.label());
    }

    pub fn toggle_sort_dir(&mut self) {
        self.sort.ascending = !self.sort.ascending;
        self.reapply_sort();
        self.status = format!("Sort: {}", self.sort.label());
    }

    /// Rebuild entry ordering after a sort change, for whichever view is active.
    fn reapply_sort(&mut self) {
        if self.view == ViewMode::Columns {
            self.refresh_columns();
        } else {
            self.apply_view();
            self.clamp_selection();
        }
    }

    // -- text input --------------------------------------------------------

    pub fn start_input(&mut self, kind: InputKind) {
        if kind == InputKind::Rename && self.selected_entry().is_none() {
            return;
        }
        self.input.clear();
        self.cursor = 0;
        match kind {
            InputKind::Rename => {
                if let Some(entry) = self.selected_entry() {
                    self.input = entry.name.clone();
                    self.cursor = self.input.chars().count();
                }
            }
            InputKind::Filter => {
                self.input = self.filter.clone();
                self.cursor = self.input.chars().count();
            }
            InputKind::NewFile | InputKind::NewDir | InputKind::CsvColumn => {}
            InputKind::CsvCell => {
                if let Some(sh) = &self.sheet {
                    if let Some(cell) = sh.rows.get(sh.row).and_then(|r| r.get(sh.col)) {
                        self.input = cell.clone();
                        self.cursor = self.input.chars().count();
                    }
                }
            }
        }
        self.mode = Mode::Input(kind);
    }

    pub fn input_insert(&mut self, c: char) {
        let idx = char_index_to_byte(&self.input, self.cursor);
        self.input.insert(idx, c);
        self.cursor += 1;
        self.after_input_edit();
    }

    /// Insert a whole pasted string into the input field at once (instant,
    /// instead of one key event per character).
    pub fn input_insert_text(&mut self, text: &str) {
        let cleaned: String = text.replace("\r\n", "\n").replace('\r', "\n");
        if cleaned.is_empty() {
            return;
        }
        let idx = char_index_to_byte(&self.input, self.cursor);
        self.input.insert_str(idx, &cleaned);
        self.cursor += cleaned.chars().count();
        self.after_input_edit();
    }

    pub fn input_backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let idx = char_index_to_byte(&self.input, self.cursor);
        self.input.remove(idx);
        self.after_input_edit();
    }

    pub fn input_move_cursor(&mut self, delta: i32) {
        let len = self.input.chars().count() as i32;
        self.cursor = (self.cursor as i32 + delta).clamp(0, len) as usize;
    }

    /// Keep the live filter in sync while typing in filter mode.
    fn after_input_edit(&mut self) {
        if matches!(self.mode, Mode::Input(InputKind::Filter)) {
            self.filter = self.input.clone();
            self.selected = 0;
            self.apply_view();
            self.clamp_selection();
        }
    }

    pub fn submit_input(&mut self) {
        let Mode::Input(kind) = self.mode else {
            return;
        };
        let value = self.input.trim().to_string();
        self.mode = Mode::Normal;
        match kind {
            InputKind::NewFile => {
                if value.is_empty() {
                    self.status = String::from("Name cannot be empty");
                    return;
                }
                match fs::create_file(&self.cwd, &value) {
                    Ok(()) => {
                        self.status = format!("Created {value}");
                        self.refresh();
                    }
                    Err(e) => self.status = format!("Could not create {value}: {e}"),
                }
            }
            InputKind::NewDir => {
                if value.is_empty() {
                    self.status = String::from("Name cannot be empty");
                    return;
                }
                match fs::create_dir(&self.cwd, &value) {
                    Ok(()) => {
                        self.status = format!("Created directory {value}");
                        self.refresh();
                    }
                    Err(e) => self.status = format!("Could not create {value}: {e}"),
                }
            }
            InputKind::Rename => {
                let Some(entry) = self.selected_entry().cloned() else {
                    return;
                };
                if value.is_empty() {
                    self.status = String::from("Name cannot be empty");
                    return;
                }
                match fs::rename_entry(&entry.path, &value) {
                    Ok(()) => {
                        self.status = format!("Renamed to {value}");
                        self.refresh();
                    }
                    Err(e) => self.status = format!("Could not rename: {e}"),
                }
            }
            InputKind::Filter => {
                self.status = if self.filter.is_empty() {
                    String::new()
                } else {
                    format!("Filter: {}", self.filter)
                };
            }
            InputKind::CsvCell => {
                // Use the raw input: cell values may legitimately have
                // leading/trailing spaces, so don't trim.
                if let Some(sh) = self.sheet.as_mut() {
                    sh.set_cell(self.input.clone());
                    self.status = String::from("Cell updated");
                }
                self.mode = Mode::Sheet;
            }
            InputKind::CsvColumn => {
                let name = self.input.trim().to_string();
                if let Some(sh) = self.sheet.as_mut() {
                    let name = if name.is_empty() {
                        format!("col{}", sh.ncols() + 1)
                    } else {
                        name
                    };
                    sh.add_column(name);
                }
                self.mode = Mode::Sheet;
            }
        }
    }

    pub fn cancel_input(&mut self) {
        let kind = match self.mode {
            Mode::Input(k) => k,
            _ => {
                self.mode = Mode::Normal;
                return;
            }
        };
        if kind == InputKind::Filter {
            self.clear_filter();
        }
        // Cancelling a cell/column edit returns to the table viewer.
        self.mode = if matches!(kind, InputKind::CsvCell | InputKind::CsvColumn) {
            Mode::Sheet
        } else {
            Mode::Normal
        };
    }

    // -- delete ------------------------------------------------------------

    pub fn confirm_delete(&mut self) {
        if self.selected_entry().is_some() {
            self.mode = Mode::ConfirmDelete;
        }
    }

    pub fn do_delete(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        self.mode = Mode::Normal;
        match fs::delete_entry(&entry.path) {
            Ok(()) => {
                self.status = format!("Deleted {}", entry.name);
                self.refresh();
            }
            Err(e) => self.status = format!("Could not delete {}: {e}", entry.name),
        }
    }

    // -- clipboard ---------------------------------------------------------

    /// Copy the selected entry's absolute path to the OS clipboard.
    pub fn copy_path(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let path = entry.path.to_string_lossy().into_owned();
        if fs::copy_to_clipboard(&path) {
            self.status = format!("Copied path: {path}");
        } else {
            // No OS clipboard tool: still show the path so it can be copied by hand.
            self.status = format!("Path (clipboard unavailable): {path}");
        }
    }

    // -- preview -----------------------------------------------------------

    /// Preview for the selected entry, cached per path.
    pub fn preview(&mut self) -> &Preview {
        let sel = self.selected_entry().map(|e| (e.path.clone(), e.is_dir));
        let Some((path, is_dir)) = sel else {
            static EMPTY: Preview = Preview::Text(String::new());
            return &EMPTY;
        };
        if self.preview_cache.0 != path || self.preview_cache.1 != is_dir {
            let preview = fs::read_preview(&path, is_dir);
            self.preview_cache = (path, is_dir, preview);
        }
        &self.preview_cache.2
    }
}

// -- tabs ------------------------------------------------------------------

/// One tab: either a file browser or an interactive shell.
pub enum Tab {
    Browser(App),
    Shell(ShellTab),
}

/// The whole application: the tab strip plus state shared across tabs
/// (theme, clipboard, quit flag).
pub struct Workspace {
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// File-operation clipboard, shared across tabs: (source path, is_cut).
    pub clipboard: Option<(PathBuf, bool)>,
    /// Index into `theme::THEMES`; synced into every browser tab.
    pub theme_idx: usize,
    /// Editor line-number gutter toggle (persisted); synced into every tab.
    pub show_line_numbers: bool,
    pub should_quit: bool,
    /// Tab-bar hit ranges recorded during render: (x_start, x_end, tab).
    pub tab_hits: Vec<(u16, u16, usize)>,
}

impl Workspace {
    pub fn new(cwd: PathBuf) -> Self {
        let (theme_idx, show_line_numbers) = theme::load_settings();
        let theme_idx = theme_idx.min(theme::THEMES.len().saturating_sub(1));
        let mut app = App::new(cwd);
        app.theme_idx = theme_idx;
        app.show_line_numbers = show_line_numbers;
        Self {
            tabs: vec![Tab::Browser(app)],
            active: 0,
            clipboard: None,
            theme_idx,
            show_line_numbers,
            should_quit: false,
            tab_hits: Vec::new(),
        }
    }

    /// The active color theme (copy; cheap).
    pub fn theme(&self) -> theme::Theme {
        theme::THEMES[self.theme_idx % theme::THEMES.len()]
    }

    /// Cycle to the next theme, sync it into every tab, and persist it.
    pub fn cycle_theme(&mut self) {
        self.theme_idx = (self.theme_idx + 1) % theme::THEMES.len();
        theme::save_settings(self.theme_idx, self.show_line_numbers);
        for tab in &mut self.tabs {
            if let Tab::Browser(app) = tab {
                app.theme_idx = self.theme_idx;
            }
        }
        let name = self.theme().name;
        if let Some(app) = self.active_browser_mut() {
            app.status = format!("Theme: {name}");
        }
    }

    /// Toggle the editor line-number gutter everywhere and persist it.
    pub fn toggle_line_numbers(&mut self) {
        self.show_line_numbers = !self.show_line_numbers;
        theme::save_settings(self.theme_idx, self.show_line_numbers);
        for tab in &mut self.tabs {
            if let Tab::Browser(app) = tab {
                app.show_line_numbers = self.show_line_numbers;
                if let Some(ed) = app.editor.as_mut() {
                    ed.show_line_numbers = self.show_line_numbers;
                }
            }
        }
        let on = self.show_line_numbers;
        if let Some(app) = self.active_browser_mut() {
            app.status = format!("Line numbers: {}", if on { "on" } else { "off" });
        }
    }

    pub fn active_browser(&self) -> Option<&App> {
        match self.tabs.get(self.active) {
            Some(Tab::Browser(app)) => Some(app),
            _ => None,
        }
    }

    pub fn active_browser_mut(&mut self) -> Option<&mut App> {
        match self.tabs.get_mut(self.active) {
            Some(Tab::Browser(app)) => Some(app),
            _ => None,
        }
    }

    pub fn is_shell_active(&self) -> bool {
        matches!(self.tabs.get(self.active), Some(Tab::Shell(_)))
    }

    /// Directory a new tab should start in: the active browser's cwd,
    /// or the process cwd when a shell tab is active.
    fn new_tab_cwd(&self) -> PathBuf {
        if let Some(app) = self.active_browser() {
            return app.cwd.clone();
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    fn push_browser_tab(&mut self, cwd: PathBuf) {
        let mut app = App::new(cwd);
        app.theme_idx = self.theme_idx;
        app.show_line_numbers = self.show_line_numbers;
        self.tabs.push(Tab::Browser(app));
        self.active = self.tabs.len() - 1;
    }

    /// Open a new file-browser tab in the current directory.
    pub fn new_browser_tab(&mut self) {
        let cwd = self.new_tab_cwd();
        self.push_browser_tab(cwd);
    }

    /// Open a new interactive shell tab in the current directory.
    /// The pty is sized properly on the first render.
    pub fn new_shell_tab(&mut self) {
        let cwd = self.new_tab_cwd();
        match ShellTab::spawn(80, 24, &cwd) {
            Ok(tab) => {
                self.tabs.push(Tab::Shell(tab));
                self.active = self.tabs.len() - 1;
            }
            Err(e) => {
                if let Some(app) = self.active_browser_mut() {
                    app.status = format!("Could not open terminal: {e}");
                }
            }
        }
    }

    /// Close the active tab. The last tab cannot be closed.
    pub fn close_active_tab(&mut self) {
        if self.tabs.len() <= 1 {
            if let Some(app) = self.active_browser_mut() {
                app.status = String::from("Cannot close the last tab");
            }
            return;
        }
        self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
    }

    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    pub fn goto_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    /// Short label for the tab strip.
    pub fn tab_title(&self, i: usize) -> String {
        match self.tabs.get(i) {
            Some(Tab::Browser(app)) => app
                .cwd
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| app.cwd.to_string_lossy().into_owned()),
            Some(Tab::Shell(sh)) => {
                if sh.state.exited {
                    String::from("shell (exited)")
                } else {
                    sh.state.title.clone()
                }
            }
            None => String::new(),
        }
    }

    /// Tab-management keys, active in every mode — including shell tabs,
    /// where every other key goes to the shell instead. Returns true when
    /// the key was consumed.
    pub fn handle_tab_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if ctrl && !alt {
            match key.code {
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    self.new_browser_tab();
                    return true;
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    self.close_active_tab();
                    return true;
                }
                KeyCode::PageUp => {
                    self.prev_tab();
                    return true;
                }
                KeyCode::PageDown => {
                    self.next_tab();
                    return true;
                }
                _ => {}
            }
        }
        if alt && !ctrl {
            if let KeyCode::Char(c) = key.code {
                if ('1'..='9').contains(&c) {
                    self.goto_tab((c as usize) - ('1' as usize));
                    return true;
                }
            }
        }
        false
    }

    /// Forward a key to the active shell tab as terminal input.
    pub fn handle_shell_key(&mut self, key: KeyEvent) {
        let bytes = crate::shell::key_to_bytes(&key);
        if let Some(Tab::Shell(sh)) = self.tabs.get_mut(self.active) {
            if let Some(b) = bytes {
                sh.send(&b);
            }
        }
    }

    /// Drain pty output from every shell tab into its parser.
    /// Returns true if any tab produced new screen content.
    pub fn poll_shell(&mut self) -> bool {
        let mut changed = false;
        for tab in &mut self.tabs {
            if let Tab::Shell(sh) = tab {
                changed |= sh.poll();
            }
        }
        changed
    }

    /// Resize every shell tab's pty. `rows` excludes the tab bar.
    pub fn resize_shell_tabs(&mut self, cols: u16, rows: u16) {
        for tab in &mut self.tabs {
            if let Tab::Shell(sh) = tab {
                sh.resize(cols, rows);
            }
        }
    }

    /// Auto-refresh the active browser tab when its directory changed.
    /// Returns true when a refresh happened.
    pub fn poll_external_changes(&mut self) -> bool {
        if let Some(app) = self.active_browser_mut() {
            app.poll_external_changes()
        } else {
            false
        }
    }

    // -- shared clipboard --------------------------------------------------

    pub fn yank(&mut self, cut: bool) {
        let entry = self
            .active_browser()
            .and_then(|a| a.selected_entry().cloned());
        if let Some(entry) = entry {
            self.clipboard = Some((entry.path.clone(), cut));
            if let Some(app) = self.active_browser_mut() {
                app.status = format!("{} {}", if cut { "Cut" } else { "Copied" }, entry.name);
            }
        }
    }

    pub fn paste(&mut self) {
        let Some((src, is_cut)) = self.clipboard.clone() else {
            if let Some(app) = self.active_browser_mut() {
                app.status = String::from("Clipboard is empty");
            }
            return;
        };
        let fallback_name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let cwd = match self.active_browser() {
            Some(app) => app.cwd.clone(),
            None => return,
        };
        let result = if is_cut {
            fs::move_entry(&src, &cwd)
        } else {
            fs::copy_entry(&src, &cwd)
        };
        match result {
            Ok(dst) => {
                let name = dst
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(fallback_name);
                if is_cut {
                    self.clipboard = None;
                }
                if let Some(app) = self.active_browser_mut() {
                    app.status = format!("Pasted {name}");
                    app.refresh();
                }
            }
            Err(e) => {
                if let Some(app) = self.active_browser_mut() {
                    app.status = format!("Paste failed: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_workspace() -> (Workspace, PathBuf) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fex-wstest-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
        (Workspace::new(dir.clone()), dir)
    }

    #[test]
    fn tab_open_switch_close() {
        let (mut ws, dir) = test_workspace();
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.active, 0);
        // New browser tab opens in the same directory and becomes active.
        ws.new_browser_tab();
        assert_eq!(ws.tabs.len(), 2);
        assert_eq!(ws.active, 1);
        assert!(ws.active_browser().is_some());
        // Switch back and forth.
        ws.prev_tab();
        assert_eq!(ws.active, 0);
        ws.next_tab();
        assert_eq!(ws.active, 1);
        ws.next_tab();
        assert_eq!(ws.active, 0, "tabs wrap around");
        ws.goto_tab(1);
        assert_eq!(ws.active, 1);
        ws.goto_tab(99);
        assert_eq!(ws.active, 1, "out-of-range jump is ignored");
        // Closing the active tab activates its neighbor.
        ws.close_active_tab();
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.active, 0);
        // The last tab cannot be closed.
        ws.close_active_tab();
        assert_eq!(ws.tabs.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tab_keys_and_shared_clipboard() {
        let (mut ws, dir) = test_workspace();
        let key = |code: KeyCode, mods: KeyModifiers| KeyEvent {
            code,
            modifiers: mods,
            kind: ratatui::crossterm::event::KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        };
        let ctrl = KeyModifiers::CONTROL;
        // Ctrl+T opens a tab, Ctrl+W closes it.
        assert!(ws.handle_tab_key(key(KeyCode::Char('t'), ctrl)));
        assert_eq!(ws.tabs.len(), 2);
        assert!(ws.handle_tab_key(key(KeyCode::Char('w'), ctrl)));
        assert_eq!(ws.tabs.len(), 1);
        // Alt+2 jumps to the second tab.
        ws.handle_tab_key(key(KeyCode::Char('t'), ctrl));
        assert!(ws.handle_tab_key(key(KeyCode::Char('2'), KeyModifiers::ALT)));
        assert_eq!(ws.active, 1);
        // Plain keys are not consumed.
        assert!(!ws.handle_tab_key(key(KeyCode::Char('q'), KeyModifiers::NONE)));
        // Clipboard is shared: yank in tab 0, paste in tab 1.
        ws.goto_tab(0);
        ws.yank(false);
        assert!(ws.clipboard.is_some());
        ws.goto_tab(1);
        ws.paste();
        let app = ws.active_browser().unwrap();
        assert!(
            app.status.starts_with("Pasted "),
            "status was {:?}",
            app.status
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn theme_change_syncs_all_tabs() {
        let (mut ws, dir) = test_workspace();
        ws.new_browser_tab();
        let first = ws.theme_idx;
        ws.cycle_theme();
        assert_ne!(ws.theme_idx, first);
        for tab in &ws.tabs {
            if let Tab::Browser(app) = tab {
                assert_eq!(app.theme_idx, ws.theme_idx);
            }
        }
        let on = ws.show_line_numbers;
        ws.toggle_line_numbers();
        assert_ne!(ws.show_line_numbers, on);
        for tab in &ws.tabs {
            if let Tab::Browser(app) = tab {
                assert_eq!(app.show_line_numbers, ws.show_line_numbers);
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
