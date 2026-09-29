//! Application state: directory listing, selection, filter, sort, UI modes,
//! and the multi-tab workspace (file-browser tabs plus terminal tabs).

use crate::editor::Editor;
use crate::fs::{self, Entry, Preview, SearchResult};
use crate::net::{self, Drive, NetDevice};
use crate::session;
use crate::sheet::{self, Sheet};
use crate::shell::ShellTab;
use crate::theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::cmp::Ordering;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Type,
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
            (SortKey::Type, true) => "Type ↑",
            (SortKey::Type, false) => "Type ↓",
        }
    }

    fn cycle_key(&mut self) {
        self.key = match self.key {
            SortKey::Name => SortKey::Size,
            SortKey::Size => SortKey::Modified,
            SortKey::Modified => SortKey::Type,
            SortKey::Type => SortKey::Name,
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
    /// Manual network host (IP or hostname) in the Network view.
    NetHost,
    /// Share name typed by hand when listing shares fails.
    NetShare,
    /// SMB username for a share that needs credentials.
    SmbUser,
    /// SMB password for a share that needs credentials.
    SmbPass,
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
            InputKind::NetHost => "Network host (IP or name)",
            InputKind::NetShare => "Share name",
            InputKind::SmbUser => "Username",
            InputKind::SmbPass => "Password",
        }
    }

    /// True for the password prompt, whose text is masked on screen.
    pub fn is_secret(&self) -> bool {
        matches!(self, InputKind::SmbPass)
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
    /// Network device browser (mDNS discovery + SMB mounting).
    Network,
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

/// Lowercase file extension used by the Type sort, e.g. "rs".
/// Files without an extension (and dotfiles like `.gitignore`) sort as "".
fn file_ext(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
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
        // Folders are NOT forced to the top: only the Type key groups.
        let ord = match sort.key {
            SortKey::Name => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
            SortKey::Size => ea.size.cmp(&eb.size),
            SortKey::Modified => ea.modified.cmp(&eb.modified),
            SortKey::Type => {
                // Group: directories first, then files grouped by extension;
                // alphabetical by name inside each group.
                match (ea.is_dir, eb.is_dir) {
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (true, true) => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
                    (false, false) => file_ext(&ea.name)
                        .cmp(&file_ext(&eb.name))
                        .then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase())),
                }
            }
        };
        if sort.ascending {
            ord
        } else {
            ord.reverse()
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
    /// Save-as dialog for never-saved documents (None when closed).
    pub save_dialog: Option<SaveDialog>,
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
    /// Rendered Miller columns as (column index, rect), for mouse clicks.
    pub col_hits: Vec<(usize, Rect)>,
    /// Last mouse click (x, y, time), for double-click detection.
    pub last_click: Option<(u16, u16, Instant)>,
    /// Index into `theme::THEMES`.
    pub theme_idx: usize,
    /// Editor line-number gutter toggle (persisted).
    pub show_line_numbers: bool,
    /// Preview pane visibility in list view (`P` toggles; off by default).
    pub show_preview: bool,
    /// Network device browser (`G`); None when closed.
    pub net: Option<NetView>,
    /// Mount point owned by this tab (macOS); unmounted on tab close.
    pub mount_point: Option<PathBuf>,
    /// Display name for a network tab, e.g. `//nas/Media`.
    pub network_name: Option<String>,
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
                key: SortKey::Modified,
                ascending: false,
            },
            mode: Mode::Normal,
            input: String::new(),
            cursor: 0,
            status: String::new(),
            preview_cache: (PathBuf::new(), false, Preview::Text(String::new())),
            editor: None,
            save_dialog: None,
            view: ViewMode::List,
            columns: Vec::new(),
            col_active: 0,
            search_results: Vec::new(),
            search_selected: 0,
            search_deep: false,
            sheet: None,
            dir_mtime: None,
            list_origin: (0, 0),
            col_hits: Vec::new(),
            last_click: None,
            theme_idx: 0,
            show_line_numbers: true,
            show_preview: false,
            net: None,
            mount_point: None,
            network_name: None,
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

    /// Select the visible entry named `name`, if it's there. Used by
    /// session restore and favorites jumps.
    pub fn select_by_name(&mut self, name: &str) {
        if let Some(pos) = self
            .visible
            .iter()
            .position(|&i| self.entries[i].name == name)
        {
            self.selected = pos;
            self.list_state.select(Some(pos));
        }
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

    /// Row (visible index) of the shown entry named `name`, if present.
    /// Test helper (also handy for future mouse/click logic).
    #[cfg(test)]
    pub fn visible_row(&self, name: &str) -> Option<usize> {
        self.visible
            .iter()
            .position(|&i| self.entries[i].name == name)
    }

    /// Select the file-list row under a mouse click at terminal (x, y).
    /// Clicks outside the list are ignored.
    pub fn click_select(&mut self, x: u16, y: u16) {
        if self.view == ViewMode::Columns {
            self.click_select_column(x, y);
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

    /// Select the Miller-column row under a mouse click. Clicking a row in
    /// an earlier column activates that column (like keyboard-left), then
    /// selects the row and rebuilds the trailing columns.
    fn click_select_column(&mut self, x: u16, y: u16) {
        let hits = self.col_hits.clone();
        for (idx, rect) in hits {
            if x < rect.x || x >= rect.x + rect.width || y <= rect.y || y >= rect.y + rect.height {
                continue;
            }
            if idx > self.col_active {
                return; // preview panel, not a real column
            }
            let j = (y - rect.y - 1) as usize;
            let len = self.columns.get(idx).map(|c| c.entries.len()).unwrap_or(0);
            if j >= len {
                return;
            }
            self.col_active = idx;
            if let Some(col) = self.columns.get_mut(idx) {
                col.selected = j;
            }
            self.cwd = self.columns[idx].path.clone();
            self.sync_columns();
            return;
        }
    }

    /// Double-click a file-list row: select it, then open it like Enter.
    pub fn dblclick_open(&mut self, x: u16, y: u16) {
        self.click_select(x, y);
        self.enter_selected();
    }

    /// Show the selected file/folder in the OS file explorer (Finder on
    /// macOS, Explorer on Windows, the default file manager on Linux).
    pub fn reveal_in_explorer(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            self.status = String::from("Nothing to reveal");
            return;
        };
        match fs::reveal_in_explorer(&entry.path) {
            Ok(()) => {
                self.status = format!("Revealed {} in the file explorer", entry.name);
            }
            Err(e) => {
                self.status = format!("Could not open the file explorer: {e}");
            }
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
        self.save_dialog = None;
        self.mode = Mode::Normal;
        self.refresh(); // the file may have changed on disk
    }

    /// Open the save-as dialog for a never-saved document, starting in the
    /// tab's current directory.
    pub fn open_save_dialog(&mut self) {
        let cwd = self.cwd.clone();
        self.save_dialog = Some(SaveDialog::new(cwd));
    }

    /// Open the Network view: live mDNS discovery of SMB devices.
    pub fn open_network(&mut self) {
        match NetView::new() {
            Ok(nv) => {
                self.net = Some(nv);
                self.mode = Mode::Network;
            }
            Err(e) => self.status = format!("Network discovery unavailable: {e}"),
        }
    }

    /// Close the Network view (stops discovery).
    pub fn close_network(&mut self) {
        self.net = None;
        self.mode = Mode::Normal;
    }

    /// Enter on the network selection: list shares or mount one.
    pub fn net_enter(&mut self) {
        if let Some(nv) = self.net.as_mut() {
            nv.enter();
        }
    }

    /// Back out of the network view (or close it at the top level).
    pub fn net_back(&mut self) {
        let close = self.net.as_mut().is_some_and(|nv| nv.back());
        if close {
            self.close_network();
        }
    }

    /// Drain network worker messages; returns mounts ready to open.
    pub fn poll_net(&mut self) -> (bool, Vec<(String, String, net::MountedShare)>) {
        let mut changed = false;
        let mut mounts = Vec::new();
        if let Some(nv) = self.net.as_mut() {
            let (c, msgs) = nv.poll();
            changed |= c;
            for m in msgs {
                if let Some((host, share, mounted)) = nv.apply_msg(m) {
                    mounts.push((host, share, mounted));
                }
            }
        }
        (changed, mounts)
    }

    /// Route a key to the open save-as dialog, carrying out saves.
    pub fn save_dialog_key(&mut self, key: KeyEvent) {
        let action = match self.save_dialog.as_mut() {
            Some(d) => d.key(key),
            None => return,
        };
        match action {
            SaveAction::None => {}
            SaveAction::Cancel => {
                self.save_dialog = None;
            }
            SaveAction::Save(target) => {
                // A typed sub-path ("notes/todo.txt") may name folders
                // that don't exist yet — create them.
                if let Some(parent) = target.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        if let Some(d) = self.save_dialog.as_mut() {
                            d.message = format!("Cannot create folder: {e}");
                        }
                        return;
                    }
                }
                match self.editor.as_mut().map(|ed| ed.save_to(&target)) {
                    Some(Ok(())) => {
                        self.save_dialog = None;
                        self.refresh(); // the new file appears in the listing
                    }
                    Some(Err(e)) => {
                        if let Some(d) = self.save_dialog.as_mut() {
                            d.message = format!("Save failed: {e}");
                        }
                    }
                    None => {
                        self.save_dialog = None;
                    }
                }
            }
        }
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

    /// Toggle the preview pane in list view / Miller columns.
    pub fn toggle_preview(&mut self) {
        self.show_preview = !self.show_preview;
        self.status = if self.show_preview {
            String::from("Preview on")
        } else {
            String::from("Preview off")
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
            InputKind::NetHost | InputKind::NetShare | InputKind::SmbUser | InputKind::SmbPass => {}
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
            InputKind::NetHost => {
                let host = value.clone();
                self.mode = Mode::Network;
                if let Some(nv) = self.net.as_mut() {
                    nv.add_manual_host(&host);
                }
            }
            InputKind::NetShare => {
                self.mode = Mode::Network;
                if let Some(nv) = self.net.as_mut() {
                    nv.submit_manual_share(&value);
                }
            }
            InputKind::SmbUser => {
                // Chain straight into the password prompt.
                if let Some(nv) = self.net.as_mut() {
                    nv.submit_credentials(InputKind::SmbUser, &value);
                }
                self.input.clear();
                self.cursor = 0;
                self.mode = Mode::Input(InputKind::SmbPass);
            }
            InputKind::SmbPass => {
                self.mode = Mode::Network;
                if let Some(nv) = self.net.as_mut() {
                    nv.submit_credentials(InputKind::SmbPass, &value);
                }
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
        // Cancelling a cell/column edit returns to the table viewer;
        // cancelling a network prompt returns to the Network view.
        self.mode = if matches!(kind, InputKind::CsvCell | InputKind::CsvColumn) {
            Mode::Sheet
        } else if matches!(
            kind,
            InputKind::NetHost | InputKind::NetShare | InputKind::SmbUser | InputKind::SmbPass
        ) {
            Mode::Network
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
    /// Ctrl+G was just pressed: the next key jumps tabs (macOS-friendly
    /// alternative to Alt+1-9, since macOS keyboards have no Alt key).
    pub goto_pending: bool,
    /// Favorite files/folders (global, persisted to `~/.config/fex/favorites`).
    pub favorites: Vec<PathBuf>,
    /// The favorites popup is open. Modal: it eats every key until closed.
    pub show_favorites: bool,
    /// Selected row in the favorites popup.
    pub fav_sel: usize,
    /// Base dir for session/favorites persistence. `None` skips all disk I/O
    /// (tests pass a temp dir so they never touch the real `~/.config`).
    pub config_dir: Option<PathBuf>,
}

/// Max characters shown in a tab title; longer names get an ellipsis so
/// one long file name can't crowd the whole tab strip.
const TAB_TITLE_CHARS: usize = 24;

fn short_tab_title(name: &str) -> String {
    if name.chars().count() <= TAB_TITLE_CHARS {
        return name.to_owned();
    }
    let mut s: String = name.chars().take(TAB_TITLE_CHARS - 1).collect();
    s.push('…');
    s
}

/// What the save-as dialog wants the app to do after a key.
pub enum SaveAction {
    None,
    Cancel,
    Save(PathBuf),
}

/// A save-as dialog: pick a directory by browsing, type a file name.
/// Opened by Ctrl+S on a never-saved ("untitled") document.
pub struct SaveDialog {
    pub cwd: PathBuf,
    pub dirs: Vec<PathBuf>,
    pub selected: usize,
    pub filename: String,
    /// Char index of the text cursor inside `filename`.
    pub fcursor: usize,
    pub message: String,
    confirm_overwrite: bool,
}

impl SaveDialog {
    pub fn new(cwd: PathBuf) -> Self {
        let mut dlg = Self {
            cwd,
            dirs: Vec::new(),
            selected: 0,
            filename: String::new(),
            fcursor: 0,
            message: String::new(),
            confirm_overwrite: false,
        };
        dlg.read_dirs();
        dlg
    }

    fn read_dirs(&mut self) {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.cwd) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                }
            }
        }
        dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()).then_with(|| a.cmp(b)));
        self.dirs = dirs;
        self.selected = 0;
    }

    fn move_selection(&mut self, delta: i32) {
        if self.dirs.is_empty() {
            return;
        }
        let n = self.dirs.len() as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(n) as usize;
    }

    fn enter_selected(&mut self) {
        if let Some(d) = self.dirs.get(self.selected).cloned() {
            self.cwd = d;
            self.read_dirs();
        }
    }

    fn go_parent(&mut self) {
        if let Some(p) = self.cwd.parent() {
            self.cwd = p.to_path_buf();
            self.read_dirs();
        }
    }

    /// The full path that would be written, or None when no name typed.
    pub fn target(&self) -> Option<PathBuf> {
        if self.filename.is_empty() {
            None
        } else {
            Some(self.cwd.join(&self.filename))
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> SaveAction {
        // Any edit to the name restarts the overwrite confirmation.
        let name_edit = matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace);
        match key.code {
            KeyCode::Esc => return SaveAction::Cancel,
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Right => self.enter_selected(),
            KeyCode::Left => self.go_parent(),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => {
                self.selected = self.dirs.len().saturating_sub(1);
            }
            KeyCode::Backspace => {
                if self.fcursor > 0 {
                    self.fcursor -= 1;
                    let at = char_index_to_byte(&self.filename, self.fcursor);
                    self.filename.remove(at);
                }
            }
            KeyCode::Char(c) => {
                let at = char_index_to_byte(&self.filename, self.fcursor);
                self.filename.insert(at, c);
                self.fcursor += 1;
            }
            KeyCode::Enter => {
                let Some(target) = self.target() else {
                    self.message = String::from("Type a file name first");
                    return SaveAction::None;
                };
                if target.exists() && !self.confirm_overwrite {
                    self.confirm_overwrite = true;
                    self.message = format!(
                        "\"{}\" exists — Enter again to overwrite, Esc to cancel",
                        self.filename
                    );
                    return SaveAction::None;
                }
                return SaveAction::Save(target);
            }
            _ => {}
        }
        if name_edit {
            self.confirm_overwrite = false;
            self.message.clear();
        }
        SaveAction::None
    }
}

