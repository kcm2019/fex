//! Keyboard input handling, dispatched by UI mode.

use crate::app::{App, InputKind, Mode, NetState, Tab, ViewMode, Workspace};
use crate::fs;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Two left-clicks this close together (same cell) count as a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

pub fn handle_key(ws: &mut Workspace, key: KeyEvent) {
    // The favorites popup is modal: it eats every key while open.
    if ws.show_favorites {
        handle_favorites(ws, key);
        return;
    }
    // Tab-management keys work everywhere: file list, editor, prompts,
    // even shell tabs (where every other key goes to the shell).
    if ws.handle_tab_key(key) {
        return;
    }
    // In a shell tab, everything else is terminal input.
    if ws.is_shell_active() {
        ws.handle_shell_key(key);
        return;
    }
    let Some(mode) = ws.active_browser().map(|app| app.mode) else {
        return;
    };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    // Ctrl+C quits from prompts and popups. In the file list and the editor it
    // copies instead (OS-style), so those modes handle it themselves below.
    // The CSV viewer has no clipboard of its own, so Ctrl+C quits there too.
    if ctrl && !shift && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')) {
        if !matches!(mode, Mode::Normal | Mode::Editor) {
            ws.should_quit = true;
            return;
        }
    }
    // Each new keypress in a non-input mode starts with a fresh status line;
    // operations below set their own message afterwards.
    if !matches!(mode, Mode::Input(_)) {
        if let Some(app) = ws.active_browser_mut() {
            app.status.clear();
        }
    }
    match mode {
        Mode::Normal => handle_normal(ws, key),
        Mode::Input(_) => {
            if let Some(app) = ws.active_browser_mut() {
                handle_input(app, key);
            }
        }
        Mode::Search => {
            if let Some(app) = ws.active_browser_mut() {
                handle_search(app, key);
            }
        }
        Mode::ConfirmDelete => {
            if let Some(app) = ws.active_browser_mut() {
                handle_confirm(app, key);
            }
        }
        Mode::Editor => {
            if let Some(app) = ws.active_browser_mut() {
                handle_editor(app, key);
            }
        }
        Mode::Sheet => {
            if let Some(app) = ws.active_browser_mut() {
                handle_sheet(app, key);
            }
        }
        Mode::ConfirmDiscard => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(app) = ws.active_browser_mut() {
                    app.close_editor();
                }
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(app) = ws.active_browser_mut() {
                    app.mode = Mode::Editor;
                }
            }
            _ => {}
        },
        Mode::Help => match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                if let Some(app) = ws.active_browser_mut() {
                    app.mode = Mode::Normal;
                }
            }
            // Settings, also listed in the help popup itself.
            KeyCode::Char('t') | KeyCode::Char('T') => ws.cycle_theme(),
            KeyCode::Char('l') | KeyCode::Char('L') => ws.toggle_line_numbers(),
            _ => {}
        },
        Mode::Network => handle_network(ws, key),
    }
}

/// Keys for the Network view: browse local drives and discovered
/// devices, mount a share as a new tab.
fn handle_network(ws: &mut Workspace, key: KeyEvent) {
    // Enter on a mapped network drive (Windows) opens it as a new browser
    // tab — it's already connected, so no mounting is needed.
    // Enter on a drive row opens it as a new browser tab; anywhere else
    // the view handles the key itself.
    if matches!(key.code, KeyCode::Enter | KeyCode::Right) {
        let mapped = ws.active_browser().and_then(|app| {
            app.net
                .as_ref()
                .and_then(|nv| nv.selected_mapped())
                .map(|m| {
                    (
                        PathBuf::from(format!("{}{}", m.local, std::path::MAIN_SEPARATOR)),
                        format!("{} ({})", m.local, m.remote),
                    )
                })
        });
        if let Some((path, name)) = mapped {
            ws.open_drive_tab(&path, &name);
            return;
        }
        let drive = ws.active_browser().and_then(|app| {
            app.net
                .as_ref()
                .and_then(|nv| nv.selected_drive())
                .map(|d| (d.mount_point.clone(), d.name.clone()))
        });
        if let Some((path, name)) = drive {
            ws.open_drive_tab(&path, &name);
            return;
        }
    }
    let Some(app) = ws.active_browser_mut() else {
        return;
    };
    match key.code {
        KeyCode::Up => {
            if let Some(nv) = app.net.as_mut() {
                nv.move_selection(-1);
            }
        }
        KeyCode::Down => {
            if let Some(nv) = app.net.as_mut() {
                nv.move_selection(1);
            }
        }
        KeyCode::Home => {
            if let Some(nv) = app.net.as_mut() {
                nv.jump_to(0);
            }
        }
        KeyCode::End => {
            if let Some(nv) = app.net.as_mut() {
                nv.jump_to(usize::MAX);
            }
        }
        KeyCode::Enter | KeyCode::Right => app.net_enter(),
        KeyCode::Left | KeyCode::Esc => app.net_back(),
        // Add a host by hand (IP or hostname) for devices that don't
        // advertise over mDNS.
        KeyCode::Char('m') => {
            if app
                .net
                .as_ref()
                .is_some_and(|nv| matches!(nv.state, NetState::Devices))
            {
                app.start_input(InputKind::NetHost);
            }
        }
        _ => {}
    }
}

