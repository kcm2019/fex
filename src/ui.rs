//! Rendering: layout, file list, preview pane, status bar, and popups.

use crate::app::{App, InputKind, Mode, NetRow, NetState, Tab, ViewMode, Workspace};
use crate::editor::Editor;
use crate::fs::{self, Entry, Preview};
use crate::net;
use crate::theme::Theme;
use chrono::{DateTime, Local};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::{
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::path::PathBuf;
use std::time::SystemTime;

fn human_time(modified: Option<SystemTime>) -> String {
    match modified {
        Some(t) => {
            let dt: DateTime<Local> = t.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        }
        None => String::from("--"),
    }
}

/// Git status badge prefix for an entry row: a dim letter (`M` modified,
/// `A` staged/added, `D` deleted, `?` untracked, `R` renamed), or two
/// spaces so names stay aligned when there is no badge. A plain dim
/// foreground — never a background, per the no-solid-backgrounds rule.
fn git_badge_span(app: &App, entry: &Entry) -> Span<'static> {
    let th = app.theme();
    match app.git_badge_for(entry) {
        Some(b) => Span::styled(format!("{b} "), Style::default().fg(th.dim)),
        None => Span::raw("  "),
    }
}

pub fn render(frame: &mut Frame, ws: &mut Workspace) {
    let area = frame.area();
    if area.height == 0 {
        return;
    }
    let th = ws.theme();

    // Tab strip across the top; everything else renders below it.
    let bar = Rect::new(area.x, area.y, area.width, 1);
    render_tab_bar(frame, ws, &th, bar);
    let content = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );

    // The favorites popup is modal: it floats above everything else.
    if ws.show_favorites {
        render_favorites_popup(frame, ws, content);
        return;
    }

    // Terminal tabs render the pty screen; nothing else applies there.
    if ws.is_shell_active() {
        render_shell(frame, ws, content);
        return;
    }
    let favs = ws.favorites.clone();
    let Some(app) = ws.active_browser_mut() else {
        return;
    };

    if matches!(app.mode, Mode::Editor | Mode::ConfirmDiscard) {
        render_editor(frame, app, content);
        return;
    }

    // CSV table viewer. Cell editing pops an input dialog over it.
    if matches!(app.mode, Mode::Sheet | Mode::Input(InputKind::CsvCell)) {
        render_sheet(frame, app, content);
        if let Mode::Input(kind) = app.mode {
            render_input_popup(frame, app, kind, content);
        }
        return;
    }

    // Network device browser. Its input prompts pop over it.
    if matches!(app.mode, Mode::Network)
        || matches!(
            app.mode,
            Mode::Input(
                InputKind::NetHost | InputKind::NetShare | InputKind::SmbUser | InputKind::SmbPass
            )
        )
    {
        render_network(frame, app, content);
        if let Mode::Input(kind) = app.mode {
            render_input_popup(frame, app, kind, content);
        }
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
        .split(content);

    render_header(frame, app, layout[0]);

    match app.mode {
        Mode::Search => render_search(frame, app, layout[1]),
        _ if app.view == ViewMode::Columns => render_columns(frame, app, &favs, layout[1]),
        _ => {
            if app.show_preview {
                let main = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
                    .split(layout[1]);
                render_list(frame, app, &favs, main[0]);
                render_preview(frame, app, main[1]);
            } else {
                render_list(frame, app, &favs, layout[1]);
            }
        }
    }

    render_status(frame, app, layout[2]);
    render_footer(frame, app, layout[3]);

    if app.move_dialog.is_some() {
        render_move_dialog(frame, app, content);
    }

    match app.mode {
        Mode::Input(kind) => render_input_popup(frame, app, kind, content),
        Mode::ConfirmDelete => render_confirm_popup(frame, app, content),
        Mode::Help => render_help_popup(frame, ws, content),
        Mode::Normal
        | Mode::Editor
        | Mode::ConfirmDiscard
        | Mode::Search
        | Mode::Sheet
        | Mode::Network => {}
    }
}

