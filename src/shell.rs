//! An interactive shell running in a tab, backed by a pseudo-terminal.
//!
//! Each shell tab spawns the user's `$SHELL` under `portable-pty`; a reader
//! thread pumps pty bytes into a `vt100` parser, whose screen is drawn with
//! ratatui. Keystrokes are translated to the byte sequences a terminal
//! would send (so Ctrl+C really is SIGINT here).

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color as RColor, Modifier, Style};
use ratatui::text::{Line, Span};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;

/// Bytes arriving from the pty, or notice that the shell exited.
enum ShellEvent {
    Output(Vec<u8>),
    Exited,
}

/// The terminal screen state: parser, size, and liveness. Kept separate
/// from the pty handles so it can be unit-tested without spawning.
pub struct ShellState {
    parser: vt100::Parser,
    pub rows: u16,
    pub cols: u16,
    pub exited: bool,
    pub title: String,
}

impl ShellState {
    pub fn new(cols: u16, rows: u16) -> Self {
        let (cols, rows) = (cols.max(1), rows.max(1));
        Self {
            parser: vt100::Parser::new(rows, cols, 0),
            rows,
            cols,
            exited: false,
            title: String::from("shell"),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub fn set_size(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        if cols != self.cols || rows != self.rows {
            self.cols = cols;
            self.rows = rows;
            self.parser.set_size(rows, cols);
        }
    }

    fn vt_color(c: vt100::Color) -> RColor {
        match c {
            vt100::Color::Default => RColor::Reset,
            vt100::Color::Idx(i) => RColor::Indexed(i),
            vt100::Color::Rgb(r, g, b) => RColor::Rgb(r, g, b),
        }
    }

    /// The current screen as ratatui lines, with the cursor drawn reversed.
    pub fn render_lines(&self) -> Vec<Line<'static>> {
        let screen = self.parser.screen();
        let mut lines = Vec::with_capacity(self.rows as usize);
        for r in 0..self.rows {
            let mut spans = Vec::with_capacity(self.cols as usize);
            for c in 0..self.cols {
                let (text, style) = match screen.cell(r, c) {
                    Some(cell) => {
                        let mut style = Style::default()
                            .fg(Self::vt_color(cell.fgcolor()))
                            .bg(Self::vt_color(cell.bgcolor()));
                        if cell.bold() {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if cell.inverse() {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                        (cell.contents(), style)
                    }
                    None => (String::from(" "), Style::default()),
                };
                spans.push(Span::styled(text, style));
            }
            lines.push(Line::from(spans));
        }
        if !screen.hide_cursor() {
            let (cr, cc) = screen.cursor_position();
            if cr < self.rows && cc < self.cols {
                if let Some(line) = lines.get_mut(cr as usize) {
                    if let Some(span) = line.spans.get_mut(cc as usize) {
                        span.style = span.style.add_modifier(Modifier::REVERSED);
                    }
                }
            }
        }
        lines
    }
}

/// A live shell tab: screen state plus the pty handles and reader channel.
pub struct ShellTab {
    pub state: ShellState,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    rx: mpsc::Receiver<ShellEvent>,
}

impl ShellTab {
    /// Spawn an interactive `$SHELL` (fallback `/bin/sh`) in `cwd`.
    pub fn spawn(cols: u16, rows: u16, cwd: &Path) -> std::io::Result<Self> {
        let state = ShellState::new(cols, rows);
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: state.rows,
                cols: state.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| String::from("/bin/sh"));
        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(cwd);
        cmd.args(["-i"]);
        // The vt100 parser speaks xterm; advertise exactly that.
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        // The process outlives this handle; exit is detected when the pty
        // reader hits EOF. Dropping the master later delivers SIGHUP.
        let _ = child;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(ShellEvent::Output(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = tx.send(ShellEvent::Exited);
        });

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        Ok(Self {
            state,
            master: pair.master,
            writer,
            rx,
        })
    }

    /// Drain any bytes the reader thread delivered into the parser.
    /// Drain all pending pty output into the screen. Returns true if anything
    /// arrived (the screen may have changed).
    pub fn poll(&mut self) -> bool {
        let mut got = false;
        while let Ok(ev) = self.rx.try_recv() {
            got = true;
            match ev {
                ShellEvent::Output(b) => self.state.process(&b),
                ShellEvent::Exited => self.state.exited = true,
            }
        }
        got
    }

    /// Send raw bytes to the shell. Failures mean the pty is gone.
    pub fn send(&mut self, bytes: &[u8]) {
        if self.writer.write_all(bytes).is_err() || self.writer.flush().is_err() {
            self.state.exited = true;
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if self.state.cols == cols.max(1) && self.state.rows == rows.max(1) {
            return;
        }
        self.state.set_size(cols, rows);
        let _ = self.master.resize(PtySize {
            rows: self.state.rows,
            cols: self.state.cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
}

/// Translate a crossterm key event into the bytes a real terminal would
/// send for it. Returns `None` for keys with no terminal meaning.
pub fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Char(c) => {
            if ctrl && !alt {
                let lower = c.to_ascii_lowercase();
                if ('a'..='z').contains(&lower) {
                    return Some(vec![lower as u8 - b'a' + 1]);
                }
                return match c {
                    ' ' | '@' => Some(vec![0x00]),
                    '[' => Some(vec![0x1b]),
                    '\\' => Some(vec![0x1c]),
                    ']' => Some(vec![0x1d]),
                    '^' => Some(vec![0x1e]),
                    '_' => Some(vec![0x1f]),
                    '?' => Some(vec![0x7f]),
                    _ => None,
                };
            }
            if alt {
                // Meta: ESC followed by the character.
                let mut v = vec![0x1b];
                v.extend_from_slice(c.to_string().as_bytes());
                return Some(v);
            }
            Some(c.to_string().into_bytes())
        }
        KeyCode::Enter => Some(b"\r".to_vec()),
        KeyCode::Backspace => Some(b"\x7f".to_vec()),
        KeyCode::Esc => Some(b"\x1b".to_vec()),
        KeyCode::Tab => Some(b"\t".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::F(n) => Some(match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => return None,
        }),
        _ => None,
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

    #[test]
    fn key_mapping_basics() {
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('a'), none)),
            Some(b"a".to_vec())
        );
        // Ctrl+C is the interrupt byte, not a copy shortcut, in a shell tab.
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('c'), ctrl)),
            Some(vec![0x03])
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('z'), ctrl)),
            Some(vec![0x1a])
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Enter, none)),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Up, none)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            key_to_bytes(&key(KeyCode::Char('b'), KeyModifiers::ALT)),
            Some(b"\x1bb".to_vec())
        );
    }

    #[test]
    fn parser_renders_text_and_colors() {
        let mut st = ShellState::new(10, 3);
        st.process(b"hello\x1b[31mRED\x1b[0m");
        let lines = st.render_lines();
        assert_eq!(lines.len(), 3);
        let first: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first.starts_with("helloRED"), "was {first:?}");
        // The colored cells carry the red foreground.
        let red_span = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "R")
            .unwrap();
        assert_eq!(red_span.style.fg, Some(RColor::Indexed(1)));
    }

    #[test]
    fn cursor_is_drawn_reversed() {
        let mut st = ShellState::new(10, 2);
        st.process(b"ab");
        let lines = st.render_lines();
        // Cursor sits after "ab"; that cell should be reversed.
        let cur = &lines[0].spans[2];
        assert!(cur.style.add_modifier.contains(Modifier::REVERSED));
    }
}