/// Key handling inside the built-in editor. Ctrl+S saves, Ctrl+Z / Ctrl+Y
/// undo and redo, Esc closes (asking first when the buffer has unsaved
/// changes, clearing a mouse selection first).
fn handle_editor(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    // The save-as dialog eats every key while it's open.
    if app.save_dialog.is_some() {
        app.save_dialog_key(key);
        return;
    }
    // Ctrl+R opens find & replace in the editor. Placed before the find-bar
    // routing so Ctrl+R switches an open Ctrl+F bar into replace mode
    // instead of being eaten as query text (r is free in the editor).
    if ctrl && !shift && matches!(key.code, KeyCode::Char('r') | KeyCode::Char('R')) {
        if let Some(ed) = app.editor.as_mut() {
            ed.open_replace();
        }
        return;
    }
    // Ctrl+F toggles the find bar in the editor (f is free: s/o/z/y/c/x/v
    // are already taken).
    if ctrl && !shift && matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F')) {
        if let Some(ed) = app.editor.as_mut() {
            if ed.find.is_some() {
                ed.close_find();
            } else {
                ed.open_find();
            }
        }
        return;
    }
    // While the find bar is open it eats every key, like the save dialog:
    // Esc closes it, Enter jumps to the next match, Shift+Enter to the
    // previous one (only where the terminal delivers it), everything else
    // edits the query — never the document. In replace mode (Ctrl+R) Enter
    // replaces the current match instead, Tab switches the Find/Replace
    // fields, and Ctrl+A replaces every match.
    if app.editor.as_ref().is_some_and(|ed| ed.find.is_some()) {
        if let Some(ed) = app.editor.as_mut() {
            match key.code {
                KeyCode::Esc => ed.close_find(),
                KeyCode::Enter => {
                    if ed.find.as_ref().is_some_and(|f| f.replace_mode) {
                        ed.replace_current();
                    } else if shift {
                        ed.find_jump(-1);
                    } else {
                        ed.find_jump(1);
                    }
                }
                _ => {
                    ed.find_input(key);
                }
            }
        }
        return;
    }
    // Ctrl+S saves; Ctrl+O is a second save binding (nano-style) for
    // terminals that intercept Ctrl+S before the app sees it.
    if ctrl
        && !shift
        && matches!(
            key.code,
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Char('o') | KeyCode::Char('O')
        )
    {
        let untitled = app.editor.as_ref().is_some_and(|ed| ed.is_untitled());
        if untitled {
            // No path yet: browse for a location and file name.
            app.open_save_dialog();
        } else if let Some(ed) = app.editor.as_mut() {
            if let Err(e) = ed.save() {
                ed.message = format!("Save failed: {e}");
            } else {
                app.invalidate_git(); // the file's git status may have changed
            }
        }
        return;
    }
    // Undo / redo: Ctrl+Z undoes, Ctrl+Y or Ctrl+Shift+Z redoes.
    if ctrl && matches!(key.code, KeyCode::Char('z') | KeyCode::Char('Z')) {
        if let Some(ed) = app.editor.as_mut() {
            if shift {
                ed.redo();
            } else {
                ed.undo();
            }
        }
        return;
    }
    if ctrl && !shift && matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
        if let Some(ed) = app.editor.as_mut() {
            ed.redo();
        }
        return;
    }
    // Ctrl+Left/Right: word jumps; with Shift held, extend the selection.
    if ctrl && matches!(key.code, KeyCode::Left | KeyCode::Right) {
        let shifted = shift;
        let moved = match key.code {
            KeyCode::Left => app
                .editor
                .as_mut()
                .map(|ed| {
                    ed.shift_select(shifted);
                    ed.word_start();
                })
                .is_some(),
            _ => app
                .editor
                .as_mut()
                .map(|ed| {
                    ed.shift_select(shifted);
                    ed.word_end();
                })
                .is_some(),
        };
        if moved {
            return;
        }
    }
    // OS-style clipboard: Ctrl+C copies the selection (or current line),
    // Ctrl+X cuts it, Ctrl+V pastes. Copied text never includes the
    // line-number gutter.
    if ctrl && !shift {
        let acted = match key.code {
            KeyCode::Char('c') | KeyCode::Char('C') => {
                app.editor.as_mut().map(|ed| ed.copy_line()).is_some()
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                app.editor.as_mut().map(|ed| ed.cut_line()).is_some()
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                app.editor.as_mut().map(|ed| ed.paste()).is_some()
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                app.editor.as_mut().map(|ed| ed.select_all()).is_some()
            }
            // Ctrl+E is a second select-all: some terminals grab Ctrl+A for
            // their own "select all" so it never reaches fex; Ctrl+E
            // always gets through.
            KeyCode::Char('e') | KeyCode::Char('E') => {
                app.editor.as_mut().map(|ed| ed.select_all()).is_some()
            }
            _ => false,
        };
        if acted {
            return;
        }
    }
    if matches!(key.code, KeyCode::Esc) {
        let has_selection = app.editor.as_ref().is_some_and(|e| e.sel_anchor.is_some());
        if has_selection {
            if let Some(ed) = app.editor.as_mut() {
                ed.clear_selection();
            }
            return;
        }
        let dirty = app.editor.as_ref().is_some_and(|e| e.dirty);
        if dirty {
            app.mode = Mode::ConfirmDiscard;
        } else {
            app.close_editor();
        }
        return;
    }
    let Some(ed) = app.editor.as_mut() else {
        app.mode = Mode::Normal;
        return;
    };
    match key.code {
        KeyCode::Char(c) if !ctrl => ed.insert_char(c),
        KeyCode::Enter => ed.newline(),
        KeyCode::Backspace => ed.backspace(),
        KeyCode::Delete => ed.delete(),
        KeyCode::Left => {
            ed.shift_select(shift);
            ed.move_left();
        }
        KeyCode::Right => {
            ed.shift_select(shift);
            ed.move_right();
        }
        KeyCode::Up => {
            ed.shift_select(shift);
            ed.move_up();
        }
        KeyCode::Down => {
            ed.shift_select(shift);
            ed.move_down();
        }
        KeyCode::Home => {
            ed.shift_select(shift);
            ed.home();
        }
        KeyCode::End => {
            ed.shift_select(shift);
            ed.end();
        }
        KeyCode::PageUp => {
            ed.shift_select(shift);
            ed.page_up();
        }
        KeyCode::PageDown => {
            ed.shift_select(shift);
            ed.page_down();
        }
        KeyCode::Tab => {
            for _ in 0..4 {
                ed.insert_char(' ');
            }
        }
        _ => {}
    }
}

/// Key handling inside the CSV table viewer. Arrows move between cells,
/// Enter edits the current cell, Tab jumps to the next cell, `a` adds a
/// row, `A` adds a column, Ctrl+S saves, Esc closes (asking first about
/// unsaved changes).
fn handle_sheet(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // Unsaved-changes confirmation eats every key except y/n/Esc.
    if app.sheet.as_ref().is_some_and(|s| s.confirm_discard) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => app.close_sheet(),
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(sh) = app.sheet.as_mut() {
                    sh.confirm_discard = false;
                }
            }
            _ => {}
        }
        return;
    }

    // Ctrl+F toggles find-in-sheet; while the bar is open it eats every
    // key (Esc closes it before the dirty-check below can fire).
    if ctrl && !shift && matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F')) {
        if let Some(sh) = app.sheet.as_mut() {
            if sh.find.is_some() {
                sh.close_find();
            } else {
                sh.open_find();
            }
        }
        return;
    }
    if app.sheet.as_ref().is_some_and(|s| s.find.is_some()) {
        if let Some(sh) = app.sheet.as_mut() {
            match key.code {
                KeyCode::Esc => sh.close_find(),
                KeyCode::Enter if !shift => sh.find_jump(1),
                KeyCode::Enter => sh.find_jump(-1),
                _ => {
                    sh.find_input(key);
                }
            }
        }
        return;
    }

    // Ctrl+S saves; Ctrl+O is a second save binding for terminals that
    // intercept Ctrl+S before the app sees it.
    if ctrl
        && !shift
        && matches!(
            key.code,
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Char('o') | KeyCode::Char('O')
        )
    {
        if let Some(sh) = app.sheet.as_mut() {
            if let Err(e) = sh.save() {
                sh.message = format!("Save failed: {e}");
            } else {
                app.invalidate_git(); // the file's git status may have changed
            }
        }
        return;
    }
    if matches!(key.code, KeyCode::Esc) {
        let dirty = app.sheet.as_ref().is_some_and(|s| s.dirty);
        if dirty {
            if let Some(sh) = app.sheet.as_mut() {
                sh.confirm_discard = true;
            }
        } else {
            app.close_sheet();
        }
        return;
    }
    if matches!(key.code, KeyCode::Char('A')) && !ctrl {
        app.start_input(InputKind::CsvColumn);
        return;
    }
    let Some(sh) = app.sheet.as_mut() else {
        app.mode = Mode::Normal;
        return;
    };
    match key.code {
        KeyCode::Left => sh.move_cell(0, -1),
        KeyCode::Right => sh.move_cell(0, 1),
        KeyCode::Up => sh.move_cell(-1, 0),
        KeyCode::Down => sh.move_cell(1, 0),
        KeyCode::Home => {
            sh.col = 0;
            sh.move_cell(0, 0);
        }
        KeyCode::End => {
            sh.col = sh.ncols().saturating_sub(1);
            sh.move_cell(0, 0);
        }
        KeyCode::PageUp => sh.move_cell(-(sh.view_h as isize), 0),
        KeyCode::PageDown => sh.move_cell(sh.view_h as isize, 0),
        KeyCode::Tab => {
            if shift {
                sh.prev_cell();
            } else {
                sh.next_cell();
            }
        }
        KeyCode::Enter => app.start_input(InputKind::CsvCell),
        KeyCode::Char('a') if !ctrl => sh.add_row(),
        // xlsx sheet switching (no-op for CSV).
        KeyCode::Char('[') => sh.switch_sheet(-1),
        KeyCode::Char(']') => sh.switch_sheet(1),
        _ => {}
    }
}

