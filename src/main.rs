//! fex — a terminal file explorer written in Rust.

mod app;
mod editor;
mod fs;
mod input;
mod net;
mod session;
mod sheet;
mod shell;
mod theme;
mod ui;

use app::Workspace;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{
            self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
            EnableMouseCapture, Event, KeyEventKind,
        },
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    Terminal,
};
use std::io;
use std::time::Duration;

fn main() -> io::Result<()> {
    // Optional CLI path: `fex [path]`. A directory opens there; a file
    // opens its parent folder with the file selected. An explicit path
    // wins over session restore.
    let arg = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let has_arg = arg.is_some();
    let start_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let (cwd, select_name): (std::path::PathBuf, Option<String>) = match arg {
        Some(p) => {
            let p = if p.is_absolute() {
                p
            } else {
                start_dir.join(p)
            };
            if p.is_file() {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
                let parent = p
                    .parent()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                (parent, name)
            } else {
                (p, None)
            }
        }
        None => (start_dir, None),
    };
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut ws = Workspace::new(cwd);
    if has_arg {
        // An explicit path starts fresh (no session restore) and selects
        // the named file when one was given.
        if let Some(app) = ws.active_browser_mut() {
            app.refresh();
            if let Some(name) = select_name {
                app.select_file_by_name(&name);
            }
        }
    } else {
        // Bring back last session's tabs (browser, editor, terminal) when there
        // is a saved session; otherwise the fresh single tab above stands.
        ws.restore_session();
    }
    let result = run(&mut terminal, &mut ws);
    // Best effort: unmount network shares before the terminal is restored.
    ws.unmount_all();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ws: &mut Workspace,
) -> io::Result<()> {
    // Initial full draw; afterwards redraw only when something changed so an
    // idle app stays quiet (no per-tick output, no wasted CPU).
    terminal.draw(|f| ui::render(f, ws))?;
    loop {
        // Drain any shell output; redraw if it produced new screen content.
        let mut dirty = ws.poll_shell();
        // Drain network discovery and worker messages too.
        dirty |= ws.poll_network();
        // Drain finished background find-in-files (Ctrl+F) searches.
        dirty |= ws.poll_grep();
        // Drain finished background `git status` badge workers.
        dirty |= ws.poll_git();
        if event::poll(Duration::from_millis(100))? {
            // Drain the whole pending burst before redrawing: a paste that
            // arrives as individual key events (no bracketed paste, e.g.
            // under tmux) or fast typing is applied as one update instead
            // of flickering through one redraw per event.
            loop {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        input::handle_key(ws, key)
                    }
                    // Bracketed paste: the terminal delivers a whole paste as one
                    // event, so pasting is instant (no char-by-char replay).
                    Event::Paste(text) => input::handle_paste(ws, text),
                    Event::Mouse(m) => input::handle_mouse(ws, m),
                    // Keep shell tabs sized to their content area (below the tab bar).
                    Event::Resize(cols, rows) => ws.resize_shell_tabs(cols, rows.saturating_sub(1)),
                    _ => {}
                }
                // Handled input may have changed the UI; a terminal resize is
                // picked up by the next draw's autoresize.
                dirty = true;
                if ws.should_quit {
                    break;
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        } else {
            // No input this tick: pick up files that appeared or vanished
            // behind our back (downloads, other programs).
            dirty |= ws.poll_external_changes();
        }
        if dirty {
            terminal.draw(|f| ui::render(f, ws))?;
        }
        if ws.should_quit {
            // Save the session (tabs + backups for unsaved editors) before
            // the terminal is restored, so a relaunch brings it all back.
            ws.save_session();
            return Ok(());
        }
    }
}