/// The tab strip: one clickable label per tab, the active one highlighted.
fn render_tab_bar(frame: &mut Frame, ws: &mut Workspace, th: &Theme, area: Rect) {
    ws.tab_hits.clear();
    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;
    for i in 0..ws.tabs.len() {
        let title = ws.tab_title(i);
        let label = format!(" {} {} ", i + 1, title);
        let w = label.chars().count() as u16;
        ws.tab_hits.push((x, x + w, i));
        let style = if i == ws.active {
            // No background on the active tab: a solid select background
            // renders as ugly boxes on terminals whose own background
            // isn't the same color. Bold accent text marks it instead.
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th.dim)
        };
        spans.push(Span::styled(label, style));
        x += w;
        if i + 1 < ws.tabs.len() {
            spans.push(Span::styled("│", Style::default().fg(th.dim)));
            x += 1;
        }
    }
    // Right-aligned hint, dimmed. While the Ctrl+G leader is armed it shows
    // what the next key does instead.
    let hint = if ws.goto_pending {
        "Go to tab: 1-9 · n next · p prev · Esc cancel"
    } else {
        "Ctrl+T new tab · Ctrl+N new text doc · ` terminal · click a tab to switch"
    };
    let used = x.saturating_sub(area.x) as usize;
    let pad = (area.width as usize).saturating_sub(used + hint.len() + 1);
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(hint, Style::default().fg(th.dim)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Draw the active shell tab's pty screen.
fn render_shell(frame: &mut Frame, ws: &mut Workspace, area: Rect) {
    let (exited, lines) = match ws.tabs.get_mut(ws.active) {
        Some(Tab::Shell(sh)) => {
            sh.resize(area.width, area.height);
            (sh.state.exited, sh.state.render_lines())
        }
        _ => return,
    };
    let mut lines: Vec<Line> = lines.into_iter().take(area.height as usize).collect();
    if exited {
        lines.push(Line::from(Span::styled(
            " [process exited — Ctrl+W closes this tab]",
            Style::default().fg(ws.theme().dim),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let th = app.theme();
    let mut meta = format!("  {} items", app.visible_count());
    meta.push_str(&format!("  ·  sort: {}", app.sort.label()));
    if !app.filter.is_empty() {
        meta.push_str(&format!("  ·  filter: {}", app.filter));
    }
    if app.show_hidden {
        meta.push_str("  ·  hidden: shown");
    }
    // Archive tabs show the virtual path inside the archive, e.g.
    // `backup.zip/docs/`, instead of the folder holding the archive.
    let path_show = match &app.archive {
        Some(arch) => {
            let name = arch
                .source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| String::from("archive"));
            format!("{name}/{}", arch.prefix)
        }
        None => app.cwd.to_string_lossy().into_owned(),
    };
    let line = Line::from(vec![
        Span::styled(path_show, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(meta, Style::default().fg(th.dim)),
    ]);
    let header = Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(" fex "));
    frame.render_widget(header, area);
}

fn render_list(frame: &mut Frame, app: &mut App, favs: &[PathBuf], area: Rect) {
    let th = app.theme();
    app.list_origin = (area.x, area.y);
    let items: Vec<ListItem> = app
        .visible_entries()
        .map(|e| {
            let (style, name) = if e.is_dir {
                (
                    Style::default().fg(th.heading).add_modifier(Modifier::BOLD),
                    format!("{}/", e.name),
                )
            } else {
                (Style::default(), e.name.clone())
            };
            let size = if e.is_dir {
                String::from("<DIR>")
            } else {
                fs::human_size(e.size)
            };
            let mut spans = Vec::new();
            spans.push(git_badge_span(app, e));
            if favs.contains(&e.path) {
                spans.push(Span::styled(
                    "★ ",
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                ));
            }
            if app.multi.contains(&e.path) {
                spans.push(Span::styled(
                    "● ",
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                ));
            }
            spans.push(Span::styled(name, style));
            spans.push(Span::styled(
                format!("  {size:>8}  {}", human_time(e.modified)),
                Style::default().fg(th.dim),
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Files "))
        .highlight_style(Style::default().fg(th.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

/// A full-width separator line with a centered label, e.g.
/// `────── Drives ──────`. Used to split the Network view into its
/// Drives and Network sections.
fn section_separator(label: &str, width: usize) -> String {
    let label = format!(" {label} ");
    if width <= label.len() {
        return label;
    }
    let fill = width - label.len();
    let left = fill / 2;
    format!("{}{label}{}", "─".repeat(left), "─".repeat(fill - left))
}

/// Network view: discovered SMB devices, or the shares on one device.
/// Selecting a share mounts it and opens it as a regular browser tab.
fn render_network(frame: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let (title, rows) = match app.net.as_ref().map(|nv| &nv.state) {
        Some(NetState::Devices) => {
            let nv = app.net.as_ref().expect("network view");
            // Inner width for the full-width section separators.
            let inner_w = area.width.saturating_sub(2) as usize;
            let rows: Vec<ListItem> = nv
                .rows()
                .iter()
                .map(|row| match row {
                    NetRow::Header(section) => ListItem::new(Line::from(Span::styled(
                        section_separator(section.label(), inner_w),
                        Style::default().fg(th.dim),
                    ))),
                    NetRow::Drive(i) => {
                        let d = &nv.drives[*i];
                        let mut spans = vec![Span::styled(
                            d.name.clone(),
                            Style::default().fg(th.heading).add_modifier(Modifier::BOLD),
                        )];
                        spans.push(Span::styled(
                            format!("  {}", net::drive_space(d)),
                            Style::default().fg(th.dim),
                        ));
                        if d.removable {
                            spans.push(Span::styled("  removable", Style::default().fg(th.dim)));
                        }
                        ListItem::new(Line::from(spans))
                    }
                    NetRow::Mapped(i) => {
                        let m = &nv.mapped[*i];
                        let mut spans = vec![Span::styled(
                            m.local.clone(),
                            Style::default().fg(th.heading).add_modifier(Modifier::BOLD),
                        )];
                        spans.push(Span::styled(
                            format!("  {}", m.remote),
                            Style::default().fg(th.dim),
                        ));
                        spans.push(Span::styled("  mapped", Style::default().fg(th.dim)));
                        ListItem::new(Line::from(spans))
                    }
                    NetRow::Device(i) => {
                        let d = &nv.devices[*i];
                        let mut spans = vec![Span::styled(
                            d.name.clone(),
                            Style::default().fg(th.heading).add_modifier(Modifier::BOLD),
                        )];
                        spans.push(Span::styled(
                            format!("  {}", d.host),
                            Style::default().fg(th.dim),
                        ));
                        if d.manual {
                            spans.push(Span::styled("  manual", Style::default().fg(th.dim)));
                        }
                        if nv.mounted_hosts.iter().any(|h| h == &d.host) {
                            spans.push(Span::styled("  ● mounted", Style::default().fg(th.accent)));
                        }
                        ListItem::new(Line::from(spans))
                    }
                    NetRow::Hint => ListItem::new(Line::from(Span::styled(
                        "No SMB devices found yet — press m to add one by hand",
                        Style::default().fg(th.dim),
                    ))),
                })
                .collect();
            (" Drives & network ", rows)
        }
        Some(NetState::Shares { host, shares }) => {
            let rows: Vec<ListItem> = shares
                .iter()
                .map(|s| {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("//{host}/"), Style::default().fg(th.dim)),
                        Span::styled(
                            s.clone(),
                            Style::default().fg(th.heading).add_modifier(Modifier::BOLD),
                        ),
                    ]))
                })
                .collect();
            (" Network — shares ", rows)
        }
        _ => (" Network ", Vec::new()),
    };

    let selected = app.net.as_ref().map(|nv| nv.selected);
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(selected);
    }
    let list = List::new(rows)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().fg(th.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, layout[0], &mut state);

    // Live status line: scanning, loading, mounting, or errors.
    let message = app
        .net
        .as_ref()
        .map(|nv| nv.message.clone())
        .unwrap_or_default();
    let busy = app.net.as_ref().is_some_and(|nv| {
        matches!(
            nv.state,
            NetState::LoadingShares { .. } | NetState::Mounting { .. }
        )
    });
    let status_text = if busy || !message.is_empty() {
        message
    } else {
        String::new()
    };
    let status = Paragraph::new(status_text).style(Style::default().fg(th.dim));
    frame.render_widget(status, layout[1]);

    let hints = "↑↓ move · Enter open/list · ← back · m add host · Esc close";
    let footer = Paragraph::new(hints).style(Style::default().fg(th.dim));
    frame.render_widget(footer, layout[2]);
}

/// Miller-column view: directory columns side by side, with a file
/// preview panel at the right when the active selection is a file.
fn render_columns(frame: &mut Frame, app: &mut App, favs: &[PathBuf], area: Rect) {
    let show_preview = app.show_preview && app.selected_entry().is_some_and(|e| !e.is_dir);
    let n_panels = app.columns.len() + usize::from(show_preview);
    if n_panels == 0 {
        return;
    }
    let col_w: u16 = 28;
    // Show the last `fit` panels so the active column stays on screen.
    let fit = ((area.width / col_w).max(1) as usize).min(n_panels);
    let start = n_panels - fit;
    let mut x = area.x;
    app.col_hits.clear();
    for (pi, i) in (start..n_panels).enumerate() {
        let last = pi == fit - 1;
        let w = if last {
            area.x + area.width - x
        } else {
            col_w.min(area.x + area.width - x)
        };
        if w < 8 {
            break;
        }
        let rect = Rect {
            x,
            y: area.y,
            width: w,
            height: area.height,
        };
        if i < app.columns.len() {
            app.col_hits.push((i, rect));
            render_column(frame, app, favs, i, i == app.col_active, rect);
        } else {
            render_preview(frame, app, rect);
        }
        x += w;
    }
}

fn render_column(
    frame: &mut Frame,
    app: &App,
    favs: &[PathBuf],
    idx: usize,
    active: bool,
    area: Rect,
) {
    let th = app.theme();
    let Some(col) = app.columns.get(idx) else {
        return;
    };
    let name = col
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| col.path.to_string_lossy().into_owned());
    let items: Vec<ListItem> = col
        .entries
        .iter()
        .map(|e| {
            let (style, label) = if e.is_dir {
                (
                    Style::default().fg(th.heading).add_modifier(Modifier::BOLD),
                    format!("{}/", e.name),
                )
            } else {
                (Style::default(), e.name.clone())
            };
            let mut spans = Vec::new();
            spans.push(git_badge_span(app, e));
            if favs.contains(&e.path) {
                spans.push(Span::styled(
                    "★ ",
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                ));
            }
            if app.multi.contains(&e.path) {
                spans.push(Span::styled(
                    "● ",
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                ));
            }
            spans.push(Span::styled(label, style));
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(col.selected));
    let title = if active {
        format!(" {name} ● ")
    } else {
        format!(" {name} ")
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().fg(th.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut state);
}

/// Search mode: query bar on top, live results below.
fn render_search(frame: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let cursor = app.cursor.min(app.input.chars().count());
    let left: String = app.input.chars().take(cursor).collect();
    let right: String = app.input.chars().skip(cursor).collect();
    let mode_label = if app.search_deep {
        "deep (contents)"
    } else {
        "names"
    };
    let hint = if app.grep_mode {
        "   (Enter: search/jump · Esc: exit)"
    } else {
        "   (Tab: toggle deep · Enter: jump · Esc: exit)"
    };
    let bar = Line::from(vec![
        Span::styled(
            format!(" Search [{mode_label}]: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(left),
        Span::styled("█", Style::default().fg(th.accent)),
        Span::styled(right, Style::default().fg(th.dim)),
        Span::styled(hint, Style::default().fg(th.dim)),
    ]);
    frame.render_widget(
        Paragraph::new(bar).block(Block::default().borders(Borders::ALL).title(" Find ")),
        layout[0],
    );

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|r| {
            let rel = r.path.strip_prefix(&app.cwd).unwrap_or(&r.path);
            let label = match (r.line_no, &r.snippet) {
                (Some(n), Some(s)) => format!("{}:{}  {s}", rel.display(), n),
                _ => format!("{}{}", rel.display(), if r.is_dir { "/" } else { "" }),
            };
            let style = if r.is_dir {
                Style::default().fg(th.heading).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();
    let mut state = ListState::default();
    if !app.search_results.is_empty() {
        state.select(Some(app.search_selected));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Results ({}) ", app.search_results.len())),
        )
        .highlight_style(Style::default().fg(th.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, layout[1], &mut state);
}

fn render_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    let name = app
        .selected_entry()
        .map(|e| e.name.clone())
        .unwrap_or_default();
    let preview = app.preview().clone();
    let title = if name.is_empty() {
        String::from(" Preview ")
    } else {
        format!(" Preview: {name} ")
    };
    let body: Vec<Line> = match &preview {
        Preview::Text(s) | Preview::Dir(s) => vec![Line::from(s.as_str())],
        Preview::Markdown(src) => render_markdown(&th, src),
        Preview::Csv(rows) => csv_preview_lines(&th, rows),
    };
    let preview = Paragraph::new(body)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(preview, area);
}

/// Aligned-column mini table for the CSV preview pane (header in bold).
fn csv_preview_lines(theme: &Theme, rows: &[Vec<String>]) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return vec![Line::from("[empty csv]")];
    }
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; ncols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count().min(20));
        }
    }
    rows.iter()
        .enumerate()
        .map(|(ri, row)| {
            let mut spans = Vec::new();
            for (i, w) in widths.iter().enumerate() {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                let shown: String = cell.chars().take(*w).collect();
                let style = if ri == 0 {
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(theme.header)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(format!("{shown:<width$}  ", width = w), style));
            }
            Line::from(spans)
        })
        .collect()
}

/// One table row for the CSV viewer: padded cells, the selected cell
/// reversed, the header row bold.
/// Dim style for the table grid lines.
fn grid_style(theme: &Theme) -> Style {
    Style::default().fg(theme.dim)
}

fn csv_row_line(
    theme: &Theme,
    cells: &[String],
    widths: &[usize],
    off_col: usize,
    selected_col: Option<usize>,
    is_header: bool,
    gutter: usize,          // row-number gutter width (0 = no gutter)
    row_num: Option<usize>, // 1-based row number; None = blank corner
) -> Line<'static> {
    let grid = grid_style(theme);
    let mut spans = Vec::new();
    // Row-number gutter, right-aligned in dim, mirroring the editor's
    // line numbers. The header and separator rows get a blank corner.
    if gutter > 0 {
        match row_num {
            Some(n) => spans.push(Span::styled(
                format!("{:>width$} ", n, width = gutter),
                Style::default().fg(theme.dim),
            )),
            None => spans.push(Span::raw(" ".repeat(gutter + 1))),
        }
    }
    spans.push(Span::styled("│", grid));
    for (i, w) in widths.iter().enumerate().skip(off_col) {
        let cell = cells.get(i).map(|s| s.as_str()).unwrap_or("");
        let shown: String = cell.chars().take(*w).collect();
        let mut style = if is_header {
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.header)
        } else {
            Style::default()
        };
        if selected_col == Some(i) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        spans.push(Span::styled(format!(" {shown:<width$} ", width = w), style));
        spans.push(Span::styled("│", grid));
    }
    Line::from(spans)
}

/// Horizontal grid separator between the header and the body (`├─┼─┤`),
/// with a blank row-number gutter corner when the table shows one.
fn csv_sep_line(theme: &Theme, widths: &[usize], off_col: usize, gutter: usize) -> Line<'static> {
    let grid = grid_style(theme);
    let mut spans = Vec::new();
    if gutter > 0 {
        spans.push(Span::raw(" ".repeat(gutter + 1)));
    }
    let mut text = String::from("├");
    for (k, w) in widths.iter().enumerate().skip(off_col) {
        if k > off_col {
            text.push('┼');
        }
        text.push_str(&"─".repeat(w + 2));
    }
    text.push('┤');
    spans.push(Span::styled(text, grid));
    Line::from(spans)
}

fn render_sheet(frame: &mut Frame, app: &mut App, area: Rect) {
    let discard = app.sheet.as_ref().is_some_and(|s| s.confirm_discard);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let th = app.theme();
    {
        let Some(sh) = app.sheet.as_mut() else {
            return;
        };
        sh.view_h = layout[0].height.saturating_sub(4).max(1) as usize; // border + header + separator
        sh.view_w = layout[0].width.saturating_sub(2) as usize;
        sh.view_x = layout[0].x;
        sh.view_y = layout[0].y;
        sh.ensure_visible();

        let widths = sh.col_widths();
        let mut lines: Vec<Line> = Vec::with_capacity(sh.view_h + 1);
        // 1-based row numbers in a dim gutter, mirroring the editor.
        let gutter = sh.rows.len().to_string().len().max(1);
        lines.push(csv_row_line(
            &th,
            &sh.headers,
            &widths,
            sh.off_col,
            None,
            true,
            gutter,
            None,
        ));
        lines.push(csv_sep_line(&th, &widths, sh.off_col, gutter));
        for j in sh.off_row..(sh.off_row + sh.view_h).min(sh.rows.len()) {
            let sel = if j == sh.row { Some(sh.col) } else { None };
            lines.push(csv_row_line(
                &th,
                &sh.rows[j],
                &widths,
                sh.off_col,
                sel,
                false,
                gutter,
                Some(j + 1),
            ));
        }

        let sheet_suffix = match sh.sheet_tabs() {
            Some((names, active)) => format!(" › {}", names[active]),
            None => String::new(),
        };
        let title = format!(
            " {}{} {} ",
            sh.path.display(),
            sheet_suffix,
            if sh.dirty { "[modified]" } else { "[saved]" }
        );
        let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(para, layout[0]);

        let mut status = format!(
            "Row {}/{} · Col {}/{} · Enter edit · Tab next · a row · A column · Ctrl+S/O save · Ctrl+F find · Esc close",
            sh.row + 1,
            sh.rows.len(),
            sh.col + 1,
            sh.ncols(),
        );
        // xlsx: sheet tabs up front, `[`/`]` to switch.
        if let Some((names, active)) = sh.sheet_tabs() {
            let tabs: Vec<String> = names
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    if i == active {
                        format!("[{n}]")
                    } else {
                        n.clone()
                    }
                })
                .collect();
            status = format!("{} · [ ] sheets · {status}", tabs.join(" "));
        }
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(th.dim)),
            layout[1],
        );
        // The find bar (Ctrl+F) takes over the message line while open.
        if sh.find.is_some() {
            frame.render_widget(
                Paragraph::new(sh.find_bar_text()).style(Style::default().fg(th.accent)),
                layout[2],
            );
        } else {
            frame.render_widget(
                Paragraph::new(sh.message.clone()).style(Style::default().fg(th.accent)),
                layout[2],
            );
        }

        // Put the real terminal cursor on the selected cell — or, while
        // the find bar is open, in its query field.
        if let Some(find) = &sh.find {
            let cx = layout[2].x + "Find: ".len() as u16 + find.cursor.min(10_000) as u16;
            frame.set_cursor_position(Position::new(cx, layout[2].y));
        } else {
            let mut cx = layout[0].x + 3 + (gutter + 1) as u16; // border + gutter + │ + space
            for (i, w) in widths.iter().enumerate() {
                if i < sh.off_col {
                    continue;
                }
                if i >= sh.col {
                    break;
                }
                cx += *w as u16 + 3; // │ + space + content + space
            }
            let cy = layout[0].y + 3 + sh.row.saturating_sub(sh.off_row) as u16; // header + separator
            frame.set_cursor_position(Position::new(cx, cy));
        }
    }

    if discard {
        render_discard_popup(frame, &app.theme(), area);
    }
}