/// Messages from network worker threads to the UI.
pub enum NetMsg {
    Shares {
        host: String,
        result: io::Result<Vec<String>>,
    },
    Mounted {
        host: String,
        share: String,
        result: io::Result<net::MountedShare>,
    },
}

/// Where the Network view is: the device list, loading a host's shares,
/// the share list, or mounting a share.
pub enum NetState {
    Devices,
    LoadingShares { host: String },
    Shares { host: String, shares: Vec<String> },
    Mounting { host: String, share: String },
}

/// The two sections of the combined Drives & network list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetSection {
    Drives,
    /// Windows mapped network drives (File Explorer's connected locations).
    Mapped,
    Network,
}

impl NetSection {
    pub fn label(self) -> &'static str {
        match self {
            NetSection::Drives => "Drives",
            NetSection::Mapped => "Mapped network drives",
            NetSection::Network => "Network",
        }
    }
}

/// One row of the combined list. Headers are separator lines and are
/// never selectable; `Hint` is the empty-network-devices note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetRow {
    Header(NetSection),
    Drive(usize),
    Mapped(usize),
    Device(usize),
    Hint,
}

impl NetRow {
    fn selectable(self) -> bool {
        matches!(
            self,
            NetRow::Drive(_) | NetRow::Mapped(_) | NetRow::Device(_)
        )
    }
}

/// Normalize a drive root for comparison: `Z:\` and `z:` are the same.
fn norm_drive(s: &str) -> String {
    s.trim_end_matches(['\\', '/']).to_lowercase()
}

/// The Network view (`G`): local drives plus live mDNS device list and
/// SMB share browsing. Dropping it stops discovery.
pub struct NetView {
    /// Local drives/volumes, loaded once when the view opens.
    pub drives: Vec<Drive>,
    /// Windows mapped network drives (already connected; no mounting
    /// needed). Empty on other platforms.
    pub mapped: Vec<net::MappedDrive>,
    /// Discovered + hand-added devices, merged and sorted.
    pub devices: Vec<NetDevice>,
    manual: Vec<NetDevice>,
    /// Index into [`NetView::rows`] (headers/hints included).
    pub selected: usize,
    pub state: NetState,
    pub message: String,
    /// Hosts with a share currently mounted in some tab, for tagging.
    pub mounted_hosts: Vec<String>,
    tx: mpsc::Sender<NetMsg>,
    rx: mpsc::Receiver<NetMsg>,
    discovery: net::Discovery,
    /// Mount waiting on credentials: (host, share, user).
    pending_auth: Option<(String, String, String)>,
    /// Host waiting on a hand-typed share name.
    pending_host: Option<String>,
    /// Input prompt the workspace should open next (set by worker
    /// messages, drained once).
    next_prompt: Option<InputKind>,
}

