//! Application state: directory listing, selection, filter, sort, and UI modes.

use crate::fs::{self, Entry};
use crate::editor::Editor;
use ratatui::widgets::ListState;
use std::cmp::Ordering;
use std::path::PathBuf;

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
}

impl InputKind {
    pub fn title(&self) -> &'static str {
        match self {
            InputKind::NewFile => "New file",
            InputKind::NewDir => "New directory",
            InputKind::Rename => "Rename",
            InputKind::Filter => "Filter (live — Esc clears)",
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
    clipboard: Option<(PathBuf, bool)>, // (source path, is_cut)
    pub status: String,
    preview_cache: (PathBuf, bool, String),
    pub editor: Option<Editor>,
    pub should_quit: bool,
}

fn char_index_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

impl App {
    pub fn new(cwd: PathBuf) -> Self {
        let mut app = Self {
            cwd,
            entries: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            show_hidden: false,
            filter: String::new(),
            sort: SortOrder { key: SortKey::Name, ascending: true },
            mode: Mode::Normal,
            input: String::new(),
            cursor: 0,
            clipboard: None,
            status: String::new(),
            preview_cache: (PathBuf::new(), false, String::new()),
            editor: None,
            should_quit: false,
        };
        app.refresh();
        app
    }

    // -- view helpers -----------------------------------------------------

    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    pub fn visible_entries(&self) -> impl Iterator<Item = &Entry> {
        self.visible.iter().map(|&i| &self.entries[i])
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.visible.get(self.selected).map(|&i| &self.entries[i])
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
    pub fn refresh(&mut self) {
        let selected_name = self.selected_entry().map(|e| e.name.clone());
        self.entries = fs::list_dir(&self.cwd);
        self.apply_view();
        if let Some(name) = selected_name {
            if let Some(pos) = self.visible.iter().position(|&i| self.entries[i].name == name) {
                self.selected = pos;
            }
        }
        self.clamp_selection();
        self.preview_cache.0.clear(); // invalidate preview
    }

    /// Rebuild the visible list from filter + sort settings.
    fn apply_view(&mut self) {
        let needle = self.filter.to_lowercase();
        let mut idx: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                (self.show_hidden || !e.is_hidden)
                    && (needle.is_empty() || e.name.to_lowercase().contains(&needle))
            })
            .map(|(i, _)| i)
            .collect();
        let sort = self.sort;
        idx.sort_by(|&a, &b| {
            let ea = &self.entries[a];
            let eb = &self.entries[b];
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
                    if sort.ascending { ord } else { ord.reverse() }
                }
            }
        });
        self.visible = idx;
    }

    // -- navigation -------------------------------------------------------

    pub fn move_selection(&mut self, delta: i32) {
        if self.visible.is_empty() {
            return;
        }
        let len = self.visible.len() as i32;
        self.selected = (self.selected as i32 + delta).clamp(0, len - 1) as usize;
        self.list_state.select(Some(self.selected));
    }

    pub fn jump_to(&mut self, index: usize) {
        if self.visible.is_empty() {
            return;
        }
        self.selected = index.min(self.visible.len() - 1);
        self.list_state.select(Some(self.selected));
    }

    /// Enter a directory, or open a file with the OS default app.
    pub fn enter_selected(&mut self) {
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
    pub fn open_editor(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        if entry.is_dir {
            self.status = "Cannot edit a directory".to_string();
            return;
        }
        let name = entry.name.clone();
        match Editor::open(&entry.path) {
            Ok(ed) => {
                self.editor = Some(ed);
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

    /// Go to the parent directory, keeping the old directory selected.
    pub fn go_to_parent(&mut self) {
        let Some(parent) = self.cwd.parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let old_name = self.cwd.file_name().map(|n| n.to_string_lossy().into_owned());
        self.cwd = parent;
        self.selected = 0;
        self.refresh();
        if let Some(name) = old_name {
            if let Some(pos) = self.visible.iter().position(|&i| self.entries[i].name == name) {
                self.selected = pos;
                self.list_state.select(Some(pos));
            }
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
        self.apply_view();
        self.clamp_selection();
        self.status = format!("Sort: {}", self.sort.label());
    }

    pub fn toggle_sort_dir(&mut self) {
        self.sort.ascending = !self.sort.ascending;
        self.apply_view();
        self.clamp_selection();
        self.status = format!("Sort: {}", self.sort.label());
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
            InputKind::NewFile | InputKind::NewDir => {}
        }
        self.mode = Mode::Input(kind);
    }

    pub fn input_insert(&mut self, c: char) {
        let idx = char_index_to_byte(&self.input, self.cursor);
        self.input.insert(idx, c);
        self.cursor += 1;
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
        }
    }

    pub fn cancel_input(&mut self) {
        if matches!(self.mode, Mode::Input(InputKind::Filter)) {
            self.clear_filter();
        }
        self.mode = Mode::Normal;
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

    pub fn yank(&mut self, cut: bool) {
        if let Some(entry) = self.selected_entry().cloned() {
            self.clipboard = Some((entry.path.clone(), cut));
            self.status = format!("{} {}", if cut { "Cut" } else { "Copied" }, entry.name);
        }
    }

    pub fn paste(&mut self) {
        let Some((src, is_cut)) = self.clipboard.clone() else {
            self.status = String::from("Clipboard is empty");
            return;
        };
        let fallback_name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = if is_cut {
            fs::move_entry(&src, &self.cwd)
        } else {
            fs::copy_entry(&src, &self.cwd)
        };
        match result {
            Ok(dst) => {
                let name = dst
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(fallback_name);
                self.status = format!("Pasted {name}");
                if is_cut {
                    self.clipboard = None;
                }
                self.refresh();
            }
            Err(e) => self.status = format!("Paste failed: {e}"),
        }
    }

    // -- preview -----------------------------------------------------------

    /// Preview text for the selected entry, cached per path.
    pub fn preview_text(&mut self) -> &str {
        let (path, is_dir) = match self.selected_entry() {
            Some(e) => (e.path.clone(), e.is_dir),
            None => return "",
        };
        if self.preview_cache.0 != path || self.preview_cache.1 != is_dir {
            let text = fs::read_preview(&path, is_dir);
            self.preview_cache = (path, is_dir, text);
        }
        &self.preview_cache.2
    }
}