/// Save-as dialog: a directory browser plus a file-name field, rendered
/// over the editor.
fn render_save_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let th = app.theme();
    let Some(dlg) = app.save_dialog.as_ref() else {
        return;
    };
    let popup = centered_rect(62, 70, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Save as ")
        .style(Style::default().fg(th.accent));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Current directory, truncated from the left when too long.
    let cwd_s = dlg.cwd.display().to_string();
    let w = chunks[0].width as usize;
    let cwd_show = if cwd_s.len() > w {
        format!("…{}", &cwd_s[cwd_s.len() - w + 1..])
    } else {
        cwd_s
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            cwd_show,
            Style::default().fg(th.dim),
        ))),
        chunks[0],
    );

    // Directory list, kept scrolled so the selection stays visible.
    let list_h = chunks[1].height as usize;
    let start = if dlg.selected + 1 > list_h.max(1) {
        dlg.selected + 1 - list_h.max(1)
    } else {
        0
    };
    let rows: Vec<Line> = dlg
        .dirs
        .iter()
        .enumerate()
        .skip(start)
        .take(list_h)
        .map(|(i, d)| {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| d.display().to_string());
            let label = format!("{}/", name);
            if i == dlg.selected {
                Line::from(vec![
                    Span::styled(
                        "▸ ",
                        Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        label,
                        Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(format!("  {}", label))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), chunks[1]);

    // File-name field with a block cursor.
    let fc = dlg.fcursor.min(dlg.filename.chars().count());
    let left: String = dlg.filename.chars().take(fc).collect();
    let right: String = dlg.filename.chars().skip(fc).collect();
    let name_line = Line::from(vec![
        Span::styled("Name: ", Style::default().fg(th.dim)),
        Span::raw(left),
        Span::styled("█", Style::default().fg(th.accent)),
        Span::styled(right, Style::default().fg(th.dim)),
    ]);
    frame.render_widget(Paragraph::new(name_line), chunks[2]);

    // Message line (prompts and errors).
    let msg_style = if dlg.message.starts_with("Cannot") || dlg.message.starts_with("Save failed") {
        Style::default().fg(th.danger)
    } else {
        Style::default().fg(th.accent)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(dlg.message.clone(), msg_style))),
        chunks[3],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ select · → open dir · ← up · type name · Enter save · Esc cancel",
            Style::default().fg(th.dim),
        ))),
        chunks[4],
    );
}