impl NetView {
    pub fn new() -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mapped = net::list_mapped_drives();
        // On Windows the disk list also reports mapped network drives;
        // drop those from Drives so each appears once (with its UNC path,
        // under "Mapped network drives").
        let mut drives = net::list_drives();
        if !mapped.is_empty() {
            let letters: Vec<String> = mapped.iter().map(|m| norm_drive(&m.local)).collect();
            drives.retain(|d| !letters.contains(&norm_drive(&d.mount_point.to_string_lossy())));
        }
        let mut nv = Self {
            drives,
            mapped,
            devices: Vec::new(),
            manual: Vec::new(),
            selected: 0,
            state: NetState::Devices,
            message: String::from("Scanning for devices…"),
            mounted_hosts: Vec::new(),
            tx,
            rx,
            discovery: net::Discovery::start()?,
            pending_auth: None,
            pending_host: None,
            next_prompt: None,
        };
        nv.selected = nv.first_selectable();
        Ok(nv)
    }

    /// Flat rows of the combined list: a "Drives" separator, the drives,
    /// a "Mapped network drives" separator (Windows only, when any are
    /// mapped), a "Network" separator, then the devices (or a hint when
    /// empty).
    pub fn rows(&self) -> Vec<NetRow> {
        let mut rows = vec![NetRow::Header(NetSection::Drives)];
        rows.extend((0..self.drives.len()).map(NetRow::Drive));
        if !self.mapped.is_empty() {
            rows.push(NetRow::Header(NetSection::Mapped));
            rows.extend((0..self.mapped.len()).map(NetRow::Mapped));
        }
        rows.push(NetRow::Header(NetSection::Network));
        if self.devices.is_empty() {
            rows.push(NetRow::Hint);
        } else {
            rows.extend((0..self.devices.len()).map(NetRow::Device));
        }
        rows
    }

    /// Index of the first selectable row (never a header).
    fn first_selectable(&self) -> usize {
        self.rows().iter().position(|r| r.selectable()).unwrap_or(0)
    }

    /// Step `selected` to a selectable row, wrapping around.
    fn step_selection(&mut self, delta: i32) {
        let rows = self.rows();
        if !rows.iter().any(|r| r.selectable()) {
            return;
        }
        let n = rows.len();
        let mut i = self.selected as i32;
        loop {
            i = (i + delta).rem_euclid(n as i32);
            if rows[i as usize].selectable() {
                self.selected = i as usize;
                break;
            }
        }
    }

    /// Jump to the i-th selectable row.
    fn jump_to_selectable(&mut self, i: usize) {
        let rows = self.rows();
        let selectable: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selectable())
            .map(|(idx, _)| idx)
            .collect();
        if !selectable.is_empty() {
            self.selected = selectable[i.min(selectable.len() - 1)];
        }
    }

    /// Return to the top-level list, resetting the selection past the
    /// section headers.
    fn to_devices(&mut self) {
        self.state = NetState::Devices;
        self.selected = self.first_selectable();
    }

    /// Drain discovery snapshots and worker messages. Returns
    /// (changed, worker messages for the workspace to act on).
    pub fn poll(&mut self) -> (bool, Vec<NetMsg>) {
        let mut changed = false;
        if let Some(found) = self.discovery.poll() {
            let mut merged = found;
            merged.extend(self.manual.iter().cloned());
            merged.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            merged.dedup_by(|a, b| a.host == b.host);
            if merged != self.devices {
                self.devices = merged;
                // Keep the selection if it still lands on a selectable
                // row; otherwise fall back to the first one.
                let rows = self.rows();
                if self.selected >= rows.len() || !rows[self.selected].selectable() {
                    self.selected = self.first_selectable();
                }
                self.message.clear();
                changed = true;
            }
        }
        let mut msgs = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            msgs.push(m);
            changed = true;
        }
        (changed, msgs)
    }

    pub fn move_selection(&mut self, delta: i32) {
        match self.state {
            NetState::Devices => self.step_selection(delta),
            NetState::Shares { ref shares, .. } => {
                if shares.is_empty() {
                    return;
                }
                self.selected =
                    (self.selected as i32 + delta).rem_euclid(shares.len() as i32) as usize;
            }
            _ => return,
        }
        self.message.clear();
    }

    pub fn jump_to(&mut self, i: usize) {
        match self.state {
            NetState::Devices => self.jump_to_selectable(i),
            NetState::Shares { ref shares, .. } => {
                if !shares.is_empty() {
                    self.selected = i.min(shares.len() - 1);
                }
            }
            _ => return,
        }
        self.message.clear();
    }

    fn selected_device(&self) -> Option<&NetDevice> {
        match self.rows().get(self.selected) {
            Some(NetRow::Device(i)) => self.devices.get(*i),
            _ => None,
        }
    }

    /// The drive under the cursor, if the Devices list is on a drive row.
    /// Opening it as a tab is handled by the input layer.
    pub fn selected_drive(&self) -> Option<&Drive> {
        if !matches!(self.state, NetState::Devices) {
            return None;
        }
        match self.rows().get(self.selected) {
            Some(NetRow::Drive(i)) => self.drives.get(*i),
            _ => None,
        }
    }

    /// The mapped network drive under the cursor, if the list is on one of
    /// those rows. Already connected, so the input layer opens it directly.
    pub fn selected_mapped(&self) -> Option<&net::MappedDrive> {
        if !matches!(self.state, NetState::Devices) {
            return None;
        }
        match self.rows().get(self.selected) {
            Some(NetRow::Mapped(i)) => self.mapped.get(*i),
            _ => None,
        }
    }

    fn selected_share(&self) -> Option<(String, String)> {
        match &self.state {
            NetState::Shares { host, shares } => {
                shares.get(self.selected).map(|s| (host.clone(), s.clone()))
            }
            _ => None,
        }
    }

    /// Enter on the selection: list a device's shares, or mount a share.
    /// Drives and mapped network drives are opened by the input layer (see
    /// `selected_drive` / `selected_mapped`); Enter here ignores them.
    pub fn enter(&mut self) {
        match self.state {
            NetState::Devices => {
                let Some(dev) = self.selected_device() else {
                    return;
                };
                let host = dev.host.clone();
                self.state = NetState::LoadingShares { host: host.clone() };
                self.message = format!("Listing shares on {host}…");
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let result = net::list_shares(&host);
                    let _ = tx.send(NetMsg::Shares { host, result });
                });
            }
            NetState::Shares { .. } => {
                let Some((host, share)) = self.selected_share() else {
                    return;
                };
                self.start_mount(host, share, String::new(), String::new());
            }
            _ => {}
        }
    }

    /// Go back a level. Returns true when the view should close.
    pub fn back(&mut self) -> bool {
        match self.state {
            NetState::Devices => true,
            _ => {
                // A late worker reply for the abandoned level is ignored by
                // matching on the host (see apply_msg).
                self.to_devices();
                self.message.clear();
                false
            }
        }
    }

    pub fn add_manual_host(&mut self, host: &str) {
        let host = host.trim().to_string();
        if host.is_empty() {
            return;
        }
        if !self.manual.iter().any(|d| d.host == host) {
            self.manual.push(NetDevice::manual(&host));
            let mut merged = self.devices.clone();
            merged.push(NetDevice::manual(&host));
            merged.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            self.devices = merged;
        }
        self.message.clear();
    }

    /// Apply a worker message. Returns a mount ready to open, if any.
    pub fn apply_msg(&mut self, msg: NetMsg) -> Option<(String, String, net::MountedShare)> {
        match msg {
            NetMsg::Shares { host, result } => {
                // Ignore stale replies after backing out.
                if !matches!(&self.state, NetState::LoadingShares { host: h } if h == &host) {
                    return None;
                }
                match result {
                    Ok(shares) if !shares.is_empty() => {
                        self.state = NetState::Shares { host, shares };
                        self.selected = 0;
                        self.message.clear();
                    }
                    Ok(_) => {
                        self.to_devices();
                        self.message = format!("No shares found on {host}");
                    }
                    Err(e) => {
                        // Fall back to typing the share name by hand.
                        self.pending_host = Some(host.clone());
                        self.to_devices();
                        self.message = format!("Could not list shares on {host}: {e}");
                        self.next_prompt = Some(InputKind::NetShare);
                    }
                }
                None
            }
            NetMsg::Mounted {
                host,
                share,
                result,
            } => {
                if !matches!(&self.state, NetState::Mounting { host: h, share: s } if h == &host && s == &share)
                {
                    return None;
                }
                match result {
                    Ok(mounted) => {
                        self.to_devices();
                        Some((host, share, mounted))
                    }
                    Err(e) => {
                        self.to_devices();
                        if net::is_auth_error(&e) {
                            self.pending_auth = Some((host.clone(), share.clone(), String::new()));
                            self.message = format!("{host} needs a login — enter your username");
                            self.next_prompt = Some(InputKind::SmbUser);
                        } else {
                            self.message = format!("Could not mount {host}/{share}: {e}");
                        }
                        None
                    }
                }
            }
        }
    }

    fn start_mount(&mut self, host: String, share: String, user: String, pass: String) {
        self.pending_auth = None;
        self.pending_host = None;
        self.state = NetState::Mounting {
            host: host.clone(),
            share: share.clone(),
        };
        self.message = format!("Mounting {host}/{share}…");
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = net::mount_share(&host, &share, &user, &pass);
            let _ = tx.send(NetMsg::Mounted {
                host,
                share,
                result,
            });
        });
    }

    /// The share name typed by hand after listing failed.
    pub fn submit_manual_share(&mut self, share: &str) {
        let share = share.trim().to_string();
        let Some(host) = self.pending_host.take() else {
            return;
        };
        if share.is_empty() {
            self.message = String::from("Share name cannot be empty");
            return;
        }
        self.start_mount(host, share, String::new(), String::new());
    }

    /// Credentials entered after an auth failure. `user` empty means the
    /// username prompt just completed — the password prompt is next.
    pub fn submit_credentials(&mut self, kind: InputKind, value: &str) {
        let Some((host, share, user)) = self.pending_auth.take() else {
            return;
        };
        match kind {
            InputKind::SmbUser => {
                // Next: the password prompt.
                self.pending_auth = Some((host, share, value.to_string()));
            }
            _ => {
                self.start_mount(host, share, user, value.to_string());
            }
        }
    }

    /// True when the username step just finished and the password prompt
    /// should open next.
    pub fn take_next_prompt(&mut self) -> Option<InputKind> {
        self.next_prompt.take()
    }
}

impl Workspace {
    pub fn new(cwd: PathBuf) -> Self {
        let (theme_idx, show_line_numbers) = theme::load_settings();
        let theme_idx = theme_idx.min(theme::THEMES.len().saturating_sub(1));
        let mut app = App::new(cwd);
        app.theme_idx = theme_idx;
        app.show_line_numbers = show_line_numbers;
        let mut ws = Self {
            tabs: vec![Tab::Browser(app)],
            active: 0,
            clipboard: None,
            theme_idx,
            show_line_numbers,
            should_quit: false,
            tab_hits: Vec::new(),
            goto_pending: false,
            favorites: Vec::new(),
            show_favorites: false,
            fav_sel: 0,
            config_dir: None,
        };
        ws.set_config_dir(session::config_dir());
        ws
    }

