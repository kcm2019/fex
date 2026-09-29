//! Rendering: layout, file list, preview pane, status bar, and popups.

use crate::app::{App, InputKind, Mode};
use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::time::SystemTime;

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn human_time(modified: Option<SystemTime>) -> String {
    match modified {
        Some(t) => {
            let dt: DateTime<Local> = t.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        }
        None => String::from("--"),
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    if matches!(app.mode, Mode::Editor | Mode::ConfirmDiscard) {
        render_editor(frame, app);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(frame, app, layout[0]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(layout[1]);
    render_list(frame, app, main[0]);
    render_preview(frame, app, main[1]);

    render_status(frame, app, layout[2]);
    render_footer(frame, layout[3]);

    match app.mode {
        Mode::Input(kind) => render_input_popup(frame, app, kind, area),
        Mode::ConfirmDelete => render_confirm_popup(frame, app, area),
        Mode::Help => render_help_popup(frame, area),
        Mode::Normal | Mode::Editor | Mode::ConfirmDiscard => {}
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut meta = format!("  {} items", app.visible_count());
    meta.push_str(&format!("  ·  sort: {}", app.sort.label()));
    if !app.filter.is_empty() {
        meta.push_str(&format!("  ·  filter: {}", app.filter));
    }
    if app.show_hidden {
        meta.push_str("  ·  hidden: shown");
    }
    let line = Line::from(vec![
        Span::styled(
            app.cwd.to_string_lossy().into_owned(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(meta, Style::default().fg(Color::DarkGray)),
    ]);
    let header = Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(" fex "));
    frame.render_widget(header, area);
}

fn render_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .visible_entries()
        .map(|e| {
            let (style, name) = if e.is_dir {
                (
                    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
                    format!("{}/", e.name),
                )
            } else {
                (Style::default(), e.name.clone())
            };
            let size = if e.is_dir {
                String::from("<DIR>")
            } else {
                human_size(e.size)
            };
            let line = Line::from(vec![
                Span::styled(name, style),
                Span::styled(
                    format!("  {size:>8}  {}", human_time(e.modified)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Files "))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    let text = app.preview_text().to_string();
    let title = match app.selected_entry() {
        Some(e) => format!(" Preview: {} ", e.name),
        None => String::from(" Preview "),
    };
    let preview = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(preview, area);
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let style = if app.status.is_empty() {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    };
    frame.render_widget(Paragraph::new(app.status.as_str()).style(style), area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let hints = "↑↓ move · →/Enter open · ← back · / filter · e edit · n new · r rename · d delete · y/x/p copy/cut/paste · s sort · . hidden · ? help · q quit";
    let footer = Paragraph::new(hints).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn render_input_popup(frame: &mut Frame, app: &App, kind: InputKind, area: Rect) {
    let popup = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup);
    let cursor = app.cursor.min(app.input.chars().count());
    let left: String = app.input.chars().take(cursor).collect();
    let right: String = app.input.chars().skip(cursor).collect();
    let line = Line::from(vec![
        Span::raw(left),
        Span::styled("█", Style::default().fg(Color::Yellow)),
        Span::styled(right, Style::default().fg(Color::DarkGray)),
    ]);
    let input = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", kind.title())),
    );
    frame.render_widget(input, popup);
}

fn render_confirm_popup(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(50, 22, area);
    frame.render_widget(Clear, popup);
    let name = app.selected_entry().map(|e| e.name.clone()).unwrap_or_default();
    let text = vec![
        Line::from(format!("Delete \"{name}\"?")),
        Line::from(""),
        Line::from(Span::styled(
            "y: yes    n: no",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let confirm = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Confirm delete ")
            .style(Style::default().fg(Color::Red)),
    );
    frame.render_widget(confirm, popup);
}

fn render_editor(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let discard = app.mode == Mode::ConfirmDiscard;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    {
        let Some(ed) = app.editor.as_mut() else {
            return;
        };
        ed.view_h = layout[0].height.saturating_sub(2) as usize; // inside the border
        let num_w = ed.lines.len().to_string().len().max(2);
        let gutter_w = (num_w + 1) as u16; // digits + one trailing space

        let visible = ed.highlight_visible(ed.offset, ed.view_h);
        let text: Vec<Line> = visible
            .into_iter()
            .enumerate()
            .map(|(j, spans)| {
                let lineno = ed.offset + j + 1;
                let mut v = vec![Span::styled(
                    format!("{lineno:>num_w$} "),
                    Style::default().fg(Color::DarkGray),
                )];
                v.extend(spans);
                Line::from(v)
            })
            .collect();

        let title = format!(
            " {} {} ",
            ed.path.display(),
            if ed.dirty { "[modified]" } else { "[saved]" }
        );
        let para =
            Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(para, layout[0]);

        let status = format!(
            "Ln {}, Col {} · {} lines · Ctrl+S save · Esc close",
            ed.row + 1,
            ed.col + 1,
            ed.lines.len()
        );
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
            layout[1],
        );
        frame.render_widget(
            Paragraph::new(ed.message.clone()).style(Style::default().fg(Color::Yellow)),
            layout[2],
        );

        // Put the real terminal cursor on the editor cursor.
        let cx = layout[0].x + 1 + gutter_w + ed.col.min(10_000) as u16;
        let cy = layout[0].y + 1 + ed.row.saturating_sub(ed.offset) as u16;
        frame.set_cursor_position(Position::new(cx, cy));
    }

    if discard {
        render_discard_popup(frame, area);
    }
}

fn render_discard_popup(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(46, 24, area);
    frame.render_widget(Clear, popup);
    let text = vec![
        Line::from(""),
        Line::from("Discard unsaved changes?"),
        Line::from(""),
        Line::from(vec![
            Span::styled(" y ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" discard    "),
            Span::styled(
                " n / Esc ",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" keep editing"),
        ]),
    ];
    let p = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(" Unsaved changes "));
    frame.render_widget(p, popup);
}

fn render_help_popup(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(62, 72, area);
    frame.render_widget(Clear, popup);
    let rows = [
        ("↑ / ↓", "move selection"),
        ("→ / Enter", "enter directory · open file in default app"),
        ("← / Backspace", "parent directory"),
        ("PgUp / PgDn", "jump 10 entries"),
        ("Home / End", "first / last entry"),
        ("/", "filter list (live, Esc clears)"),
        ("s / S", "cycle sort key · toggle direction"),
        (".", "show / hide hidden files"),
        ("n / N", "new file / new directory"),
        ("r", "rename"),
        ("d", "delete (asks first)"),
        ("y / x / p", "copy / cut / paste"),
        ("e", "edit file in the built-in editor"),
        ("?", "this help"),
        ("q / Esc", "quit"),
        ("Ctrl+C", "quit"),
    ];
    let mut lines: Vec<Line> = rows
        .iter()
        .map(|(keys, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("{keys:<16}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(*desc),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter on a file opens it with the macOS default app. In the editor: arrows/Home/End/PgUp/PgDn move · type to edit · Ctrl+S save · Esc close (asks if unsaved).",
        Style::default().fg(Color::DarkGray),
    )));
    let help = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Help (?/Esc to close) "));
    frame.render_widget(help, popup);
}