/// Move-to-folder dialog: like the save dialog but with no name field —
/// Enter moves the files into the highlighted folder.
fn render_move_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let th = app.theme();
    let Some(dlg) = app.move_dialog.as_ref() else {
        return;
    };
    let popup = centered_rect(62, 70, area);
    frame.render_widget(Clear, popup);
    let title = if dlg.count == 1 {
        String::from(" Move file ")
    } else {
        format!(" Move {} files ", dlg.count)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(Style::default().fg(th.accent));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Current directory, truncated from the left when too long.
    let cwd_s = dlg.cwd.display().to_string();
    let w = chunks[0].width as usize;
    let cwd_show = if cwd_s.len() > w {
        format!("…{}", &cwd_s[cwd_s.len() - w + 1..])
    } else {
        cwd_s
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            cwd_show,
            Style::default().fg(th.dim),
        ))),
        chunks[0],
    );

    // Directory list, kept scrolled so the selection stays visible.
    let list_h = chunks[1].height as usize;
    let start = if dlg.selected + 1 > list_h.max(1) {
        dlg.selected + 1 - list_h.max(1)
    } else {
        0
    };
    let rows: Vec<Line> = dlg
        .dirs
        .iter()
        .enumerate()
        .skip(start)
        .take(list_h)
        .map(|(i, d)| {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| d.display().to_string());
            let label = format!("{}/", name);
            if i == dlg.selected {
                Line::from(vec![
                    Span::styled(
                        "▸ ",
                        Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        label,
                        Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(format!("  {}", label))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), chunks[1]);

    // Message line (prompts and errors).
    let msg_style = if dlg.message.starts_with("Cannot") {
        Style::default().fg(th.danger)
    } else {
        Style::default().fg(th.accent)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(dlg.message.clone(), msg_style))),
        chunks[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ select · → open dir · ← up · Enter move here · Esc cancel",
            Style::default().fg(th.dim),
        ))),
        chunks[3],
    );
}