/// A bracketed paste arrived from the terminal as one event: insert it all
/// at once instead of replaying it character by character.
pub fn handle_paste(ws: &mut Workspace, text: String) {
    if text.is_empty() {
        return;
    }
    // In a shell tab the paste goes straight to the shell.
    if ws.is_shell_active() {
        if let Some(Tab::Shell(sh)) = ws.tabs.get_mut(ws.active) {
            sh.send(text.as_bytes());
        }
        return;
    }
    // Pastes only make sense in a file-browser tab.
    let Some(app) = ws.active_browser_mut() else {
        return;
    };
    match app.mode {
        Mode::Editor => {
            if let Some(ed) = app.editor.as_mut() {
                ed.insert_text(&text);
            }
        }
        Mode::Input(_) => app.input_insert_text(&text),
        Mode::Search => {
            app.input_insert_text(&text);
            app.run_search();
        }
        _ => {}
    }
}

/// Mouse handling: click a tab to switch to it, click to move the editor
/// cursor (drag to select), click to select files or table cells, wheel
/// to scroll.
pub fn handle_mouse(ws: &mut Workspace, m: MouseEvent) {
    use ratatui::crossterm::event::{MouseButton, MouseEventKind as Kind};
    // The tab bar is the top row: a left click there switches tabs.
    if m.row == 0 && matches!(m.kind, Kind::Down(MouseButton::Left)) {
        let hits = ws.tab_hits.clone();
        for (x0, x1, idx) in hits {
            if m.column >= x0 && m.column < x1 {
                ws.goto_tab(idx);
                return;
            }
        }
        return;
    }
    // Shell tabs own their whole content area; the mouse does nothing there.
    let Some(app) = ws.active_browser_mut() else {
        return;
    };
    // Note: the editor/sheet/list click handlers take absolute terminal
    // coordinates (their view origins come from the render layout, which
    // already sits below the tab bar), so no row translation is needed here.
    match app.mode {
        Mode::Editor => {
            let Some(ed) = app.editor.as_mut() else {
                return;
            };
            match m.kind {
                Kind::Down(MouseButton::Left) => {
                    ed.click_at(m.column, m.row);
                    ed.sel_anchor = Some((ed.row, ed.col));
                }
                Kind::Drag(MouseButton::Left) => {
                    ed.click_at(m.column, m.row);
                }
                Kind::Up(MouseButton::Left) => {
                    if ed.sel_anchor == Some((ed.row, ed.col)) {
                        ed.sel_anchor = None; // plain click, not a drag
                    }
                }
                Kind::ScrollUp => ed.scroll_by(-3),
                Kind::ScrollDown => ed.scroll_by(3),
                _ => {}
            }
        }
        Mode::Sheet => {
            let Some(sh) = app.sheet.as_mut() else {
                return;
            };
            match m.kind {
                Kind::Down(MouseButton::Left) => sh.click_at(m.column, m.row),
                Kind::ScrollUp => sh.scroll_by(-3),
                Kind::ScrollDown => sh.scroll_by(3),
                _ => {}
            }
        }
        Mode::Normal => match m.kind {
            Kind::Down(MouseButton::Left) => {
                let now = Instant::now();
                let double = matches!(app.last_click,
                    Some((px, py, t)) if px == m.column && py == m.row && now.duration_since(t) <= DOUBLE_CLICK);
                app.last_click = Some((m.column, m.row, now));
                // Select the clicked row first: a double-click acts on it,
                // and double-clicking an archive opens the read-only
                // archive tab instead of the normal enter.
                app.click_select(m.column, m.row);
                let arch_src: Option<PathBuf> = if double && app.archive.is_none() {
                    app.selected_entry()
                        .filter(|e| fs::is_archive(&e.path))
                        .map(|e| e.path.clone())
                } else {
                    None
                };
                if double {
                    match arch_src {
                        // `app` is not used in this branch, so its borrow
                        // ends here and `ws` can be used.
                        Some(src) => ws.open_archive_tab(src),
                        None => app.enter_selected(),
                    }
                }
            }
            Kind::ScrollUp => app.move_selection(-3),
            Kind::ScrollDown => app.move_selection(3),
            _ => {}
        },
        _ => {}
    }
}