    /// Point persistence at `dir` (or disable it with `None`) and reload
    /// favorites from there. Tests pass a temp dir.
    pub fn set_config_dir(&mut self, dir: Option<PathBuf>) {
        self.config_dir = dir;
        self.favorites = self
            .config_dir
            .as_deref()
            .map(session::load_favorites_in)
            .unwrap_or_default();
    }

    /// Write the favorites file, when persistence is enabled.
    fn write_favorites(&self) {
        if let Some(dir) = self.config_dir.as_deref() {
            let _ = session::save_favorites_in(dir, &self.favorites);
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
        self.save_tabs_only();
    }

    /// Open a new tab with a blank, unsaved text document. Ctrl+S on it
    /// opens the save-as dialog to pick a location and file name.
    pub fn new_text_tab(&mut self) {
        let cwd = self.new_tab_cwd();
        let mut app = App::new(cwd);
        app.theme_idx = self.theme_idx;
        app.show_line_numbers = self.show_line_numbers;
        let mut ed = Editor::untitled();
        ed.show_line_numbers = app.show_line_numbers;
        app.editor = Some(ed);
        app.mode = Mode::Editor;
        self.tabs.push(Tab::Browser(app));
        self.active = self.tabs.len() - 1;
        self.save_tabs_only();
    }

    /// Open a browser tab on a freshly mounted network share.
    pub fn open_mounted_tab(&mut self, host: &str, share: &str, mounted: net::MountedShare) {
        let mut app = App::new(mounted.path.clone());
        app.theme_idx = self.theme_idx;
        app.show_line_numbers = self.show_line_numbers;
        app.mount_point = if mounted.needs_unmount {
            Some(mounted.path)
        } else {
            None
        };
        app.network_name = Some(format!("//{host}/{share}"));
        app.status = format!("Mounted //{host}/{share}");
        self.tabs.push(Tab::Browser(app));
        self.active = self.tabs.len() - 1;
        self.save_tabs_only();
    }

    /// Open a browser tab on a local drive picked in the Network view.
    /// Drives are never mounted by fex, so there is nothing to unmount.
    pub fn open_drive_tab(&mut self, path: &Path, name: &str) {
        let mut app = App::new(path.to_path_buf());
        app.theme_idx = self.theme_idx;
        app.show_line_numbers = self.show_line_numbers;
        app.status = format!("Opened {name}");
        self.tabs.push(Tab::Browser(app));
        self.active = self.tabs.len() - 1;
        self.save_tabs_only();
    }

    /// Drain network worker messages for the active tab's Network view.
    /// Opens mounted shares as new tabs and raises input prompts for
    /// credentials or hand-typed share names. Returns true when the UI
    /// changed.
    pub fn poll_network(&mut self) -> bool {
        let mut changed = false;
        let mut mounts: Vec<(String, String, net::MountedShare)> = Vec::new();
        let mut prompts: Vec<InputKind> = Vec::new();
        // Hosts with a share open in some tab, so the device list can tag
        // them as mounted.
        let mounted_hosts: Vec<String> = self
            .tabs
            .iter()
            .filter_map(|t| match t {
                Tab::Browser(app) => app
                    .network_name
                    .as_ref()
                    .and_then(|n| n.strip_prefix("//"))
                    .and_then(|s| s.split('/').next())
                    .map(str::to_string),
                _ => None,
            })
            .collect();
        if let Some(app) = self.active_browser_mut() {
            if let Some(nv) = app.net.as_mut() {
                if nv.mounted_hosts != mounted_hosts {
                    nv.mounted_hosts = mounted_hosts;
                    changed = true;
                }
            }
            let (c, ms) = app.poll_net();
            changed |= c;
            mounts.extend(ms);
            if let Some(nv) = app.net.as_mut() {
                if let Some(kind) = nv.take_next_prompt() {
                    prompts.push(kind);
                }
            }
        }
        for (host, share, mounted) in mounts {
            changed = true;
            self.open_mounted_tab(&host, &share, mounted);
        }
        for kind in prompts {
            if let Some(app) = self.active_browser_mut() {
                app.start_input(kind);
                changed = true;
            }
        }
        changed
    }

    /// Unmount every mounted network share (best effort, for shutdown).
    pub fn unmount_all(&mut self) {
        for tab in &self.tabs {
            if let Tab::Browser(app) = tab {
                if let Some(mp) = &app.mount_point {
                    let _ = net::unmount_path(mp);
                }
            }
        }
    }

    /// Open a new interactive shell tab in the current directory.
    /// The pty is sized properly on the first render.
    pub fn new_shell_tab(&mut self) {
        let cwd = self.new_tab_cwd();
        match ShellTab::spawn(80, 24, &cwd) {
            Ok(tab) => {
                self.tabs.push(Tab::Shell(tab));
                self.active = self.tabs.len() - 1;
                self.save_tabs_only();
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
        // Unmount network shares owned by the closing tab.
        if let Some(Tab::Browser(app)) = self.tabs.get(self.active) {
            if let Some(mp) = app.mount_point.clone() {
                let _ = net::unmount_path(&mp);
            }
        }
        // Destroy the tab's session backup: a closed tab is gone for good and
        // must not be resurrected by session restore on the next launch.
        if let Some(Tab::Browser(app)) = self.tabs.get_mut(self.active) {
            if let Some(ed) = app.editor.as_mut() {
                ed.clear_backup();
            }
        }
        self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
        self.show_favorites = false;
        self.save_tabs_only();
    }

    // ------------------------------------------------------------ favorites

    /// Toggle `path` in the favorites list. Returns true when it was added.
    /// The list is persisted immediately.
    pub fn toggle_favorite(&mut self, path: PathBuf) -> bool {
        let added = if let Some(i) = self.favorites.iter().position(|f| f == &path) {
            self.favorites.remove(i);
            false
        } else {
            self.favorites.push(path);
            true
        };
        self.write_favorites();
        added
    }

    /// Remove the favorite currently selected in the popup.
    pub fn remove_favorite_sel(&mut self) {
        if self.fav_sel < self.favorites.len() {
            self.favorites.remove(self.fav_sel);
            self.fav_sel = self.fav_sel.min(self.favorites.len().saturating_sub(1));
            self.write_favorites();
        }
    }

    /// Jump the active browser tab to the favorite selected in the popup:
    /// directories are opened, files are revealed in their parent folder.
    pub fn jump_to_favorite(&mut self) {
        let Some(path) = self.favorites.get(self.fav_sel).cloned() else {
            return;
        };
        self.show_favorites = false;
        let Some(app) = self.active_browser_mut() else {
            return;
        };
        if path.is_dir() {
            app.cwd = path;
            app.filter.clear();
            app.refresh();
            app.status = String::from("Jumped to favorite");
        } else if path.is_file() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let parent = path.parent().map(Path::to_path_buf);
            match (parent, name) {
                (Some(parent), Some(name)) if parent.is_dir() => {
                    app.cwd = parent;
                    app.filter.clear();
                    app.refresh();
                    app.select_by_name(&name);
                }
                _ => app.status = String::from("Favorite no longer exists"),
            }
        } else {
            app.status = String::from("Favorite no longer exists");
        }
    }

    // ------------------------------------------------------------- session

    /// Describe one tab for the session file.
    fn tab_desc(&self, idx: usize) -> Option<session::TabDesc> {
        match self.tabs.get(idx)? {
            Tab::Browser(app) => {
                if let Some(ed) = app.editor.as_ref() {
                    Some(session::TabDesc::Editor(session::EditorDesc {
                        cwd: app.cwd.to_string_lossy().into_owned(),
                        path: ed.path.to_string_lossy().into_owned(),
                        backup: ed
                            .backup
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        row: ed.row,
                        col: ed.col,
                    }))
                } else {
                    Some(session::TabDesc::Browser(session::BrowserDesc {
                        cwd: app.cwd.to_string_lossy().into_owned(),
                        selected: app
                            .selected_entry()
                            .map(|e| e.name.clone())
                            .unwrap_or_default(),
                        view: match app.view {
                            ViewMode::List => "list",
                            ViewMode::Columns => "columns",
                        }
                        .to_string(),
                        sort: match app.sort.key {
                            SortKey::Name => "name",
                            SortKey::Size => "size",
                            SortKey::Modified => "modified",
                            SortKey::Type => "type",
                        }
                        .to_string(),
                        ascending: app.sort.ascending,
                        show_hidden: app.show_hidden,
                    }))
                }
            }
            Tab::Shell(sh) => Some(session::TabDesc::Shell(session::ShellDesc {
                cwd: sh.cwd.to_string_lossy().into_owned(),
            })),
        }
    }

    fn collect_descs(&self) -> Vec<session::TabDesc> {
        (0..self.tabs.len())
            .filter_map(|i| self.tab_desc(i))
            .collect()
    }

    /// Persist the tab list only (cheap). Called when tabs open, close, or
    /// switch, so even a crash restores the layout. Dirty editor buffers are
    /// backed up on quit, not here.
    pub fn save_tabs_only(&self) {
        if let Some(dir) = self.config_dir.as_deref() {
            let _ = session::save_tabs_in(dir, &self.collect_descs(), self.active);
        }
    }

    /// Full session save for quit: tab list plus a buffer backup for every
    /// dirty or untitled editor, so unsaved work survives a restart.
    pub fn save_session(&mut self) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let _ = session::clear_backups_in(&dir);
        for i in 0..self.tabs.len() {
            let backup_name = format!("backup-{i}.txt");
            if let Some(Tab::Browser(app)) = self.tabs.get_mut(i) {
                if let Some(ed) = app.editor.as_mut() {
                    let needs_backup = ed.dirty || ed.path.as_os_str().is_empty();
                    if needs_backup {
                        if let Some(path) = session::backup_file_in(&dir, &backup_name) {
                            if std::fs::write(&path, ed.backup_text()).is_ok() {
                                ed.backup = Some(path);
                            }
                        }
                    } else {
                        ed.clear_backup();
                    }
                }
            }
        }
        self.save_tabs_only();
    }

    /// Restore tabs from the previous session. Does nothing when there is
    /// no usable session file (the fresh tab from `new` stands).
    pub fn restore_session(&mut self) {
        let Some(dir) = self.config_dir.as_deref() else {
            return;
        };
        let Some((descs, active)) = session::load_tabs_in(dir) else {
            return;
        };
        let mut tabs = Vec::new();
        for d in descs {
            if let Some(tab) = self.restore_tab(d) {
                tabs.push(tab);
            }
        }
        if tabs.is_empty() {
            return;
        }
        self.tabs = tabs;
        self.active = active.min(self.tabs.len() - 1);
        if let Some(app) = self.active_browser_mut() {
            app.status = String::from("Session restored");
        }
    }

    /// A browser tab with workspace theme settings applied, like `new` builds.
    fn restored_app(&self, cwd: PathBuf) -> App {
        let mut app = App::new(cwd);
        app.theme_idx = self.theme_idx;
        app.show_line_numbers = self.show_line_numbers;
        app
    }

    fn home_dir() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/"))
    }