/// Render markdown source into styled lines: headings bold blue, `code` green,
/// *emphasis* italic, **strong** bold, links underlined with the URL dimmed.
fn render_markdown(theme: &Theme, src: &str) -> Vec<Line<'static>> {
    struct State {
        lines: Vec<Line<'static>>,
        cur: Vec<Span<'static>>,
        styles: Vec<Style>,
        link_url: Option<String>,
        item_prefix: Option<&'static str>,
        quote: usize,
        quote_style: Style,
        accent_style: Style,
    }
    impl State {
        fn style(&self) -> Style {
            *self.styles.last().unwrap()
        }
        fn newline(&mut self) {
            let mut spans = Vec::new();
            for _ in 0..self.quote {
                spans.push(Span::styled("│ ", self.quote_style));
            }
            if let Some(p) = self.item_prefix.take() {
                spans.push(Span::styled(p, self.accent_style));
            }
            spans.extend(self.cur.drain(..));
            self.lines.push(Line::from(spans));
        }
        fn blank(&mut self) {
            self.newline();
            self.lines.push(Line::from(""));
        }
    }

    let mut st = State {
        lines: Vec::new(),
        cur: Vec::new(),
        styles: vec![Style::default()],
        link_url: None,
        item_prefix: None,
        quote: 0,
        quote_style: Style::default().fg(theme.dim),
        accent_style: Style::default().fg(theme.accent),
    };
    let heading = Style::default()
        .fg(theme.heading)
        .add_modifier(Modifier::BOLD);
    let code = Style::default().fg(theme.code);
    let dim = Style::default().fg(theme.dim);

    for ev in Parser::new(src) {
        match ev {
            Event::Start(Tag::Heading { .. }) => {
                st.newline();
                st.styles.push(heading);
            }
            Event::End(TagEnd::Heading(_)) => {
                st.styles.pop();
                st.blank();
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => st.blank(),
            Event::Start(Tag::BlockQuote(_)) => {
                st.newline();
                st.quote += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                st.quote = st.quote.saturating_sub(1);
                st.newline();
            }
            Event::Start(Tag::CodeBlock(_)) => {
                st.newline();
                st.styles.push(code);
            }
            Event::End(TagEnd::CodeBlock) => {
                st.styles.pop();
                st.blank();
            }
            Event::Start(Tag::List(_)) => st.newline(),
            Event::End(TagEnd::List(_)) => st.newline(),
            Event::Start(Tag::Item) => {
                st.newline();
                st.item_prefix = Some("• ");
            }
            Event::End(TagEnd::Item) => {}
            Event::Start(Tag::Emphasis) => {
                st.styles.push(st.style().add_modifier(Modifier::ITALIC));
            }
            Event::Start(Tag::Strong) => {
                st.styles.push(st.style().add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Emphasis) | Event::End(TagEnd::Strong) => {
                st.styles.pop();
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                st.link_url = Some(dest_url.to_string());
                st.styles.push(
                    st.style()
                        .fg(theme.header)
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            Event::End(TagEnd::Link) => {
                st.styles.pop();
                if let Some(u) = st.link_url.take() {
                    st.cur.push(Span::styled(format!(" <{u}>"), dim));
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                st.cur
                    .push(Span::styled(format!("[image: {dest_url}]"), dim));
            }
            Event::End(TagEnd::Image) => {}
            Event::Text(t) => st.cur.push(Span::styled(t.to_string(), st.style())),
            Event::Code(c) => st.cur.push(Span::styled(c.to_string(), code)),
            Event::Html(h) | Event::InlineHtml(h) => {
                st.cur.push(Span::styled(h.to_string(), dim));
            }
            Event::FootnoteReference(n) => {
                st.cur.push(Span::styled(format!("[^{n}]"), dim));
            }
            Event::SoftBreak | Event::HardBreak => st.newline(),
            Event::Rule => {
                st.newline();
                st.cur.push(Span::styled("─".repeat(24), dim));
                st.newline();
            }
            Event::TaskListMarker(done) => {
                st.cur.push(Span::raw(if done { "[x] " } else { "[ ] " }));
            }
            _ => {}
        }
    }
    st.newline();
    while st.lines.last().is_some_and(|l| l.width() == 0) {
        st.lines.pop();
    }
    st.lines
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let th = app.theme();
    let style = if app.status.is_empty() {
        Style::default()
    } else {
        Style::default().fg(th.accent)
    };
    frame.render_widget(Paragraph::new(app.status.as_str()).style(style), area);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    // Archive tabs get their own hint line: the tab is read-only, so the
    // mutating keys are listed as unavailable.
    let hints = if app.archive.is_some() {
        "↑↓ move · Enter open folder · ← back · X extract selection · Esc close tab · archives are read-only · ? help"
    } else {
        "↑↓ move · →/Enter open · ← back · / filter · f find · Ctrl+F find in files · 1/2 list/columns · P preview · e edit · o explorer · n new · r rename · m move · X extract · d trash · D delete · y/x/p copy/cut/paste · Y path · C preview text · s sort · . hidden · * fav · F favorites · G network · ? help · q quit"
    };
    let mut spans = vec![Span::styled(hints, Style::default().fg(theme.dim))];
    if !app.multi.is_empty() {
        spans.push(Span::styled(
            format!(" · {} selected", app.multi.len()),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let footer = Paragraph::new(Line::from(spans));
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
    let th = app.theme();
    let popup = centered_rect(60, 20, area);
    frame.render_widget(Clear, popup);
    let cursor = app.cursor.min(app.input.chars().count());
    // Passwords are masked on screen.
    let shown: String = if kind.is_secret() {
        "•".repeat(app.input.chars().count())
    } else {
        app.input.clone()
    };
    let left: String = shown.chars().take(cursor).collect();
    let right: String = shown.chars().skip(cursor).collect();
    let line = Line::from(vec![
        Span::raw(left),
        Span::styled("█", Style::default().fg(th.accent)),
        Span::styled(right, Style::default().fg(th.dim)),
    ]);
    let input = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", kind.title())),
    );
    frame.render_widget(input, popup);
}

fn render_confirm_popup(frame: &mut Frame, app: &App, area: Rect) {
    let th = app.theme();
    let popup = centered_rect(50, 22, area);
    frame.render_widget(Clear, popup);
    let question = if app.multi.is_empty() {
        let name = app
            .selected_entry()
            .map(|e| e.name.clone())
            .unwrap_or_default();
        if app.confirm_is_trash {
            format!("Move \"{name}\" to the trash?")
        } else {
            format!("Permanently delete \"{name}\"? This cannot be undone.")
        }
    } else if app.confirm_is_trash {
        format!("Move {} selected items to the trash?", app.multi.len())
    } else {
        format!(
            "Permanently delete {} selected items? This cannot be undone.",
            app.multi.len()
        )
    };
    let text = vec![
        Line::from(question),
        Line::from(""),
        Line::from(Span::styled("y: yes    n: no", Style::default().fg(th.dim))),
    ];
    let confirm = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(if app.confirm_is_trash {
                " Confirm trash "
            } else {
                " Confirm delete "
            })
            .style(Style::default().fg(th.danger)),
    );
    frame.render_widget(confirm, popup);
}

fn render_editor(frame: &mut Frame, app: &mut App, area: Rect) {
    let discard = app.mode == Mode::ConfirmDiscard;
    let th = app.theme();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    {
        let Some(ed) = app.editor.as_mut() else {
            return;
        };
        ed.view_h = layout[0].height.saturating_sub(2) as usize; // inside the border
        ed.view_x = layout[0].x;
        ed.view_y = layout[0].y;
        let num_w = ed.lines.len().to_string().len().max(2);
        let gutter_w = if ed.show_line_numbers {
            (num_w + 1) as u16 // digits + one trailing space
        } else {
            0
        };
        // Text width inside the border and gutter: long lines wrap to it.
        ed.wrap_w = (layout[0].width as usize)
            .saturating_sub(2 + gutter_w as usize)
            .max(1);

        let visible = ed.highlight_visible_wrapped(&th, ed.offset, ed.view_h);
        let text: Vec<Line> = visible
            .into_iter()
            .map(|w| {
                // Only a line's first segment gets its number; continuation
                // segments get a blank gutter so wrapped lines read as one.
                let mut v = if ed.show_line_numbers {
                    vec![if w.first {
                        Span::styled(
                            format!("{:>num_w$} ", w.buf_row + 1),
                            Style::default().fg(th.dim),
                        )
                    } else {
                        Span::styled(" ".repeat(num_w + 1), Style::default().fg(th.dim))
                    }]
                } else {
                    Vec::new()
                };
                // Mouse selections render reversed; the gutter is never part
                // of the selection, so it can't be copied by accident. The
                // selection's buffer-column range is intersected with this
                // segment's range.
                let spans = match sel_range_for_row(ed, w.buf_row) {
                    Some((s, e)) => {
                        let (ss, se) = w.seg;
                        let a = s.clamp(ss, se);
                        let b = e.clamp(ss, se);
                        if a < b {
                            apply_selection(w.spans, a - ss, b - ss)
                        } else {
                            w.spans
                        }
                    }
                    None => w.spans,
                };
                // The Ctrl+F match paints in accent + bold while the find
                // bar is open (it eats every key, so the buffer can't change
                // underneath). Intersected with the segment like selections.
                let spans = match ed.find_match {
                    Some((r, s, e)) if r == w.buf_row && ed.find.is_some() => {
                        let (ss, se) = w.seg;
                        let a = s.clamp(ss, se);
                        let b = e.clamp(ss, se);
                        if a < b {
                            apply_match(spans, a - ss, b - ss, &th)
                        } else {
                            spans
                        }
                    }
                    _ => spans,
                };
                v.extend(spans);
                Line::from(v)
            })
            .collect();

        let title = format!(
            " {} {} ",
            if ed.path.as_os_str().is_empty() {
                "untitled".to_string()
            } else {
                ed.path.display().to_string()
            },
            if ed.dirty { "[modified]" } else { "[saved]" }
        );
        let para = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(para, layout[0]);

        let status = format!(
            "Ln {}, Col {} · {} lines · Ctrl+S/O save · Ctrl+Z/Y undo/redo · Shift+arrows select · Ctrl+F find · Ctrl+R replace · Esc close",
            ed.row + 1,
            ed.col + 1,
            ed.lines.len()
        );
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(th.dim)),
            layout[1],
        );
        // The find bar (Ctrl+F) takes over the message line while open.
        if ed.find.is_some() {
            frame.render_widget(
                Paragraph::new(ed.find_bar_text()).style(Style::default().fg(th.accent)),
                layout[2],
            );
        } else {
            frame.render_widget(
                Paragraph::new(ed.message.clone()).style(Style::default().fg(th.accent)),
                layout[2],
            );
        }

        // Put the real terminal cursor on the editor cursor's visual position
        // (a wrapped line's later segments sit on later screen rows) — or,
        // while the find bar is open, on the bar's cursor glyph (the query
        // field, or the active Find/Replace field in replace mode).
        if let Some(find) = &ed.find {
            let cx = if find.replace_mode {
                layout[2].x + find.replace_cursor_offset().min(10_000) as u16
            } else {
                layout[2].x + "Find: ".len() as u16 + find.cursor.min(10_000) as u16
            };
            frame.set_cursor_position(Position::new(cx, layout[2].y));
        } else {
            let (vrow, vcol) = ed.cursor_visual();
            let cx = layout[0].x + 1 + gutter_w + vcol.min(10_000) as u16;
            let cy = layout[0].y + 1 + vrow.saturating_sub(ed.offset) as u16;
            frame.set_cursor_position(Position::new(cx, cy));
        }
    }

    if discard {
        render_discard_popup(frame, &app.theme(), area);
    }
    if app.save_dialog.is_some() {
        render_save_dialog(frame, app, area);
    }
}

/// Selected char range (start, end) on one buffer row, if the row is
/// touched by the mouse selection.
fn sel_range_for_row(ed: &Editor, row: usize) -> Option<(usize, usize)> {
    let ((sr, sc), (er, ec)) = ed.selection()?;
    if row < sr || row > er {
        return None;
    }
    let len = ed.lines[row].chars().count();
    let start = if row == sr { sc.min(len) } else { 0 };
    let end = if row == er { ec.min(len) } else { len };
    if start >= end {
        None
    } else {
        Some((start, end))
    }
}

/// Paint the REVERSED modifier over the spans overlapping [start, end).
fn apply_selection(spans: Vec<Span<'static>>, start: usize, end: usize) -> Vec<Span<'static>> {
    let mut out = Vec::with_capacity(spans.len());
    let mut pos = 0usize;
    for span in spans {
        let text = span.content.clone().into_owned();
        let slen = text.chars().count();
        let (s0, s1) = (pos, pos + slen);
        pos = s1;
        if s1 <= start || s0 >= end {
            out.push(span);
            continue;
        }
        let chars: Vec<char> = text.chars().collect();
        let a = start.max(s0) - s0;
        let b = end.min(s1) - s0;
        let style = span.style;
        if a > 0 {
            out.push(Span::styled(chars[..a].iter().collect::<String>(), style));
        }
        out.push(Span::styled(
            chars[a..b].iter().collect::<String>(),
            style.add_modifier(Modifier::REVERSED),
        ));
        if b < slen {
            out.push(Span::styled(chars[b..].iter().collect::<String>(), style));
        }
    }
    out
}

/// Paint the find match over the spans overlapping [start, end): accent
/// foreground + bold — deliberately distinct from the mouse selection's
/// REVERSED style. Never a solid background (project rule).
fn apply_match(
    spans: Vec<Span<'static>>,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let hl = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let mut out = Vec::with_capacity(spans.len());
    let mut pos = 0usize;
    for span in spans {
        let text = span.content.clone().into_owned();
        let slen = text.chars().count();
        let (s0, s1) = (pos, pos + slen);
        pos = s1;
        if s1 <= start || s0 >= end {
            out.push(span);
            continue;
        }
        let chars: Vec<char> = text.chars().collect();
        let a = start.max(s0) - s0;
        let b = end.min(s1) - s0;
        let style = span.style;
        if a > 0 {
            out.push(Span::styled(chars[..a].iter().collect::<String>(), style));
        }
        out.push(Span::styled(chars[a..b].iter().collect::<String>(), hl));
        if b < slen {
            out.push(Span::styled(chars[b..].iter().collect::<String>(), style));
        }
    }
    out
}

fn render_discard_popup(frame: &mut Frame, theme: &Theme, area: Rect) {
    let popup = centered_rect(46, 24, area);
    frame.render_widget(Clear, popup);
    let text = vec![
        Line::from(""),
        Line::from("Discard unsaved changes?"),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " y ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" discard    "),
            Span::styled(
                " n / Esc ",
                Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" keep editing"),
        ]),
    ];
    let p = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Unsaved changes "),
        );
    frame.render_widget(p, popup);
}

/// Modal favorites list: Enter jumps the current tab there, * removes.
fn render_favorites_popup(frame: &mut Frame, ws: &Workspace, area: Rect) {
    let th = ws.theme();
    let popup = centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);
    let items: Vec<ListItem> = if ws.favorites.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::styled(
                "★ ",
                Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "No favorites yet — press * on a file or folder to add one.",
                Style::default().fg(th.dim),
            ),
        ]))]
    } else {
        ws.favorites
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let style = if i == ws.fav_sel {
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
                } else if p.exists() {
                    Style::default()
                } else {
                    Style::default().fg(th.dim)
                };
                let mut label = format!("★ {}", p.to_string_lossy());
                if p.is_dir() {
                    label.push('/');
                }
                if !p.exists() {
                    label.push_str("  (missing)");
                }
                ListItem::new(Line::from(Span::styled(label, style)))
            })
            .collect()
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Favorites ")
            .title_bottom(" Enter jump · * remove · Esc close "),
    );
    frame.render_widget(list, popup);
}