fn handle_normal(ws: &mut Workspace, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    // The move dialog eats every key while it's open.
    if ws.active_browser().is_some_and(|a| a.move_dialog.is_some()) {
        if let Some(app) = ws.active_browser_mut() {
            app.move_dialog_key(key);
        }
        return;
    }
    // Workspace-level keys: shared clipboard, quit, new terminal tab.
    match key.code {
        // OS-style clipboard for files and folders, shared across tabs
        // (the y/x/p keys below keep working as aliases).
        KeyCode::Char('c') | KeyCode::Char('C') if ctrl => {
            ws.yank(false);
            return;
        }
        KeyCode::Char('x') | KeyCode::Char('X') if ctrl => {
            ws.yank(true);
            return;
        }
        KeyCode::Char('v') | KeyCode::Char('V') if ctrl => {
            ws.paste();
            return;
        }
        KeyCode::Char('y') => {
            ws.yank(false);
            return;
        }
        KeyCode::Char('x') => {
            ws.yank(true);
            return;
        }
        KeyCode::Char('p') => {
            ws.paste();
            return;
        }
        // Backtick opens a terminal tab (a real shell, not a subshell).
        KeyCode::Char('`') => {
            ws.new_shell_tab();
            return;
        }
        KeyCode::Char('q') => {
            ws.should_quit = true;
            return;
        }
        KeyCode::Esc => {
            // In an archive tab Esc climbs out: one virtual level up, or
            // closes the tab at the archive root (the last tab becomes a
            // normal browser instead of closing).
            if ws.active_browser().is_some_and(|a| a.archive.is_some()) {
                ws.archive_back();
                return;
            }
            let quit = match ws.active_browser_mut() {
                Some(app) if !app.multi.is_empty() => {
                    app.clear_multi();
                    app.status.clear();
                    false
                }
                Some(app) if app.filter.is_empty() => true,
                Some(app) => {
                    app.clear_filter();
                    false
                }
                None => true,
            };
            if quit {
                ws.should_quit = true;
            }
            return;
        }
        _ => {}
    }
    // Navigation that may need the workspace: opening an archive creates
    // a new tab (App can't do that alone), and ←/Backspace in an archive
    // tab climbs out or closes the tab. Runs before the `app` borrow
    // below so it can take `&mut ws`.
    match key.code {
        KeyCode::Right | KeyCode::Enter => {
            // An archive opens in a new read-only tab that browses inside
            // it; everything else falls through to the normal enter.
            if ws.open_archive_if_selected() {
                return;
            }
        }
        KeyCode::Left | KeyCode::Backspace => {
            if ws.active_browser().is_some_and(|a| a.archive.is_some()) {
                ws.archive_back();
                return;
            }
        }
        _ => {}
    }
    let Some(app) = ws.active_browser_mut() else {
        return;
    };
    match key.code {
        // Navigation
        KeyCode::Up if shift => app.shift_extend(-1),
        KeyCode::Down if shift => app.shift_extend(1),
        KeyCode::Up => app.move_selection(-1),
        KeyCode::Down => app.move_selection(1),
        KeyCode::PageUp => app.move_selection(-10),
        KeyCode::PageDown => app.move_selection(10),
        KeyCode::Home => app.jump_to(0),
        KeyCode::End => app.jump_to(usize::MAX),
        KeyCode::Right | KeyCode::Enter => app.enter_selected(),
        KeyCode::Left | KeyCode::Backspace => app.go_to_parent(),

        // Views: 1 = list, 2 = Miller columns
        KeyCode::Char('1') => app.exit_column_mode(),
        KeyCode::Char('2') => app.enter_column_mode(),

        // Preview pane (off by default)
        KeyCode::Char('P') => app.toggle_preview(),

        // Filter / sort / hidden / search
        KeyCode::Char('/') => {
            if app.view == ViewMode::Columns {
                app.exit_column_mode();
            }
            app.start_input(InputKind::Filter);
        }
        // Find in files (Ctrl+F): contents search on a background thread.
        // Plain f keeps its filename search.
        KeyCode::Char('f') | KeyCode::Char('F') if ctrl => {
            app.start_input(InputKind::Grep);
        }
        KeyCode::Char('f') => app.start_search(),
        KeyCode::Char('s') => app.cycle_sort_key(),
        KeyCode::Char('S') => app.toggle_sort_dir(),
        KeyCode::Char('.') => app.toggle_hidden(),

        // File operations
        KeyCode::Char('n') => app.start_input(InputKind::NewFile),
        KeyCode::Char('N') => app.start_input(InputKind::NewDir),
        KeyCode::Char('r') => app.start_input(InputKind::Rename),
        KeyCode::Char('d') => app.confirm_trash(),
        KeyCode::Char('D') => app.confirm_delete(),
        // Move the selected file(s) to another folder (popup dialog).
        KeyCode::Char('m') => app.open_move_dialog(),
        // X extracts archives; lowercase x still cuts.
        KeyCode::Char('X') if !ctrl => app.extract_selected(),
        KeyCode::Char('Y') => app.copy_path(),
        KeyCode::Char('C') if !ctrl => app.copy_preview_text(),
        KeyCode::Char('e') => app.open_editor(),

        // Reveal the selected file/folder in the OS file explorer.
        KeyCode::Char('o') => app.reveal_in_explorer(),

        // Network: discover SMB devices on the LAN and mount their shares.
        KeyCode::Char('G') => app.open_network(),

        // Help
        KeyCode::Char('?') => app.mode = Mode::Help,

        // Favorites: star the selected file/folder, F opens the list.
        KeyCode::Char('*') => {
            // Virtual archive entries have no real path to favorite.
            if app.archive.is_some() {
                app.status = String::from("Archives are read-only");
            } else {
                let path = ws
                    .active_browser()
                    .and_then(|a| a.selected_entry())
                    .map(|e| e.path.clone());
                match path {
                    Some(path) => {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let added = ws.toggle_favorite(path);
                        if let Some(app) = ws.active_browser_mut() {
                            app.status = if added {
                                format!("★ {name} added to favorites (F to view)")
                            } else {
                                format!("{name} removed from favorites")
                            };
                        }
                    }
                    None => {
                        if let Some(app) = ws.active_browser_mut() {
                            app.status = String::from("Nothing to favorite");
                        }
                    }
                }
            }
        }
        KeyCode::Char('F') => {
            ws.show_favorites = true;
            ws.fav_sel = 0;
        }
        _ => {}
    }
}

/// Key handling for the modal favorites popup: move, jump, remove, close.
fn handle_favorites(ws: &mut Workspace, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('F') => {
            ws.show_favorites = false;
        }
        KeyCode::Up => {
            ws.fav_sel = ws.fav_sel.saturating_sub(1);
        }
        KeyCode::Down => {
            ws.fav_sel = ws
                .fav_sel
                .saturating_add(1)
                .min(ws.favorites.len().saturating_sub(1));
        }
        KeyCode::Home => ws.fav_sel = 0,
        KeyCode::End => ws.fav_sel = ws.favorites.len().saturating_sub(1),
        KeyCode::Enter => ws.jump_to_favorite(),
        KeyCode::Char('*') | KeyCode::Char('x') | KeyCode::Char('X') => {
            ws.remove_favorite_sel();
        }
        _ => {}
    }
}

fn handle_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_input(),
        KeyCode::Enter => app.submit_input(),
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Left => app.input_move_cursor(-1),
        KeyCode::Right => app.input_move_cursor(1),
        KeyCode::Char(c) => app.input_insert(c),
        _ => {}
    }
}