    /// Rebuild one tab from its descriptor. `None` when it can't be
    /// honored (deleted directory, missing file with no backup, ...).
    fn restore_tab(&mut self, desc: session::TabDesc) -> Option<Tab> {
        match desc {
            session::TabDesc::Browser(b) => {
                let cwd = PathBuf::from(&b.cwd);
                if !cwd.is_dir() {
                    return None;
                }
                let mut app = self.restored_app(cwd);
                app.show_hidden = b.show_hidden;
                app.sort.key = match b.sort.as_str() {
                    "name" => SortKey::Name,
                    "size" => SortKey::Size,
                    "type" => SortKey::Type,
                    _ => SortKey::Modified,
                };
                app.sort.ascending = b.ascending;
                app.refresh();
                if !b.selected.is_empty() {
                    app.select_by_name(&b.selected);
                }
                if b.view == "columns" {
                    // enter_column_mode carries the list selection into the
                    // last column, so select first, then switch views.
                    app.enter_column_mode();
                }
                Some(Tab::Browser(app))
            }
            session::TabDesc::Editor(e) => {
                let path = PathBuf::from(&e.path);
                // Unsaved work comes back from the backup; otherwise reopen
                // the file from disk. Untitled documents have no file.
                let dir = self.config_dir.as_deref();
                let mut ed =
                    if let Some(bp) = dir.and_then(|d| session::read_backup_in(d, &e.backup)) {
                        Editor::from_backup(path.clone(), &bp.0, bp.1)
                    } else if path.as_os_str().is_empty() {
                        Editor::untitled()
                    } else if path.is_file() {
                        Editor::open(&path).ok()?
                    } else {
                        return None;
                    };
                ed.show_line_numbers = self.show_line_numbers;
                ed.row = e.row.min(ed.lines.len().saturating_sub(1));
                ed.col = e
                    .col
                    .min(ed.lines.get(ed.row).map(|l| l.chars().count()).unwrap_or(0));
                // The browser behind the editor: its old cwd, the file's
                // parent, or home — whichever exists.
                let cwd = PathBuf::from(&e.cwd);
                let cwd = if cwd.is_dir() {
                    cwd
                } else if let Some(p) = path.parent().filter(|p| p.is_dir()) {
                    p.to_path_buf()
                } else {
                    Self::home_dir()
                };
                let mut app = self.restored_app(cwd);
                app.editor = Some(ed);
                app.mode = Mode::Editor;
                Some(Tab::Browser(app))
            }
            session::TabDesc::Shell(s) => {
                let cwd = PathBuf::from(&s.cwd);
                let cwd = if cwd.is_dir() { cwd } else { Self::home_dir() };
                // A fresh shell in the old directory; the dead process
                // itself can't be resurrected.
                ShellTab::spawn(80, 24, &cwd).ok().map(Tab::Shell)
            }
        }
    }

    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active = (self.active + 1) % self.tabs.len();
            self.show_favorites = false;
            self.save_tabs_only();
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
            self.show_favorites = false;
            self.save_tabs_only();
        }
    }

    pub fn goto_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
            self.show_favorites = false;
            self.save_tabs_only();
        }
    }

    /// Short label for the tab strip.
    /// Tab titles are capped so long file names don't crowd the tab strip.
    pub fn tab_title(&self, i: usize) -> String {
        let raw = match self.tabs.get(i) {
            Some(Tab::Browser(app)) => {
                // While editing, the tab names the open file, not the directory.
                if matches!(app.mode, Mode::Editor | Mode::ConfirmDiscard) {
                    app.editor
                        .as_ref()
                        .and_then(|ed| ed.path.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| String::from("untitled"))
                } else if let Some(name) = &app.network_name {
                    // Mounted network shares name the share, not the temp dir.
                    name.clone()
                } else {
                    app.cwd
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| app.cwd.to_string_lossy().into_owned())
                }
            }
            Some(Tab::Shell(sh)) => {
                if sh.state.exited {
                    String::from("shell (exited)")
                } else {
                    sh.state.title.clone()
                }
            }
            None => String::new(),
        };
        short_tab_title(&raw)
    }

    /// Tab-management keys, active in every mode — including shell tabs,
    /// where every other key goes to the shell instead. Returns true when
    /// the key was consumed.
    pub fn handle_tab_key(&mut self, key: KeyEvent) -> bool {
        // Ctrl+G leader: the key after it jumps tabs. This is the
        // macOS-friendly alternative to Alt+1-9 (macOS keyboards have no
        // Alt key, and terminals turn Option+digit into special characters).
        if self.goto_pending {
            self.goto_pending = false;
            let mods = key.modifiers;
            let plain = !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::ALT);
            match key.code {
                KeyCode::Esc => return true,
                KeyCode::Char(c) if plain && ('1'..='9').contains(&c) => {
                    self.goto_tab((c as usize) - ('1' as usize));
                    return true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') if plain => {
                    self.next_tab();
                    return true;
                }
                KeyCode::Char('p') | KeyCode::Char('P') if plain => {
                    self.prev_tab();
                    return true;
                }
                // Ctrl+G again: stay armed.
                KeyCode::Char('g') | KeyCode::Char('G') if mods.contains(KeyModifiers::CONTROL) => {
                    self.goto_pending = true;
                    return true;
                }
                // Anything else cancels, and the key works normally (so a
                // Ctrl+C in a shell tab still sends SIGINT, etc.).
                _ => return false,
            }
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if ctrl && !alt {
            match key.code {
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    self.new_browser_tab();
                    return true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.new_text_tab();
                    return true;
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    self.close_active_tab();
                    return true;
                }
                KeyCode::Char('g') | KeyCode::Char('G') => {
                    self.goto_pending = true;
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
        // Isolated persistence: tests must never touch the real ~/.config.
        let mut ws = Workspace::new(dir.clone());
        ws.set_config_dir(Some(dir.join("config")));
        (ws, dir)
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
    fn ctrl_g_leader_jumps_to_tab() {
        let (mut ws, dir) = test_workspace();
        let key = |code: KeyCode, mods: KeyModifiers| KeyEvent {
            code,
            modifiers: mods,
            kind: ratatui::crossterm::event::KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        };
        let ctrl = KeyModifiers::CONTROL;
        let none = KeyModifiers::NONE;
        ws.new_browser_tab();
        ws.new_browser_tab();
        assert_eq!(ws.tabs.len(), 3);
        ws.goto_tab(0);
        // Ctrl+G arms the leader, then a digit jumps.
        assert!(ws.handle_tab_key(key(KeyCode::Char('g'), ctrl)));
        assert!(ws.goto_pending);
        assert!(ws.handle_tab_key(key(KeyCode::Char('3'), none)));
        assert_eq!(ws.active, 2);
        assert!(!ws.goto_pending, "leader disarms after the jump");
        // Out-of-range digits are ignored.
        assert!(ws.handle_tab_key(key(KeyCode::Char('g'), ctrl)));
        assert!(ws.handle_tab_key(key(KeyCode::Char('9'), none)));
        assert_eq!(ws.active, 2);
        // n / p step to the next / previous tab.
        assert!(ws.handle_tab_key(key(KeyCode::Char('g'), ctrl)));
        assert!(ws.handle_tab_key(key(KeyCode::Char('n'), none)));
        assert_eq!(ws.active, 0, "next wraps around");
        assert!(ws.handle_tab_key(key(KeyCode::Char('G'), ctrl)));
        assert!(ws.handle_tab_key(key(KeyCode::Char('p'), none)));
        assert_eq!(ws.active, 2, "prev wraps around");
        // Esc cancels the leader without moving.
        assert!(ws.handle_tab_key(key(KeyCode::Char('g'), ctrl)));
        assert!(ws.handle_tab_key(key(KeyCode::Esc, none)));
        assert_eq!(ws.active, 2);
        assert!(!ws.goto_pending);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_g_unexpected_key_cancels_and_passes_through() {
        let (mut ws, dir) = test_workspace();
        let key = |code: KeyCode, mods: KeyModifiers| KeyEvent {
            code,
            modifiers: mods,
            kind: ratatui::crossterm::event::KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        };
        let ctrl = KeyModifiers::CONTROL;
        let none = KeyModifiers::NONE;
        // An unrelated key after Ctrl+G is not swallowed: the leader
        // cancels and the key is left for normal handling (so Ctrl+C in a
        // shell tab still sends SIGINT).
        assert!(ws.handle_tab_key(key(KeyCode::Char('g'), ctrl)));
        assert!(!ws.handle_tab_key(key(KeyCode::Char('x'), none)));
        assert!(!ws.goto_pending);
        assert!(ws.handle_tab_key(key(KeyCode::Char('g'), ctrl)));
        assert!(!ws.handle_tab_key(key(KeyCode::Char('c'), ctrl)));
        assert!(!ws.goto_pending);
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

    #[test]
    fn preview_off_by_default_and_toggleable() {
        let (mut ws, dir) = test_workspace();
        assert!(!ws.active_browser().unwrap().show_preview);
        ws.active_browser_mut().unwrap().toggle_preview();
        assert!(ws.active_browser().unwrap().show_preview);
        assert_eq!(ws.active_browser().unwrap().status, "Preview on");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tab_title_truncates_and_names_edited_file() {
        assert_eq!(short_tab_title("short.txt"), "short.txt");
        let t = short_tab_title("a_very_long_file_name_for_tab_title_testing.txt");
        assert_eq!(t.chars().count(), 24);
        assert!(t.ends_with('…'));

        let (mut ws, dir) = test_workspace();
        // Browser tabs name the folder.
        let folder = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(ws.tab_title(0), folder);
        // Editing tabs name the open file instead.
        ws.active_browser_mut().unwrap().open_editor();
        assert!(matches!(ws.active_browser().unwrap().mode, Mode::Editor));
        assert_eq!(ws.tab_title(0), "a.txt");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_event_ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn new_text_tab_opens_untitled_editor() {
        let (mut ws, dir) = test_workspace();
        ws.new_text_tab();
        assert_eq!(ws.tabs.len(), 2);
        assert_eq!(ws.active, 1);
        let app = ws.active_browser().unwrap();
        assert!(matches!(app.mode, Mode::Editor));
        let ed = app.editor.as_ref().unwrap();
        assert!(ed.is_untitled());
        assert!(!ed.dirty, "a fresh untitled doc has nothing to lose");
        assert_eq!(ws.tab_title(1), "untitled");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_s_on_untitled_opens_save_dialog() {
        let (mut ws, dir) = test_workspace();
        ws.new_text_tab();
        crate::input::handle_key(&mut ws, key_event_ctrl(KeyCode::Char('s')));
        assert!(ws.active_browser().unwrap().save_dialog.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn type_in_dialog(app: &mut App, text: &str) {
        for c in text.chars() {
            app.save_dialog_key(key_event(KeyCode::Char(c)));
        }
    }

    #[test]
    fn save_dialog_saves_untitled_into_chosen_dir() {
        let (mut ws, dir) = test_workspace();
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        ws.new_text_tab();
        {
            let ed = ws.active_browser_mut().unwrap().editor.as_mut().unwrap();
            for c in "hello".chars() {
                ed.insert_char(c);
            }
        }
        crate::input::handle_key(&mut ws, key_event_ctrl(KeyCode::Char('s')));
        let app = ws.active_browser_mut().unwrap();
        // Navigate into "docs".
        let idx = app
            .save_dialog
            .as_ref()
            .unwrap()
            .dirs
            .iter()
            .position(|d| d.file_name().unwrap() == "docs")
            .unwrap();
        app.save_dialog.as_mut().unwrap().selected = idx;
        app.save_dialog_key(key_event(KeyCode::Right));
        assert_eq!(app.save_dialog.as_ref().unwrap().cwd, dir.join("docs"));
        // Type the file name and save.
        type_in_dialog(app, "notes.txt");
        app.save_dialog_key(key_event(KeyCode::Enter));
        assert!(app.save_dialog.is_none(), "dialog closes after saving");
        let saved = dir.join("docs").join("notes.txt");
        assert!(saved.exists());
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "hello\n");
        let ed = app.editor.as_ref().unwrap();
        assert!(!ed.is_untitled());
        assert_eq!(ed.path, saved);
        assert!(!ed.dirty);
        assert_eq!(ws.tab_title(1), "notes.txt");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_dialog_overwrite_needs_confirmation() {
        let (mut ws, dir) = test_workspace();
        ws.new_text_tab();
        {
            let ed = ws.active_browser_mut().unwrap().editor.as_mut().unwrap();
            for c in "new".chars() {
                ed.insert_char(c);
            }
        }
        crate::input::handle_key(&mut ws, key_event_ctrl(KeyCode::Char('s')));
        let app = ws.active_browser_mut().unwrap();
        type_in_dialog(app, "a.txt"); // already exists in the test dir
        app.save_dialog_key(key_event(KeyCode::Enter));
        assert!(
            app.save_dialog.is_some(),
            "first Enter only arms the overwrite prompt"
        );
        assert!(app
            .save_dialog
            .as_ref()
            .unwrap()
            .message
            .contains("overwrite"));
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "hello\n",
            "file untouched until confirmed"
        );
        app.save_dialog_key(key_event(KeyCode::Enter));
        assert!(app.save_dialog.is_none());
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "new\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_dialog_esc_cancels_without_saving() {
        let (mut ws, dir) = test_workspace();
        ws.new_text_tab();
        crate::input::handle_key(&mut ws, key_event_ctrl(KeyCode::Char('s')));
        let app = ws.active_browser_mut().unwrap();
        type_in_dialog(app, "nope.txt");
        app.save_dialog_key(key_event(KeyCode::Esc));
        assert!(app.save_dialog.is_none());
        assert!(!dir.join("nope.txt").exists());
        assert!(app.editor.as_ref().unwrap().is_untitled());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_dialog_navigates_dirs() {
        let (mut ws, dir) = test_workspace();
        std::fs::create_dir_all(dir.join("sub").join("deep")).unwrap();
        ws.new_text_tab();
        crate::input::handle_key(&mut ws, key_event_ctrl(KeyCode::Char('s')));
        let app = ws.active_browser_mut().unwrap();
        let dlg = app.save_dialog.as_ref().unwrap();
        assert_eq!(dlg.cwd, dir);
        assert!(dlg.dirs.iter().any(|d| d.file_name().unwrap() == "sub"));
        // Down/up wrap around the list.
        let n = app.save_dialog.as_ref().unwrap().dirs.len();
        app.save_dialog_key(key_event(KeyCode::Up));
        assert_eq!(app.save_dialog.as_ref().unwrap().selected, n - 1);
        app.save_dialog_key(key_event(KeyCode::Down));
        assert_eq!(app.save_dialog.as_ref().unwrap().selected, 0);
        // Enter the first dir, then go back up.
        app.save_dialog.as_mut().unwrap().selected = 0;
        let first = app.save_dialog.as_ref().unwrap().dirs[0].clone();
        app.save_dialog_key(key_event(KeyCode::Right));
        assert_eq!(app.save_dialog.as_ref().unwrap().cwd, first);
        app.save_dialog_key(key_event(KeyCode::Left));
        assert_eq!(app.save_dialog.as_ref().unwrap().cwd, dir);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn test_net_view() -> NetView {
        NetView::new().expect("mDNS discovery should start in tests")
    }

    #[test]
    fn net_view_add_manual_host() {
        let mut nv = test_net_view();
        nv.add_manual_host("192.168.1.10");
        assert_eq!(nv.devices.len(), 1);
        assert_eq!(nv.devices[0].host, "192.168.1.10");
        assert!(nv.devices[0].manual);
        // Duplicates are ignored.
        nv.add_manual_host("192.168.1.10");
        assert_eq!(nv.devices.len(), 1);
        // Blank input is ignored.
        nv.add_manual_host("   ");
        assert_eq!(nv.devices.len(), 1);
    }

    #[test]
    fn net_view_shares_flow() {
        let mut nv = test_net_view();
        nv.state = NetState::LoadingShares {
            host: String::from("nas"),
        };
        let out = nv.apply_msg(NetMsg::Shares {
            host: String::from("nas"),
            result: Ok(vec![String::from("Media"), String::from("Backups")]),
        });
        assert!(out.is_none());
        assert!(matches!(nv.state, NetState::Shares { .. }));
        assert_eq!(nv.selected, 0);
        // A stale reply after backing out is ignored.
        nv.back();
        assert!(matches!(nv.state, NetState::Devices));
        let out = nv.apply_msg(NetMsg::Shares {
            host: String::from("nas"),
            result: Ok(vec![String::from("X")]),
        });
        assert!(out.is_none());
        assert!(matches!(nv.state, NetState::Devices));
    }

    #[test]
    fn net_view_empty_share_list() {
        let mut nv = test_net_view();
        nv.state = NetState::LoadingShares {
            host: String::from("nas"),
        };
        nv.apply_msg(NetMsg::Shares {
            host: String::from("nas"),
            result: Ok(Vec::new()),
        });
        assert!(matches!(nv.state, NetState::Devices));
        assert!(!nv.message.is_empty());
    }

    #[test]
    fn net_view_mount_success_returns_mount() {
        let mut nv = test_net_view();
        nv.state = NetState::Mounting {
            host: String::from("nas"),
            share: String::from("Media"),
        };
        let out = nv.apply_msg(NetMsg::Mounted {
            host: String::from("nas"),
            share: String::from("Media"),
            result: Ok(net::MountedShare {
                path: PathBuf::from("/tmp/fex-test-mnt"),
                needs_unmount: false,
            }),
        });
        let (host, share, mounted) = out.expect("mount should be returned");
        assert_eq!(host, "nas");
        assert_eq!(share, "Media");
        assert_eq!(mounted.path, PathBuf::from("/tmp/fex-test-mnt"));
        assert!(matches!(nv.state, NetState::Devices));
    }

    #[test]
    fn net_view_auth_failure_prompts_for_username() {
        let mut nv = test_net_view();
        nv.state = NetState::Mounting {
            host: String::from("nas"),
            share: String::from("Media"),
        };
        let err = io::Error::new(
            io::ErrorKind::Other,
            "mount_smbfs: server rejected the connection: Authentication error",
        );
        let out = nv.apply_msg(NetMsg::Mounted {
            host: String::from("nas"),
            share: String::from("Media"),
            result: Err(err),
        });
        assert!(out.is_none());
        assert_eq!(nv.take_next_prompt(), Some(InputKind::SmbUser));
        // Username in, password prompt chained by submit_input; the view
        // just records the user.
        nv.submit_credentials(InputKind::SmbUser, "kurt");
        // Password in: the mount is retried with both credentials.
        nv.submit_credentials(InputKind::SmbPass, "secret");
        assert!(matches!(nv.state, NetState::Mounting { .. }));
    }

    #[test]
    fn net_view_non_auth_mount_failure_shows_error() {
        let mut nv = test_net_view();
        nv.state = NetState::Mounting {
            host: String::from("nas"),
            share: String::from("Media"),
        };
        let out = nv.apply_msg(NetMsg::Mounted {
            host: String::from("nas"),
            share: String::from("Media"),
            result: Err(io::Error::new(io::ErrorKind::Other, "No route to host")),
        });
        assert!(out.is_none());
        assert!(nv.take_next_prompt().is_none());
        assert!(nv.message.contains("No route to host"));
    }

    #[test]
    fn net_view_share_list_failure_prompts_for_share_name() {
        let mut nv = test_net_view();
        nv.state = NetState::LoadingShares {
            host: String::from("nas"),
        };
        nv.apply_msg(NetMsg::Shares {
            host: String::from("nas"),
            result: Err(io::Error::new(io::ErrorKind::Other, "timed out")),
        });
        assert_eq!(nv.take_next_prompt(), Some(InputKind::NetShare));
        // Typing the share name starts the mount.
        nv.submit_manual_share("Media");
        assert!(matches!(nv.state, NetState::Mounting { .. }));
    }

    #[test]
    fn mounted_tab_names_share_and_unmounts_on_close() {
        let (mut ws, _dir) = test_workspace();
        ws.open_mounted_tab(
            "nas",
            "Media",
            net::MountedShare {
                path: PathBuf::from("/tmp/fex-test-mnt"),
                needs_unmount: true,
            },
        );
        assert_eq!(ws.tabs.len(), 2);
        assert_eq!(ws.tab_title(1), "//nas/Media");
        // Closing the tab unmounts (best effort; a no-op on Linux).
        ws.close_active_tab();
        assert_eq!(ws.tabs.len(), 1);
    }

    fn fake_drive(name: &str) -> Drive {
        Drive {
            name: name.to_string(),
            mount_point: PathBuf::from(format!("/mnt/{name}")),
            available: 10_000,
            total: 100_000,
            removable: false,
        }
    }

    #[test]
    fn net_view_rows_group_drives_then_network() {
        let mut nv = test_net_view();
        nv.drives = vec![fake_drive("b"), fake_drive("a")];
        nv.devices = vec![NetDevice::manual("nas")];
        let rows = nv.rows();
        assert_eq!(
            rows,
            vec![
                NetRow::Header(NetSection::Drives),
                NetRow::Drive(0),
                NetRow::Drive(1),
                NetRow::Header(NetSection::Network),
                NetRow::Device(0),
            ]
        );
    }

    #[test]
    fn net_view_rows_show_hint_when_no_devices() {
        let mut nv = test_net_view();
        nv.drives = vec![fake_drive("d")];
        nv.devices.clear();
        let rows = nv.rows();
        assert_eq!(
            rows,
            vec![
                NetRow::Header(NetSection::Drives),
                NetRow::Drive(0),
                NetRow::Header(NetSection::Network),
                NetRow::Hint,
            ]
        );
        // The hint is not selectable.
        assert!(!NetRow::Hint.selectable());
        assert!(!NetRow::Header(NetSection::Drives).selectable());
    }

    #[test]
    fn net_view_rows_include_mapped_section() {
        let mut nv = test_net_view();
        nv.drives = vec![fake_drive("d")];
        nv.mapped = vec![
            net::MappedDrive {
                local: "Z:".to_string(),
                remote: r"\\nas\media".to_string(),
            },
            net::MappedDrive {
                local: "Y:".to_string(),
                remote: r"\\nas\backup".to_string(),
            },
        ];
        nv.devices.clear();
        let rows = nv.rows();
        assert_eq!(
            rows,
            vec![
                NetRow::Header(NetSection::Drives),
                NetRow::Drive(0),
                NetRow::Header(NetSection::Mapped),
                NetRow::Mapped(0),
                NetRow::Mapped(1),
                NetRow::Header(NetSection::Network),
                NetRow::Hint,
            ]
        );
        assert!(NetRow::Mapped(0).selectable());
        // The selection walks through the mapped rows.
        nv.selected = nv.first_selectable();
        assert_eq!(nv.rows()[nv.selected], NetRow::Drive(0));
        assert!(nv.selected_mapped().is_none());
        nv.move_selection(1);
        assert_eq!(nv.rows()[nv.selected], NetRow::Mapped(0));
        assert_eq!(nv.selected_mapped().unwrap().remote, r"\\nas\media");
        nv.move_selection(1);
        assert_eq!(nv.rows()[nv.selected], NetRow::Mapped(1));
    }

    #[test]
    fn net_view_no_mapped_section_when_empty() {
        let mut nv = test_net_view();
        nv.drives = vec![fake_drive("d")];
        nv.mapped.clear();
        nv.devices.clear();
        let rows = nv.rows();
        assert!(!rows.contains(&NetRow::Header(NetSection::Mapped)));
        assert!(nv.selected_mapped().is_none());
    }

    #[test]
    fn net_view_enter_on_mapped_row_is_ignored_by_view() {
        // The input layer opens mapped drives as tabs; NetView::enter
        // ignores them, like plain drives.
        let mut nv = test_net_view();
        nv.drives.clear();
        nv.mapped = vec![net::MappedDrive {
            local: "Z:".to_string(),
            remote: r"\\nas\media".to_string(),
        }];
        nv.devices.clear();
        nv.selected = nv.first_selectable();
        assert_eq!(nv.rows()[nv.selected], NetRow::Mapped(0));
        nv.enter();
        assert!(matches!(nv.state, NetState::Devices));
    }

    #[test]
    fn norm_drive_ignores_case_and_separators() {
        assert_eq!(norm_drive("Z:\\"), "z:");
        assert_eq!(norm_drive("z:"), "z:");
        assert_eq!(norm_drive("/mnt/x/"), "/mnt/x");
    }

    #[test]
    fn net_view_selection_skips_headers_and_wraps() {
        let mut nv = test_net_view();
        nv.drives = vec![fake_drive("d")];
        nv.devices = vec![NetDevice::manual("nas")];
        // rows: [Header, Drive(0), Header, Device(0)]
        nv.selected = nv.first_selectable();
        assert_eq!(nv.rows()[nv.selected], NetRow::Drive(0));
        nv.move_selection(1);
        assert_eq!(nv.rows()[nv.selected], NetRow::Device(0));
        nv.move_selection(1); // wraps past both headers
        assert_eq!(nv.rows()[nv.selected], NetRow::Drive(0));
        nv.move_selection(-1);
        assert_eq!(nv.rows()[nv.selected], NetRow::Device(0));
        // Home/End jump to first/last selectable rows.
        nv.jump_to(0);
        assert_eq!(nv.rows()[nv.selected], NetRow::Drive(0));
        nv.jump_to(usize::MAX);
        assert_eq!(nv.rows()[nv.selected], NetRow::Device(0));
    }

    #[test]
    fn net_view_enter_on_drive_row_is_ignored() {
        let mut nv = test_net_view();
        nv.drives = vec![fake_drive("d")];
        nv.selected = nv.first_selectable();
        assert!(nv.selected_drive().is_some());
        assert!(nv.selected_device().is_none());
        nv.enter();
        // Drives are opened as tabs by the input layer; NetView::enter
        // ignores them.
        assert!(matches!(nv.state, NetState::Devices));
    }

    #[test]
    fn net_view_enter_on_device_row_starts_share_listing() {
        let mut nv = test_net_view();
        nv.drives.clear();
        nv.devices = vec![NetDevice::manual("nas")];
        nv.selected = nv.first_selectable();
        assert_eq!(nv.rows()[nv.selected], NetRow::Device(0));
        assert!(nv.selected_drive().is_none());
        nv.enter();
        assert!(matches!(
            nv.state,
            NetState::LoadingShares { ref host } if host == "nas"
        ));
    }

    #[test]
    fn net_view_back_resets_selection_past_headers() {
        let mut nv = test_net_view();
        nv.drives = vec![fake_drive("d")];
        nv.state = NetState::Shares {
            host: String::from("nas"),
            shares: vec![String::from("Media")],
        };
        nv.selected = 0; // a share-list index, meaningless up top
        assert!(!nv.back()); // back a level, don't close
        assert!(matches!(nv.state, NetState::Devices));
        assert!(nv.rows()[nv.selected].selectable());
    }

    #[test]
    fn open_drive_tab_opens_browser_at_mount_point() {
        let (mut ws, dir) = test_workspace();
        let target = dir.join("drivevol");
        std::fs::create_dir(&target).unwrap();
        ws.open_drive_tab(&target, "TestDrive");
        assert_eq!(ws.tabs.len(), 2);
        assert_eq!(ws.active, 1);
        match &ws.tabs[1] {
            Tab::Browser(app) => {
                assert_eq!(app.cwd, target);
                assert!(app.mount_point.is_none());
                assert!(app.network_name.is_none());
            }
            _ => panic!("expected a browser tab"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn fake_entry(name: &str, is_dir: bool, size: u64, modified: Option<SystemTime>) -> Entry {
        Entry {
            path: PathBuf::from(name),
            name: name.to_string(),
            is_dir,
            size,
            modified,
            is_hidden: false,
        }
    }

    fn names_in(idx: &[usize], entries: &[Entry]) -> Vec<String> {
        idx.iter().map(|&i| entries[i].name.clone()).collect()
    }

    fn app0_mut(ws: &mut Workspace) -> &mut App {
        match &mut ws.tabs[0] {
            Tab::Browser(app) => app,
            _ => panic!("expected a browser tab"),
        }
    }

    #[test]
    fn default_sort_is_modified_newest_first() {
        let (ws, dir) = test_workspace();
        match &ws.tabs[0] {
            Tab::Browser(app) => {
                assert_eq!(app.sort.key, SortKey::Modified);
                assert!(!app.sort.ascending);
                assert_eq!(app.sort.label(), "Modified ↓");
            }
            _ => panic!("expected a browser tab"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sort_name_does_not_force_dirs_first() {
        let entries = vec![
            fake_entry("zdir", true, 0, None),
            fake_entry("apple.txt", false, 1, None),
        ];
        let order = SortOrder {
            key: SortKey::Name,
            ascending: true,
        };
        let idx = view_indices(&entries, "", order, true);
        // Purely alphabetical: the file comes before the folder.
        assert_eq!(names_in(&idx, &entries), vec!["apple.txt", "zdir"]);
    }

    #[test]
    fn sort_type_groups_dirs_then_files_by_extension() {
        let entries = vec![
            fake_entry("b.rs", false, 1, None),
            fake_entry("zdir", true, 0, None),
            fake_entry("c.md", false, 1, None),
            fake_entry("adir", true, 0, None),
            fake_entry("a.rs", false, 1, None),
            fake_entry("README", false, 1, None),
        ];
        let order = SortOrder {
            key: SortKey::Type,
            ascending: true,
        };
        let idx = view_indices(&entries, "", order, true);
        assert_eq!(
            names_in(&idx, &entries),
            vec!["adir", "zdir", "README", "c.md", "a.rs", "b.rs"]
        );
    }

    #[test]
    fn sort_type_descending_reverses_groups() {
        let entries = vec![
            fake_entry("b.rs", false, 1, None),
            fake_entry("zdir", true, 0, None),
            fake_entry("c.md", false, 1, None),
        ];
        let order = SortOrder {
            key: SortKey::Type,
            ascending: false,
        };
        let idx = view_indices(&entries, "", order, true);
        assert_eq!(names_in(&idx, &entries), vec!["b.rs", "c.md", "zdir"]);
    }

    #[test]
    fn sort_modified_descending_puts_newest_first() {
        let t0 = SystemTime::UNIX_EPOCH;
        let t1 = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let entries = vec![
            fake_entry("old.txt", false, 1, Some(t0)),
            fake_entry("nodate.txt", false, 1, None),
            fake_entry("new.txt", false, 1, Some(t1)),
        ];
        let order = SortOrder {
            key: SortKey::Modified,
            ascending: false,
        };
        let idx = view_indices(&entries, "", order, true);
        assert_eq!(
            names_in(&idx, &entries),
            vec!["new.txt", "old.txt", "nodate.txt"]
        );
    }

    #[test]
    fn cycle_sort_key_includes_type() {
        let (mut ws, dir) = test_workspace();
        let app = app0_mut(&mut ws);
        assert_eq!(app.sort.key, SortKey::Modified); // default
        app.cycle_sort_key();
        assert_eq!(app.sort.key, SortKey::Type);
        assert_eq!(app.sort.label(), "Type ↓");
        app.cycle_sort_key();
        assert_eq!(app.sort.key, SortKey::Name);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ------------------------------------------------------- session/favorites

    #[test]
    fn favorites_toggle_and_persist() {
        let (mut ws, dir) = test_workspace();
        let file = dir.join("a.txt");
        assert!(!ws.favorites.contains(&file));
        assert!(ws.toggle_favorite(file.clone()), "first toggle adds");
        assert!(ws.favorites.contains(&file));
        // Persisted to the config dir: a fresh workspace sees it too.
        let mut ws2 = Workspace::new(dir.clone());
        ws2.set_config_dir(Some(dir.join("config")));
        assert!(ws2.favorites.contains(&file));
        // Toggling again removes it.
        assert!(!ws.toggle_favorite(file.clone()), "second toggle removes");
        assert!(!ws.favorites.contains(&file));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn session_round_trip_restores_browser_and_dirty_editor() {
        let (mut ws, dir) = test_workspace();
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("b.txt"), "b\n").unwrap();

        // Tab 1: browser in the subdir with b.txt selected, in column view.
        ws.new_browser_tab();
        if let Tab::Browser(app) = &mut ws.tabs[1] {
            app.cwd = sub.clone();
            app.refresh();
            app.select_by_name("b.txt");
            app.enter_column_mode();
        }

        // Tab 0: dirty editor on a.txt.
        if let Tab::Browser(app) = &mut ws.tabs[0] {
            let mut ed = Editor::open(&dir.join("a.txt")).unwrap();
            ed.insert_char('X');
            assert!(ed.dirty);
            app.editor = Some(ed);
            app.mode = Mode::Editor;
        }

        ws.save_session();

        // Restore into a fresh workspace sharing the config dir.
        let mut ws2 = Workspace::new(dir.clone());
        ws2.set_config_dir(Some(dir.join("config")));
        ws2.restore_session();
        assert_eq!(ws2.tabs.len(), 2);

        // Tab 0: the dirty editor came back from its backup.
        match &ws2.tabs[0] {
            Tab::Browser(app) => {
                let ed = app.editor.as_ref().expect("editor restored");
                assert!(ed.dirty, "restored editor is dirty");
                assert_eq!(ed.backup_text(), "Xhello\n");
            }
            _ => panic!("tab 0 should be a browser"),
        }

        // Tab 1: browser back in the subdir, still in column view with
        // the selection kept in the last column.
        match &ws2.tabs[1] {
            Tab::Browser(app) => {
                assert_eq!(app.cwd, sub);
                assert_eq!(app.view, ViewMode::Columns);
                let last = app.columns.last().expect("columns built");
                let sel = last.entries.get(last.selected).map(|e| e.name.as_str());
                assert_eq!(sel, Some("b.txt"));
            }
            _ => panic!("tab 1 should be a browser"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn session_backup_destroyed_when_tab_closed() {
        let (mut ws, dir) = test_workspace();
        // Dirty editor on tab 0, saved to disk via save_session.
        if let Tab::Browser(app) = &mut ws.tabs[0] {
            let mut ed = Editor::open(&dir.join("a.txt")).unwrap();
            ed.insert_char('X');
            app.editor = Some(ed);
            app.mode = Mode::Editor;
        }
        ws.save_session();
        let config = dir.join("config");
        let backup = config.join("session").join("backup-0.txt");
        assert!(backup.is_file(), "backup written by save_session");

        // Opening a second tab keeps the session file honest, then closing
        // the dirty tab must destroy its backup.
        ws.new_browser_tab();
        ws.prev_tab(); // back to tab 0
        ws.close_active_tab();
        assert!(!backup.exists(), "backup destroyed with its tab");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- mouse: column view ----------------------------------------------------

    /// Fake the rendered column rects: column `i` at (i*28, 4), 28 wide.
    fn fake_col_hits(app: &mut App) {
        app.col_hits = (0..app.columns.len())
            .map(|i| (i, Rect::new(i as u16 * 28, 4, 28, 20)))
            .collect();
    }

    #[test]
    fn column_click_selects_row_and_syncs_preview() {
        let (mut ws, dir) = test_workspace();
        std::fs::create_dir_all(dir.join("sub").join("inner")).unwrap();
        let app = app0_mut(&mut ws);
        app.refresh();
        app.enter_column_mode();
        let n = app.columns.len();
        let last = n - 1;
        let row = app.columns[last]
            .entries
            .iter()
            .position(|e| e.name == "sub")
            .unwrap();
        fake_col_hits(app);
        app.click_select((last as u16) * 28 + 2, 4 + 1 + row as u16);
        assert_eq!(app.col_active, last);
        assert_eq!(
            app.columns[last].entries[app.columns[last].selected].name,
            "sub"
        );
        // The trailing preview column for sub/ was rebuilt.
        assert_eq!(app.columns.len(), n + 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn column_click_on_parent_column_activates_it() {
        let (mut ws, dir) = test_workspace();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let app = app0_mut(&mut ws);
        app.refresh();
        app.enter_column_mode();
        let n = app.columns.len();
        let parent = n - 2;
        let row = app.columns[parent]
            .entries
            .iter()
            .position(|e| e.path == dir)
            .unwrap();
        fake_col_hits(app);
        app.click_select((parent as u16) * 28 + 2, 4 + 1 + row as u16);
        assert_eq!(app.col_active, parent);
        assert_eq!(app.cwd, app.columns[parent].path.clone());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn column_click_outside_columns_is_ignored() {
        let (mut ws, dir) = test_workspace();
        let app = app0_mut(&mut ws);
        app.refresh();
        app.enter_column_mode();
        fake_col_hits(app);
        let before = (app.col_active, app.columns[app.col_active].selected);
        app.click_select(200, 30); // past the rendered columns
        app.click_select(2, 100); // below the column rects
        assert_eq!(
            (app.col_active, app.columns[app.col_active].selected),
            before
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reveal_in_explorer_reports_status() {
        let (mut ws, dir) = test_workspace();
        let app = app0_mut(&mut ws);
        app.refresh();
        app.reveal_in_explorer();
        let status = app.status.clone();
        assert!(
            status.starts_with("Revealed ") || status.starts_with("Could not open"),
            "unexpected status: {status}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
