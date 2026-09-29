//! Keyboard input handling, dispatched by UI mode.

use crate::app::{App, InputKind, Mode, Tab, ViewMode, Workspace};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

pub fn handle_key(ws: &mut Workspace, key: KeyEvent) {
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
    }
}

/// Key handling inside the built-in editor. Ctrl+S saves, Ctrl+Z / Ctrl+Y
/// undo and redo, Esc closes (asking first when the buffer has unsaved
/// changes, clearing a mouse selection first).
fn handle_editor(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    if ctrl && !shift && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S')) {
        if let Some(ed) = app.editor.as_mut() {
            if let Err(e) = ed.save() {
                ed.message = format!("Save failed: {e}");
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
            KeyCode::Left => app.editor.as_mut().map(|ed| ed.word_start()).is_some(),
            KeyCode::Right => app.editor.as_mut().map(|ed| ed.word_end()).is_some(),
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
        KeyCode::Char(c) => ed.insert_char(c),
        KeyCode::Enter => ed.newline(),
        KeyCode::Backspace => ed.backspace(),
        KeyCode::Delete => ed.delete(),
        KeyCode::Left => ed.move_left(),
        KeyCode::Right => ed.move_right(),
        KeyCode::Up => ed.move_up(),
        KeyCode::Down => ed.move_down(),
        KeyCode::Home => ed.home(),
        KeyCode::End => ed.end(),
        KeyCode::PageUp => ed.page_up(),
        KeyCode::PageDown => ed.page_down(),
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

    if ctrl && !shift && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S')) {
        if let Some(sh) = app.sheet.as_mut() {
            if let Err(e) = sh.save() {
                sh.message = format!("Save failed: {e}");
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
            Kind::Down(MouseButton::Left) => app.click_select(m.column, m.row),
            Kind::ScrollUp => app.move_selection(-3),
            Kind::ScrollDown => app.move_selection(3),
            _ => {}
        },
        _ => {}
    }
}

fn handle_normal(ws: &mut Workspace, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
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
            let empty = ws.active_browser().is_some_and(|a| a.filter.is_empty());
            if empty {
                ws.should_quit = true;
            } else if let Some(app) = ws.active_browser_mut() {
                app.clear_filter();
            }
            return;
        }
        _ => {}
    }
    let Some(app) = ws.active_browser_mut() else {
        return;
    };
    match key.code {
        // Navigation
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

        // Filter / sort / hidden / search
        KeyCode::Char('/') => {
            if app.view == ViewMode::Columns {
                app.exit_column_mode();
            }
            app.start_input(InputKind::Filter);
        }
        KeyCode::Char('f') => app.start_search(),
        KeyCode::Char('s') => app.cycle_sort_key(),
        KeyCode::Char('S') => app.toggle_sort_dir(),
        KeyCode::Char('.') => app.toggle_hidden(),

        // File operations
        KeyCode::Char('n') => app.start_input(InputKind::NewFile),
        KeyCode::Char('N') => app.start_input(InputKind::NewDir),
        KeyCode::Char('r') => app.start_input(InputKind::Rename),
        KeyCode::Char('d') => app.confirm_delete(),
        KeyCode::Char('Y') => app.copy_path(),
        KeyCode::Char('C') if !ctrl => app.copy_preview_text(),
        KeyCode::Char('e') => app.open_editor(),

        // Help
        KeyCode::Char('?') => app.mode = Mode::Help,
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
/// filename/deep search, Enter jumps to the hit, Esc exits.
fn handle_search(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Enter => app.search_jump(),
        KeyCode::Tab => app.toggle_search_deep(),
        KeyCode::Backspace => {
            app.input_backspace();
            app.run_search();
        }
        KeyCode::Left => app.input_move_cursor(-1),
        KeyCode::Right => app.input_move_cursor(1),
        KeyCode::Up => app.search_move(-1),
        KeyCode::Down => app.search_move(1),
        KeyCode::Char(c) => {
            app.input_insert(c);
            app.run_search();
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => app.do_delete(),
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
        (Workspace::new(dir.clone()), dir)
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
        // Default sort is by name.
        assert_eq!(col_names(browser(&ws)), vec!["a.txt", "b.txt", "c.txt"]);
        // `s` cycles to size sort; the column must re-sort too.
        handle_key(&mut ws, key(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(col_names(browser(&ws)), vec!["c.txt", "a.txt", "b.txt"]);
        // `S` toggles direction.
        handle_key(&mut ws, key(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert_eq!(col_names(browser(&ws)), vec!["b.txt", "a.txt", "c.txt"]);
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
}
