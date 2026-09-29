//! Keyboard input handling, dispatched by UI mode.

use crate::app::{App, InputKind, Mode};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Ctrl+C always quits, from any mode.
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.should_quit = true;
        return;
    }
    // Each new keypress in a non-input mode starts with a fresh status line;
    // operations below set their own message afterwards.
    if !matches!(app.mode, Mode::Input(_)) {
        app.status.clear();
    }
    match app.mode {
        Mode::Normal => handle_normal(app, key),
        Mode::Input(_) => handle_input(app, key),
        Mode::ConfirmDelete => handle_confirm(app, key),
        Mode::Editor => handle_editor(app, key),
        Mode::ConfirmDiscard => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => app.close_editor(),
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => app.mode = Mode::Editor,
            _ => {}
        },
        Mode::Help => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')) {
                app.mode = Mode::Normal;
            }
        }
    }
}

/// Key handling inside the built-in editor. Ctrl+S saves; Esc closes
/// (asking first when the buffer has unsaved changes).
fn handle_editor(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('s')) {
        if let Some(ed) = app.editor.as_mut() {
            if let Err(e) = ed.save() {
                ed.message = format!("Save failed: {e}");
            }
        }
        return;
    }
    if matches!(key.code, KeyCode::Esc) {
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

fn handle_normal(app: &mut App, key: KeyEvent) {
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

        // Filter / sort / hidden
        KeyCode::Char('/') => app.start_input(InputKind::Filter),
        KeyCode::Char('s') => app.cycle_sort_key(),
        KeyCode::Char('S') => app.toggle_sort_dir(),
        KeyCode::Char('.') => app.toggle_hidden(),

        // File operations
        KeyCode::Char('n') => app.start_input(InputKind::NewFile),
        KeyCode::Char('N') => app.start_input(InputKind::NewDir),
        KeyCode::Char('r') => app.start_input(InputKind::Rename),
        KeyCode::Char('d') => app.confirm_delete(),
        KeyCode::Char('y') => app.yank(false),
        KeyCode::Char('x') => app.yank(true),
        KeyCode::Char('p') => app.paste(),
        KeyCode::Char('e') => app.open_editor(),

        // Help / quit
        KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            if app.filter.is_empty() {
                app.should_quit = true;
            } else {
                app.clear_filter();
            }
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

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => app.do_delete(),
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => app.mode = Mode::Normal,
        _ => {}
    }
}