/// Key handling in search mode: type to search live, Tab toggles
/// filename/deep search, Enter jumps to the hit, Esc exits. In find-in-files
/// (grep) mode typing clears stale results instead of searching live — Enter
/// re-runs the background search, Tab is ignored.
fn handle_search(app: &mut App, key: KeyEvent) {
    let grep = app.grep_mode;
    match key.code {
        KeyCode::Esc => {
            if grep {
                app.grep_rx = None;
                app.grep_active = false;
                app.grep_mode = false;
            }
            app.cancel_search();
        }
        KeyCode::Enter => {
            if grep {
                let q = app.input.clone();
                if app.grep_active || q != app.grep_query || app.search_results.is_empty() {
                    app.start_grep(q);
                } else {
                    app.search_jump();
                }
            } else {
                app.search_jump();
            }
        }
        KeyCode::Tab => {
            if !grep {
                app.toggle_search_deep();
            }
        }
        KeyCode::Backspace => {
            app.input_backspace();
            if grep {
                // Results are stale until Enter re-runs the search.
                app.search_results.clear();
                app.search_selected = 0;
            } else {
                app.run_search();
            }
        }
        KeyCode::Left => app.input_move_cursor(-1),
        KeyCode::Right => app.input_move_cursor(1),
        KeyCode::Up => app.search_move(-1),
        KeyCode::Down => app.search_move(1),
        KeyCode::Char(c) => {
            app.input_insert(c);
            if grep {
                app.search_results.clear();
                app.search_selected = 0;
            } else {
                app.run_search();
            }
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if app.confirm_is_trash {
                app.do_trash()
            } else {
                app.do_delete()
            }
        }
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => app.mode = Mode::Normal,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: mods,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctrl(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn test_app() -> (Workspace, std::path::PathBuf) {
        // Unique per call: tests run in parallel threads sharing one process.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fex-keytest-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.txt");
        std::fs::write(&p, "hello\nworld\n").unwrap();
        let mut ws = Workspace::new(dir.clone());
        // Never touch the real ~/.config from these tests.
        ws.set_config_dir(None);
        (ws, dir)
    }

    fn browser(ws: &Workspace) -> &App {
        ws.active_browser().unwrap()
    }

    fn browser_mut(ws: &mut Workspace) -> &mut App {
        ws.active_browser_mut().unwrap()
    }

    #[test]
    fn search_and_view_keys() {
        let (mut ws, dir) = test_app();
        // f enters search mode; typing narrows to the file.
        handle_key(&mut ws, key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert!(matches!(browser(&ws).mode, Mode::Search));
        handle_key(&mut ws, key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(browser(&ws).search_results.len(), 1);
        // Enter jumps back to the file list with the file selected.
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(browser(&ws).mode, Mode::Normal));
        assert_eq!(browser(&ws).selected_entry().unwrap().name, "a.txt");
        // 2 enters column mode, 1 leaves it.
        handle_key(&mut ws, key(KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(browser(&ws).view, ViewMode::Columns);
        assert!(!browser(&ws).columns.is_empty());
        handle_key(&mut ws, key(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(browser(&ws).view, ViewMode::List);
        // C copies preview text (no clipboard tool in CI → graceful status).
        handle_key(&mut ws, key(KeyCode::Char('C'), KeyModifiers::SHIFT));
        assert!(
            browser(&ws).status == "Copied preview text"
                || browser(&ws).status == "Clipboard unavailable",
            "status was {:?}",
            browser(&ws).status
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn deep_search_finds_content() {
        let (mut ws, dir) = test_app();
        std::fs::write(dir.join("b.txt"), "nothing\nneedle here\n").unwrap();
        browser_mut(&mut ws).refresh();
        handle_key(&mut ws, key(KeyCode::Char('f'), KeyModifiers::NONE));
        handle_key(&mut ws, key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(browser(&ws).search_deep);
        for c in "needle".chars() {
            handle_key(&mut ws, key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(browser(&ws).search_results.len(), 1);
        assert_eq!(browser(&ws).search_results[0].line_no, Some(2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn trash_and_delete_keybindings() {
        let (mut ws, dir) = test_app();
        // d opens the trash confirm; y trashes the selected file.
        handle_key(&mut ws, key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(browser(&ws).mode, Mode::ConfirmDelete));
        assert!(browser(&ws).confirm_is_trash);
        handle_key(&mut ws, key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(!dir.join("a.txt").exists());
        let status = browser(&ws).status.clone();
        assert!(status.contains("trash"), "status was {:?}", status);
        // D opens the permanent-delete confirm; y deletes for real.
        std::fs::write(dir.join("b.txt"), "bye\n").unwrap();
        browser_mut(&mut ws).refresh();
        handle_key(&mut ws, key(KeyCode::Char('D'), KeyModifiers::SHIFT));
        assert!(matches!(browser(&ws).mode, Mode::ConfirmDelete));
        assert!(!browser(&ws).confirm_is_trash);
        handle_key(&mut ws, key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(!dir.join("b.txt").exists());
        let status = browser(&ws).status.clone();
        assert!(status.contains("Deleted"), "status was {:?}", status);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_copies_instead_of_quitting() {
        let (mut ws, dir) = test_app();
        // Explorer: Ctrl+C copies the file, does not quit.
        handle_key(&mut ws, ctrl('c'));
        assert!(!ws.should_quit);
        assert!(
            browser(&ws).status.starts_with("Copied "),
            "status was {:?}",
            browser(&ws).status
        );
        // Explorer: Y copies the path.
        handle_key(&mut ws, key(KeyCode::Char('Y'), KeyModifiers::NONE));
        assert!(
            browser(&ws)
                .status
                .contains(dir.join("a.txt").to_str().unwrap()),
            "status was {:?}",
            browser(&ws).status
        );
        // Open the editor, Ctrl+C copies the line.
        browser_mut(&mut ws).open_editor();
        assert!(matches!(browser(&ws).mode, Mode::Editor));
        handle_key(&mut ws, ctrl('c'));
        assert!(
            matches!(browser(&ws).mode, Mode::Editor),
            "mode is {:?}",
            browser(&ws).mode
        );
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert_eq!(ed.clipboard, vec!["hello".to_string()]);
        assert_eq!(ed.message, "Copied line");
        // Ctrl+V pastes below.
        handle_key(&mut ws, key(KeyCode::Down, KeyModifiers::NONE));
        handle_key(&mut ws, ctrl('v'));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert_eq!(ed.lines, vec!["hello", "world", "hello"]);
        // Ctrl+C still quits from a dialog.
        browser_mut(&mut ws).mode = Mode::ConfirmDelete;
        handle_key(&mut ws, ctrl('c'));
        assert!(ws.should_quit);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_s_and_ctrl_o_save_the_editor() {
        let (mut ws, dir) = test_app();
        browser_mut(&mut ws).open_editor();
        assert!(matches!(browser(&ws).mode, Mode::Editor));
        // Type, then Ctrl+S: the file on disk is updated and dirty clears.
        handle_key(&mut ws, key(KeyCode::Char('!'), KeyModifiers::NONE));
        assert!(browser(&ws).editor.as_ref().unwrap().dirty);
        handle_key(&mut ws, ctrl('s'));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert!(!ed.dirty, "save clears the dirty flag");
        assert!(
            ed.message.starts_with("Saved "),
            "message was {:?}",
            ed.message
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "!hello\nworld\n"
        );
        // Ctrl+O is the alternate save binding for terminals that grab
        // Ctrl+S before it reaches the app.
        handle_key(&mut ws, key(KeyCode::Char('?'), KeyModifiers::NONE));
        handle_key(&mut ws, ctrl('o'));
        assert!(!browser(&ws).editor.as_ref().unwrap().dirty);
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).unwrap(),
            "!?hello\nworld\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_a_and_ctrl_e_select_all_in_editor() {
        let (mut ws, dir) = test_app();
        browser_mut(&mut ws).open_editor();
        assert!(matches!(browser(&ws).mode, Mode::Editor));
        // Ctrl+A selects the whole buffer; Backspace deletes it.
        handle_key(&mut ws, ctrl('a'));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert!(ed.sel_anchor.is_some(), "Ctrl+A selects all");
        handle_key(&mut ws, key(KeyCode::Backspace, KeyModifiers::NONE));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert_eq!(ed.lines, vec!["".to_string()]);
        // Ctrl+E is the fallback select-all for terminals that grab
        // Ctrl+A for their own "select all".
        handle_key(&mut ws, ctrl('z')); // undo the delete
        handle_key(&mut ws, ctrl('e'));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert!(ed.sel_anchor.is_some(), "Ctrl+E selects all");
        assert_eq!(
            ed.selection(),
            Some(((0, 0), (1, 5))),
            "Ctrl+E selects the whole buffer"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sort_key_applies_in_column_mode() {
        let (mut ws, dir) = test_app();
        std::fs::write(dir.join("b.txt"), "x".repeat(100)).unwrap();
        std::fs::write(dir.join("c.txt"), "y").unwrap();
        browser_mut(&mut ws).refresh();
        browser_mut(&mut ws).enter_column_mode();
        assert_eq!(browser(&ws).view, ViewMode::Columns);
        let col_names = |app: &App| {
            app.columns[app.col_active]
                .entries
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>()
        };
        // Default sort is by modified date, newest first. `s` cycles
        // Modified -> Type -> Name -> Size, keeping the direction;
        // the column must re-sort too.
        handle_key(&mut ws, key(KeyCode::Char('s'), KeyModifiers::NONE)); // Type ↓
        handle_key(&mut ws, key(KeyCode::Char('s'), KeyModifiers::NONE)); // Name ↓
        assert_eq!(col_names(browser(&ws)), vec!["c.txt", "b.txt", "a.txt"]);
        // `s` again reaches size sort, still descending.
        handle_key(&mut ws, key(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(col_names(browser(&ws)), vec!["b.txt", "a.txt", "c.txt"]);
        // `S` toggles direction.
        handle_key(&mut ws, key(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert_eq!(col_names(browser(&ws)), vec!["c.txt", "a.txt", "b.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backtick_opens_shell_tab_and_ctrl_w_closes_it() {
        let (mut ws, dir) = test_app();
        // Backtick in the file list opens a terminal tab.
        handle_key(&mut ws, key(KeyCode::Char('`'), KeyModifiers::NONE));
        assert_eq!(ws.tabs.len(), 2);
        assert!(ws.is_shell_active());
        // Keys now go to the shell, not the browser: `q` must not quit.
        handle_key(&mut ws, key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!ws.should_quit);
        // Ctrl+W closes the shell tab and returns to the browser.
        handle_key(&mut ws, key(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(ws.tabs.len(), 1);
        assert!(!ws.is_shell_active());
        assert!(matches!(browser(&ws).mode, Mode::Normal));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn g_opens_network_view_and_esc_closes_it() {
        let (mut ws, dir) = test_app();
        handle_key(&mut ws, key(KeyCode::Char('G'), KeyModifiers::SHIFT));
        {
            let app = browser(&ws);
            assert!(matches!(app.mode, Mode::Network));
            assert!(app.net.is_some());
        }
        // Esc at the top level closes the view.
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        {
            let app = browser(&ws);
            assert!(matches!(app.mode, Mode::Normal));
            assert!(app.net.is_none());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn m_in_network_view_prompts_for_host() {
        let (mut ws, dir) = test_app();
        handle_key(&mut ws, key(KeyCode::Char('G'), KeyModifiers::SHIFT));
        handle_key(&mut ws, key(KeyCode::Char('m'), KeyModifiers::NONE));
        let app = browser(&ws);
        assert!(matches!(app.mode, Mode::Input(InputKind::NetHost)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- mouse: click to select, double-click to open -------------------------

    use ratatui::crossterm::event::{MouseButton, MouseEventKind};

    fn click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::empty(),
        }
    }

    /// Pretend the file list renders at terminal row 4 and return the y
    /// coordinate of the row showing `name`.
    fn row_y_of(ws: &mut Workspace, name: &str) -> u16 {
        let app = browser_mut(ws);
        app.refresh();
        app.list_origin = (0, 4);
        let row = app.visible_row(name).unwrap();
        4 + 1 + row as u16
    }

    #[test]
    fn single_click_selects_without_opening() {
        let (mut ws, dir) = test_app();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let cwd = dir.clone();
        let y = row_y_of(&mut ws, "sub");
        handle_mouse(&mut ws, click(2, y));
        let app = browser(&ws);
        assert_eq!(app.cwd, cwd, "single click must not open the directory");
        assert_eq!(app.selected_entry().unwrap().name, "sub");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn double_click_opens_directory() {
        let (mut ws, dir) = test_app();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let y = row_y_of(&mut ws, "sub");
        // Two clicks in immediate succession count as a double-click.
        handle_mouse(&mut ws, click(2, y));
        handle_mouse(&mut ws, click(2, y));
        let app = browser(&ws);
        assert_eq!(app.cwd, dir.join("sub"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn click_outside_list_is_ignored() {
        let (mut ws, dir) = test_app();
        let before = {
            let app = browser_mut(&mut ws);
            app.refresh();
            app.list_origin = (0, 4);
            app.selected_entry().unwrap().name.clone()
        };
        handle_mouse(&mut ws, click(70, 20)); // far right of the list
        handle_mouse(&mut ws, click(2, 100)); // below the list
        let app = browser(&ws);
        assert_eq!(app.selected_entry().unwrap().name, before);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shift_down_multi_selects_and_esc_clears() {
        let (mut ws, dir) = test_app();
        std::fs::write(dir.join("b.txt"), "x").unwrap();
        browser_mut(&mut ws).refresh();
        browser_mut(&mut ws).jump_to(0);
        handle_key(&mut ws, key(KeyCode::Down, KeyModifiers::SHIFT));
        handle_key(&mut ws, key(KeyCode::Down, KeyModifiers::SHIFT));
        assert_eq!(browser(&ws).multi.len(), 2);
        // Esc clears the selection instead of quitting.
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(browser(&ws).multi.is_empty());
        assert!(!ws.should_quit);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn m_key_opens_move_dialog_and_esc_cancels() {
        let (mut ws, dir) = test_app();
        handle_key(&mut ws, key(KeyCode::Char('m'), KeyModifiers::NONE));
        assert!(browser(&ws).move_dialog.is_some());
        // The dialog eats keys: Esc cancels it.
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(browser(&ws).move_dialog.is_none());
        assert!(!ws.should_quit);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn o_key_reveals_in_explorer() {
        let (mut ws, dir) = test_app();
        handle_key(&mut ws, key(KeyCode::Char('o'), KeyModifiers::empty()));
        let status = browser(&ws).status.clone();
        assert!(
            status.starts_with("Opened ") || status.starts_with("Could not open"),
            "unexpected status: {status}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shift_arrows_select_in_editor() {
        let (mut ws, dir) = test_app();
        browser_mut(&mut ws).open_editor(); // a.txt: "hello\nworld\n"
                                            // Shift+Right x5 selects "hello".
        for _ in 0..5 {
            handle_key(&mut ws, key(KeyCode::Right, KeyModifiers::SHIFT));
        }
        let sel = browser(&ws).editor.as_ref().unwrap().selection();
        assert_eq!(sel, Some(((0, 0), (0, 5))));
        // Typing replaces the selection in one undo step.
        handle_key(&mut ws, key(KeyCode::Char('X'), KeyModifiers::empty()));
        {
            let ed = browser(&ws).editor.as_ref().unwrap();
            assert_eq!(ed.lines[0], "X");
            assert_eq!(ed.sel_anchor, None);
        }
        handle_key(&mut ws, key(KeyCode::Char('z'), KeyModifiers::CONTROL));
        {
            let ed = browser(&ws).editor.as_ref().unwrap();
            assert_eq!(ed.lines[0], "hello");
        }
        // Plain arrows collapse the selection and just move.
        for _ in 0..5 {
            handle_key(&mut ws, key(KeyCode::Left, KeyModifiers::SHIFT));
        }
        {
            let ed = browser(&ws).editor.as_ref().unwrap();
            assert_eq!(ed.selection(), Some(((0, 0), (0, 5))));
        }
        handle_key(&mut ws, key(KeyCode::Left, KeyModifiers::empty()));
        {
            let ed = browser(&ws).editor.as_ref().unwrap();
            assert_eq!(ed.sel_anchor, None);
            assert_eq!((ed.row, ed.col), (0, 0));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn network_enter_on_mapped_drive_opens_tab() {
        let (mut ws, dir) = test_app();
        browser_mut(&mut ws).open_network();
        assert!(matches!(browser(&ws).mode, Mode::Network));
        // Stand-in for a Windows mapped drive: point it at the temp dir.
        let local = dir.to_string_lossy().into_owned();
        if let Some(nv) = browser_mut(&mut ws).net.as_mut() {
            nv.drives.clear();
            nv.devices.clear();
            nv.mapped = vec![crate::net::MappedDrive {
                local: local.clone(),
                remote: r"\\nas\media".to_string(),
            }];
            // Select the Mapped(0) row directly.
            nv.selected = nv
                .rows()
                .iter()
                .position(|r| matches!(r, crate::app::NetRow::Mapped(0)))
                .unwrap();
        }
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(ws.tabs.len(), 2);
        assert_eq!(ws.active, 1);
        match &ws.tabs[1] {
            Tab::Browser(app) => assert_eq!(app.cwd, dir),
            _ => panic!("expected a browser tab"),
        }
    }

    fn ed_cursor(ws: &Workspace) -> (usize, usize) {
        let ed = browser(ws).editor.as_ref().unwrap();
        (ed.row, ed.col)
    }

    #[test]
    fn ctrl_f_find_bar_flow() {
        let (mut ws, dir) = test_app();
        std::fs::write(dir.join("a.txt"), "foo bar\nbaz foo\nfoo qux\n").unwrap();
        browser_mut(&mut ws).refresh();
        browser_mut(&mut ws).open_editor();
        assert!(matches!(browser(&ws).mode, Mode::Editor));
        // Ctrl+F opens the find bar.
        handle_key(&mut ws, ctrl('f'));
        assert!(browser(&ws).editor.as_ref().unwrap().find.is_some());
        // Typing edits the query and jumps live to the first match at/after
        // the cursor; the document is untouched.
        for c in "foo".chars() {
            handle_key(&mut ws, key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert_eq!(ed.find.as_ref().unwrap().query, "foo");
        assert_eq!(ed.lines, vec!["foo bar", "baz foo", "foo qux"]);
        assert!(!ed.dirty);
        assert_eq!(ed_cursor(&ws), (0, 0));
        assert_eq!(ed.find_bar_text(), "Find: foo [1/3]");
        // Enter advances to the next match and wraps around.
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(ed_cursor(&ws), (1, 4));
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(ed_cursor(&ws), (2, 0));
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(ed_cursor(&ws), (0, 0), "Enter wraps to the first match");
        // Shift+Enter goes to the previous match (wraps to the last).
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(ed_cursor(&ws), (2, 0));
        // Esc closes the bar, leaving the cursor at the last match.
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert!(ed.find.is_none());
        assert_eq!(ed_cursor(&ws), (2, 0));
        // Ctrl+F toggles the bar back open and closed again.
        handle_key(&mut ws, ctrl('f'));
        assert!(browser(&ws).editor.as_ref().unwrap().find.is_some());
        handle_key(&mut ws, ctrl('f'));
        assert!(browser(&ws).editor.as_ref().unwrap().find.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn find_bar_eats_keys_and_reports_no_match() {
        let (mut ws, dir) = test_app();
        browser_mut(&mut ws).open_editor(); // a.txt is "hello\nworld\n"
        handle_key(&mut ws, ctrl('f'));
        // A query with no matches sets not_found; the document is untouched.
        for c in "zzz".chars() {
            handle_key(&mut ws, key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert!(ed.find.as_ref().unwrap().not_found);
        assert_eq!(ed.find_bar_text(), "Find: zzz — not found");
        assert_eq!(ed.lines, vec!["hello", "world"]);
        // Ctrl+S is eaten by the bar (no save dialog, no save); Backspace
        // edits the query instead of the document.
        handle_key(&mut ws, ctrl('s'));
        assert!(browser(&ws).editor.as_ref().unwrap().find.is_some());
        handle_key(&mut ws, key(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(
            browser(&ws)
                .editor
                .as_ref()
                .unwrap()
                .find
                .as_ref()
                .unwrap()
                .query,
            "zz"
        );
        assert_eq!(
            browser(&ws).editor.as_ref().unwrap().lines,
            vec!["hello", "world"]
        );
        // Esc closes the bar; a following Esc then closes the editor
        // (unchanged behavior).
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(browser(&ws).editor.as_ref().unwrap().find.is_none());
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(browser(&ws).editor.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_r_replace_bar_flow() {
        let (mut ws, dir) = test_app();
        std::fs::write(dir.join("a.txt"), "foo bar\nbaz foo\nfoo qux\n").unwrap();
        browser_mut(&mut ws).refresh();
        browser_mut(&mut ws).open_editor();
        // Ctrl+F opens the find bar; Ctrl+R switches it into replace mode
        // keeping the query (not eaten as query text).
        handle_key(&mut ws, ctrl('f'));
        for c in "foo".chars() {
            handle_key(&mut ws, key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut ws, ctrl('r'));
        {
            let ed = browser(&ws).editor.as_ref().unwrap();
            let f = ed.find.as_ref().unwrap();
            assert!(f.replace_mode);
            assert_eq!(f.query, "foo");
            assert!(!f.replace_active);
        }
        // Tab moves to the Replace field; typing lands there.
        handle_key(&mut ws, key(KeyCode::Tab, KeyModifiers::NONE));
        for c in "qux".chars() {
            handle_key(&mut ws, key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(
            browser(&ws).editor.as_ref().unwrap().find_bar_text(),
            "Find: foo → Replace: qux█ [1/3]"
        );
        // Enter replaces the current match and advances to the next one.
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert_eq!(ed.lines[0], "qux bar");
        assert_eq!(ed_cursor(&ws), (1, 4));
        assert_eq!(ed.find_bar_text(), "Find: foo → Replace: qux█ [1/2]");
        assert!(ed.dirty);
        // Ctrl+A replaces the rest; Esc closes the bar (the bar eats Ctrl+Z
        // while open, so undo happens after). One undo restores the Ctrl+A
        // batch, a second undo restores the Enter replacement too.
        handle_key(&mut ws, ctrl('a'));
        let ed = browser(&ws).editor.as_ref().unwrap();
        assert_eq!(ed.lines, vec!["qux bar", "baz qux", "qux qux"]);
        assert!(ed.find_bar_text().contains("— replaced 2"));
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(browser(&ws).editor.as_ref().unwrap().find.is_none());
        handle_key(&mut ws, ctrl('z'));
        assert_eq!(
            browser(&ws).editor.as_ref().unwrap().lines,
            vec!["qux bar", "baz foo", "foo qux"]
        );
        handle_key(&mut ws, ctrl('z'));
        assert_eq!(
            browser(&ws).editor.as_ref().unwrap().lines,
            vec!["foo bar", "baz foo", "foo qux"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn x_extracts_selected_archive_into_named_folder() {
        let (mut ws, dir) = test_app();
        // Build a real zip inside the browser dir.
        let src = dir.join("bundle.zip");
        {
            let f = std::fs::File::create(&src).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("a.txt", opts).unwrap();
            use std::io::Write as _;
            w.write_all(b"hello").unwrap();
            w.finish().unwrap();
        }
        browser_mut(&mut ws).refresh();
        browser_mut(&mut ws).select_by_name("bundle.zip");
        handle_key(&mut ws, key(KeyCode::Char('X'), KeyModifiers::empty()));
        let app = browser(&ws);
        assert!(
            app.status.starts_with("Extracted 1 files to bundle/"),
            "status was {:?}",
            app.status
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("bundle").join("a.txt")).unwrap(),
            "hello"
        );
        // Extracting again must not collide: a fresh non-colliding folder.
        browser_mut(&mut ws).select_by_name("bundle.zip");
        handle_key(&mut ws, key(KeyCode::Char('X'), KeyModifiers::empty()));
        assert!(
            browser(&ws).status.contains("bundle copy/"),
            "status was {:?}",
            browser(&ws).status
        );
        assert!(dir.join("bundle copy").join("a.txt").exists());
        // X on a non-archive: quiet status, nothing happens. x still cuts.
        browser_mut(&mut ws).select_by_name("a.txt");
        handle_key(&mut ws, key(KeyCode::Char('X'), KeyModifiers::empty()));
        assert_eq!(browser(&ws).status, "No archive selected");
        handle_key(&mut ws, key(KeyCode::Char('x'), KeyModifiers::empty()));
        assert!(ws.clipboard.is_some(), "x must still cut files");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sheet_ctrl_f_find_bar_routing() {
        let (mut ws, dir) = test_app();
        std::fs::write(dir.join("d.csv"), "name,city\nfoo,bar\nbaz,foo\n").unwrap();
        browser_mut(&mut ws).refresh();
        browser_mut(&mut ws).select_by_name("d.csv");
        let path = dir.join("d.csv");
        browser_mut(&mut ws).open_path_in_sheet(&path);
        assert!(matches!(browser(&ws).mode, Mode::Sheet));
        // Ctrl+F opens the bar; typing jumps live; Enter goes to the
        // next match; Esc closes the bar, not the sheet.
        handle_key(&mut ws, ctrl('f'));
        assert!(browser(&ws).sheet.as_ref().unwrap().find.is_some());
        for c in "foo".chars() {
            handle_key(&mut ws, key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let sh = browser(&ws).sheet.as_ref().unwrap();
        assert_eq!((sh.row, sh.col), (0, 0));
        assert_eq!(sh.find_bar_text(), "Find: foo [1/2]");
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        let sh = browser(&ws).sheet.as_ref().unwrap();
        assert_eq!((sh.row, sh.col), (1, 1));
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(browser(&ws).sheet.as_ref().unwrap().find.is_none());
        assert!(matches!(browser(&ws).mode, Mode::Sheet));
        // Ctrl+F toggles back open.
        handle_key(&mut ws, ctrl('f'));
        assert!(browser(&ws).sheet.as_ref().unwrap().find.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_f_browser_starts_grep_search() {
        let (mut ws, dir) = test_app();
        std::fs::write(dir.join("n.txt"), "needle in a haystack\n").unwrap();
        // Ctrl+F in the browser opens the find-in-files prompt.
        handle_key(&mut ws, ctrl('f'));
        assert!(matches!(browser(&ws).mode, Mode::Input(InputKind::Grep)));
        // Type a query; Enter runs it on the background thread.
        for c in "needle".chars() {
            handle_key(&mut ws, key(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut ws, key(KeyCode::Enter, KeyModifiers::NONE));
        {
            let app = browser(&ws);
            assert!(matches!(app.mode, Mode::Search));
            assert!(app.grep_mode);
            assert!(app.grep_active);
            assert!(app.grep_rx.is_some());
        }
        // Typing clears stale results; Tab is ignored; Esc exits.
        handle_key(&mut ws, key(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(browser(&ws).search_results.is_empty());
        handle_key(&mut ws, key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(browser(&ws).grep_mode);
        handle_key(&mut ws, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(browser(&ws).mode, Mode::Normal));
        assert!(!browser(&ws).grep_mode);
        assert!(browser(&ws).grep_rx.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
