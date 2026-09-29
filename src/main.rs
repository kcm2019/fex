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

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut ws = Workspace::new(cwd);
    // Bring back last session's tabs (browser, editor, terminal) when there
    // is a saved session; otherwise the fresh single tab above stands.
    ws.restore_session();
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
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => input::handle_key(ws, key),
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