fn render_help_popup(frame: &mut Frame, ws: &Workspace, area: Rect) {
    let th = ws.theme();
    let popup = centered_rect(62, 80, area);
    frame.render_widget(Clear, popup);
    let rows = [
        ("↑ / ↓", "move selection"),
        ("→ / Enter", "enter directory · open file in default app"),
        (
            "Enter on an archive",
            "browse inside it in a new read-only tab (Enter opens folders, X extracts the selection, ← goes back, Esc closes the tab)",
        ),
        ("← / Backspace", "parent directory"),
        ("PgUp / PgDn", "jump 10 entries"),
        ("Home / End", "first / last entry"),
        (
            "Shift+↑ / ↓",
            "multi-select files (y / x / m / d act on all, Esc clears)",
        ),
        ("/", "filter list (live, Esc clears)"),
        (
            "f",
            "search file names (Tab: deep content search, Enter: jump)",
        ),
        (
            "Ctrl+F",
            "find in files: contents search on a background thread (Enter: search again / jump to the hit in the editor, Esc: exit)",
        ),
        ("1 / 2", "list view / Miller-column view"),
        ("P", "toggle the preview pane (off by default)"),
        ("s / S", "cycle sort key · toggle direction"),
        (".", "show / hide hidden files"),
        (
            "git badges",
            "inside a git repository, each entry's name shows a dim status letter — M modified · A staged/added · D deleted · ? untracked · R renamed; folders show the highest-priority badge among the files under them",
        ),
        ("n / N", "new file / new directory"),
        ("r", "rename"),
        ("m", "move the selected file(s) to another folder (popup)"),
        (
            "X",
            "extract the selected archive(s) into a new folder named after each archive (inside an archive tab: extract the selected file or folder next to the archive)",
        ),
        ("d", "move the selection to the trash (asks first)"),
        ("D", "permanently delete the selection (asks first)"),
        (
            "Ctrl+C / X / V",
            "copy / cut / paste — act on the multi-selection too (y / x / p work too)",
        ),
        ("Y", "copy selected path to the system clipboard"),
        ("C", "copy preview pane text to the system clipboard"),
        (
            "G",
            "drives & network: open a drive, mapped network drives (Windows), find SMB devices, mount a share as a new tab",
        ),
        ("*", "favorite / unfavorite the selected file or folder"),
        (
            "F",
            "favorites popup (Enter: jump there, *: remove, Esc: close)",
        ),
        (
            "e",
            "edit file (text editor) · CSV and xlsx files open the table viewer",
        ),
        (
            "Ctrl+R",
            "find & replace in the editor: Tab switches the Find/Replace fields · Enter replaces the current match · Ctrl+A replaces every match (one undo step) · Esc closes",
        ),
        ("`", "open a terminal tab (a real shell in this folder)"),
        ("Ctrl+T", "new file-browser tab"),
        ("Ctrl+N", "new blank text document (tab)"),
        ("Ctrl+W", "close current tab"),
        ("Ctrl+PgUp / PgDn", "previous / next tab"),
        ("Alt+1 … Alt+9", "jump to tab"),
        (
            "Ctrl+G then 1-9",
            "jump to tab (macOS-friendly: no Alt key needed)",
        ),
        ("Ctrl+G then n / p", "next / previous tab"),
        ("click tab", "switch tabs with the mouse"),
        ("o", "open the selected folder (or a file's parent folder) in the OS file explorer"),
        ("?", "this help"),
        ("q / Esc", "quit"),
        ("Ctrl+C", "quit (from dialogs)"),
        (
            "mouse",
            "click selects a file/folder · double-click opens it · click+drag or Shift+arrows selects text in the editor",
        ),
        ("wheel", "scroll list / editor / table"),
    ];
    let mut lines: Vec<Line> = rows
        .iter()
        .map(|(keys, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("{keys:<16}"),
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                ),
                Span::raw(*desc),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Settings (press t / l while help is open)",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    for (keys, desc) in [
        ("t", format!("cycle color theme (now: {})", th.name)),
        (
            "l",
            format!(
                "editor line numbers: {}",
                if ws.show_line_numbers { "on" } else { "off" }
            ),
        ),
    ] {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{keys:<16}"),
                Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(desc),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter on a file opens it with the system default app. Terminal tabs run your $SHELL (PowerShell on Windows) — almost every key goes straight to it (Ctrl+C is SIGINT there); Ctrl+T / Ctrl+N / Ctrl+W / Ctrl+PgUp/PgDn / Alt+1-9 / Ctrl+G still manage tabs. Ctrl+N opens a new blank text document in its own tab; Ctrl+S on it opens a save dialog where you browse to a folder, type a name, and Enter saves (existing files ask to overwrite). In the editor: arrows/Home/End/PgUp/PgDn move · Shift+arrows (or Shift+Home/End) selects text, plain arrows collapse the selection · Ctrl+Left/Right jump by word (Ctrl+Shift+Left/Right selects by word) · type to edit · paste is instant and undoes as one step (Ctrl+Z removes the whole paste, even when the terminal delivers it character by character) · Ctrl+S/O save · Ctrl+F find text in the buffer (live search: the current match is highlighted in the accent color, Enter next match, Shift+Enter previous, Esc close) · Ctrl+R find & replace (Tab switches the Find/Replace fields, Enter replaces the current match, Ctrl+A replaces every match in one undo step, Esc closes) · Ctrl+Z/Y undo/redo (fast typing undoes as one burst) · Ctrl+A select all (Ctrl+E works too, for terminals that grab Ctrl+A) · Ctrl+C/X/V copy/cut/paste (selection, never line numbers; copies also land on the system clipboard, so Cmd+V works anywhere — the terminal itself intercepts Cmd+C, it never reaches fex) · Esc clears selection, then closes (asks if unsaved). In the CSV viewer: rows are numbered · arrows move · Enter edits a cell · Tab next cell · a adds a row · A adds a column · Ctrl+F find a cell (live search: Enter next match, Shift+Enter previous, Esc close) · Ctrl+S (or Ctrl+O) saves · Esc closes. The Excel viewer (e on an .xlsx file) works the same, plus: [ and ] switch sheets · headers are column letters (A, B, C…) · dates show as dates · formula cells edit as =formula (keep the = to edit the formula, delete it to replace with a plain value; fex never recalculates — Excel refreshes formulas when you open the file there) ·  Tabs are saved between launches: quitting brings back your browser tabs, editors (including unsaved changes), and terminal tabs (a fresh shell in the same folder). Closing a tab with Ctrl+W discards its saved state for good.Esc closes.",
        Style::default().fg(Color::DarkGray),
    )));
    let help = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Help (?/Esc to close) "),
    );
    frame.render_widget(help, popup);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::THEMES;

    fn line_text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn markdown_renders_structure() {
        let lines = render_markdown(
            &THEMES[0],
            "# Title\n\nHello *world* and `code`.\n\n- a\n- b\n",
        );
        let text: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Title"), "got:\n{text}");
        assert!(text.contains("code"), "got:\n{text}");
        assert!(text.contains("• a"), "got:\n{text}");
        // The heading text got the bold-blue heading style.
        let span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content == "Title")
            .unwrap();
        assert_eq!(span.style.fg, Some(THEMES[0].heading));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn csv_grid_lines_render() {
        let cells = vec![String::from("a"), String::from("bb")];
        let line = csv_row_line(&THEMES[0], &cells, &[3, 3], 0, Some(1), false, 2, Some(1));
        let text: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(text, " 1 │ a   │ bb  │");
        // selected cell is reversed (spans: gutter, │, cell, │, cell, │)
        assert!(line.spans[4]
            .style
            .add_modifier
            .contains(Modifier::REVERSED));
        let sep = csv_sep_line(&THEMES[0], &[3, 3], 0, 2);
        let sep_text: String = sep.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(sep_text, "   ├─────┼─────┤");
        // header row is bold, with a blank gutter corner
        let head = csv_row_line(&THEMES[0], &cells, &[3, 3], 0, None, true, 2, None);
        let head_text: String = head.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(head_text, "   │ a   │ bb  │");
        assert!(head.spans[2].style.add_modifier.contains(Modifier::BOLD));
    }
}
