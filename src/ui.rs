//! Rendering: layout, file list, preview pane, status bar, and popups.

use crate::app::{
    App, InputKind, Mode, NetRow, NetState, Tab, ViewMode, Workspace, HELP_TABS,
    HELP_TAB_ACCESSIBILITY,
};
use crate::doc::{DocLine, DocSpan, EffFmt};
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

    // Word (.docx) viewer/editor.
    if matches!(app.mode, Mode::Doc) {
        render_doc(frame, app, content);
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
        Mode::ConfirmQuit => render_quit_popup(frame, ws, content),
        Mode::Help => render_help_popup(frame, ws, content),
        Mode::Normal
        | Mode::Editor
        | Mode::ConfirmDiscard
        | Mode::Search
        | Mode::Sheet
        | Mode::Doc
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
    let (exited, lines, scrolled) = match ws.tabs.get_mut(ws.active) {
        Some(Tab::Shell(sh)) => {
            sh.resize(area.width, area.height);
            (
                sh.state.exited,
                sh.state.render_lines(),
                sh.state.scrollback_pos(),
            )
        }
        _ => return,
    };
    let mut lines: Vec<Line> = lines.into_iter().take(area.height as usize).collect();
    // The notice takes the last row so it is always visible.
    if scrolled > 0 {
        lines.pop();
        lines.push(Line::from(Span::styled(
            " history — Shift+PgDn / End for live, Shift+PgUp further back",
            Style::default().fg(ws.theme().dim),
        )));
    } else if exited {
        lines.pop();
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
    hl: &CellHighlights,
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
    // 0-based data row for highlight lookups (row_num is 1-based).
    let data_row = row_num.map(|n| n.saturating_sub(1));
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
        // Selection block and find matches paint in accent (no solid
        // background); find matches add bold. The cursor cell keeps its
        // reversed style on top.
        if let (Some(dr), Some(((r0, c0), (r1, c1)))) = (data_row, hl.sel_rect) {
            if dr >= r0 && dr <= r1 && i >= c0 && i <= c1 {
                style = style.fg(theme.accent);
            }
        }
        if let Some(dr) = data_row {
            if hl.find_cells.contains(&(dr, i)) {
                style = style.fg(theme.accent).add_modifier(Modifier::BOLD);
            }
        }
        if selected_col == Some(i) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        spans.push(Span::styled(format!(" {shown:<width$} ", width = w), style));
        spans.push(Span::styled("│", grid));
    }
    Line::from(spans)
}

/// Per-cell highlight info for one grid row: Ctrl+F match cells and the
/// Shift+arrow selection block, as 0-based (row, col) data coordinates.
struct CellHighlights {
    find_cells: std::collections::HashSet<(usize, usize)>,
    sel_rect: Option<((usize, usize), (usize, usize))>,
}

impl CellHighlights {
    fn none() -> Self {
        Self {
            find_cells: std::collections::HashSet::new(),
            sel_rect: None,
        }
    }
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
        let hl_none = CellHighlights::none();
        lines.push(csv_row_line(
            &th,
            &sh.headers,
            &widths,
            sh.off_col,
            None,
            true,
            gutter,
            None,
            &hl_none,
        ));
        lines.push(csv_sep_line(&th, &widths, sh.off_col, gutter));
        // Ctrl+F match cells and the Shift+arrow selection block, painted
        // in accent (never a solid background).
        let hl = CellHighlights {
            find_cells: if sh.find.is_some() {
                sh.find_matches().into_iter().collect()
            } else {
                std::collections::HashSet::new()
            },
            sel_rect: sh.selection_rect(),
        };
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
                &hl,
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
            "Row {}/{} · Col {}/{} · Enter edit · Tab next · a row · A column · Shift+arrows select · Ctrl+C copy · Ctrl+V paste · Ctrl+S/O save · Ctrl+F find · Esc close",
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

/// The Word (.docx) viewer/editor: wrapped, styled paragraphs and
/// bordered tables, with the real cursor placed by the doc layout.
fn render_doc(frame: &mut Frame, app: &mut App, area: Rect) {
    let discard = app.doc.as_ref().is_some_and(|d| d.confirm_discard);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let th = app.theme();
    let Some(d) = app.doc.as_mut() else {
        return;
    };
    let inner_w = layout[0].width.saturating_sub(2).max(8) as usize;
    d.view_h = layout[0].height.saturating_sub(2).max(1) as usize;
    let total_lines = {
        let lines = d.layout(inner_w);
        lines.len()
    };
    d.ensure_visible();
    let top = d.top;
    let view_h = d.view_h;
    let selection = d.selection();
    let cursor = d.cursor;
    let lines: Vec<DocLine> = d.layout(inner_w).to_vec();
    let mut out: Vec<Line> = Vec::with_capacity(view_h);
    let mut cursor_screen: Option<(usize, usize)> = None;
    if let Some((cli, ccol)) = d.cursor_position() {
        if cli >= top && cli < top + view_h {
            cursor_screen = Some((cli, ccol));
        }
    }
    for (vi, line) in lines.iter().skip(top).take(view_h).enumerate() {
        let li = top + vi;
        match line {
            DocLine::Rule(text) => {
                out.push(Line::from(Span::styled(
                    text.clone(),
                    Style::default().fg(th.dim),
                )));
            }
            DocLine::Para {
                block,
                cell,
                para,
                spans,
                prefix,
                align,
                heading,
                ..
            } => {
                let content_w = prefix.chars().count()
                    + spans.iter().map(|s| s.text.chars().count()).sum::<usize>();
                let pad = match align.as_str() {
                    "center" => inner_w.saturating_sub(content_w) / 2,
                    "right" => inner_w.saturating_sub(content_w),
                    _ => 0,
                };
                let mut spans_out: Vec<Span> = Vec::new();
                if pad > 0 {
                    spans_out.push(Span::raw(" ".repeat(pad)));
                }
                let marker_w = prefix.trim_end().len();
                if marker_w > 0 {
                    spans_out.push(Span::styled(
                        prefix.chars().take(marker_w).collect::<String>(),
                        Style::default().fg(th.accent),
                    ));
                    spans_out.push(Span::raw(prefix.chars().skip(marker_w).collect::<String>()));
                } else {
                    spans_out.push(Span::raw(prefix.clone()));
                }
                for s in spans {
                    spans_out.push(doc_span(
                        &th,
                        s,
                        *heading,
                        selection,
                        (*block, *cell, *para),
                    ));
                }
                out.push(Line::from(spans_out));
                if let Some((cli, ccol)) = cursor_screen {
                    if cli == li {
                        let x = layout[0].x + 1 + pad as u16 + ccol as u16;
                        let y = layout[0].y + 1 + vi as u16;
                        d.cursor_xy = Some((x, y));
                    }
                }
            }
            DocLine::TableSlice { block, row, cells } => {
                let Some(t) = (match d.blocks.get(*block) {
                    Some(crate::doc::Block::Table(t)) => Some(t),
                    _ => None,
                }) else {
                    out.push(Line::from(""));
                    continue;
                };
                let widths = crate::doc::table_col_widths(t, inner_w);
                let mut spans_out: Vec<Span> = vec![Span::styled("│", Style::default().fg(th.dim))];
                for (ci, entry) in cells.iter().enumerate() {
                    let w = widths.get(ci).copied().unwrap_or(3);
                    let mut used = 0usize;
                    if let Some((pi, _, _, spans)) = entry {
                        for s in spans {
                            used += s.text.chars().count();
                            spans_out.push(doc_span(
                                &th,
                                s,
                                None,
                                selection,
                                (*block, Some((*row, ci)), *pi),
                            ));
                        }
                    }
                    if used < w {
                        spans_out.push(Span::raw(" ".repeat(w - used)));
                    }
                    spans_out.push(Span::styled("│", Style::default().fg(th.dim)));
                }
                out.push(Line::from(spans_out));
                if let Some((cli, ccol)) = cursor_screen {
                    if cli == li {
                        // ccol is the offset within the cursor's cell
                        // segment; add the widths of preceding cells.
                        let cc = cursor.cell.map(|(_, c)| c).unwrap_or(0);
                        let mut x = layout[0].x + 2; // border + first │
                        for (i, w) in widths.iter().enumerate() {
                            if i < cc {
                                x += *w as u16 + 1;
                            }
                        }
                        let y = layout[0].y + 1 + vi as u16;
                        d.cursor_xy = Some((x + ccol as u16, y));
                    }
                }
            }
        }
    }
    let title = format!(
        " {} {} ",
        d.path.display(),
        if d.dirty { "[modified]" } else { "[saved]" }
    );
    let para = Paragraph::new(out).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(para, layout[0]);

    // Status: style + effective formatting at the cursor, then hints.
    let status = {
        let cur_para = d.para_at(cursor);
        let style = cur_para.map(|p| d.style_name(p)).unwrap_or_default();
        let mut fmt_bits: Vec<String> = Vec::new();
        if let Some(p) = cur_para {
            // The run under the cursor (or the first run).
            let mut base = 0usize;
            let mut fmt: Option<EffFmt> = None;
            for item in &p.items {
                let runs: &[crate::doc::Run] = match item {
                    crate::doc::Item::Run(r) => std::slice::from_ref(r),
                    crate::doc::Item::Link { runs, .. } => runs,
                    _ => continue,
                };
                for r in runs {
                    let len = r.text.chars().count();
                    if cursor.offset >= base && cursor.offset <= base + len {
                        fmt = Some(d.eff_fmt(p, r));
                    }
                    base += len;
                }
            }
            if let Some(f) = fmt {
                if f.bold {
                    fmt_bits.push(String::from("Bold"));
                }
                if f.italic {
                    fmt_bits.push(String::from("Italic"));
                }
                if f.underline {
                    fmt_bits.push(String::from("Underline"));
                }
                if let Some(sz) = f.size_pt {
                    fmt_bits.push(format!("{sz}pt"));
                }
                if let Some(font) = &f.font {
                    fmt_bits.push(font.clone());
                }
            }
        }
        let line_no = d.cursor_position().map(|(li, _)| li + 1).unwrap_or(1);
        format!(
            "{style}{}{} · Line {line_no}/{total_lines} · Ctrl+B/I/U format · Ctrl+L/E/R/J align · Tab next cell · Ctrl+S save · Esc close",
            if fmt_bits.is_empty() {
                String::new()
            } else {
                format!(" · {}", fmt_bits.join(" · "))
            },
            if cur_para.is_some_and(|p| p.locked) {
                " · locked (tracked changes/fields)"
            } else {
                ""
            },
        )
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(th.dim)),
        layout[1],
    );
    frame.render_widget(
        Paragraph::new(d.message.clone()).style(Style::default().fg(th.accent)),
        layout[2],
    );
    if let Some((x, y)) = d.cursor_xy.take() {
        frame.set_cursor_position(Position::new(x, y));
    }
    if discard {
        render_discard_popup(frame, &th, area);
    }
}

/// One styled span of document text.
fn doc_span(
    th: &Theme,
    s: &DocSpan,
    heading: Option<u8>,
    selection: Option<(crate::doc::Loc, usize, usize)>,
    loc_key: (usize, Option<(usize, usize)>, usize),
) -> Span<'static> {
    if s.placeholder {
        return Span::styled(
            s.text.clone(),
            Style::default().fg(th.dim).add_modifier(Modifier::ITALIC),
        );
    }
    let f = &s.fmt;
    let mut style = Style::default();
    let mut mods = Modifier::empty();
    if f.bold || heading.is_some() {
        mods |= Modifier::BOLD;
    }
    if f.italic {
        mods |= Modifier::ITALIC;
    }
    if f.underline || s.link {
        mods |= Modifier::UNDERLINED;
    }
    if f.strike {
        mods |= Modifier::CROSSED_OUT;
    }
    if let Some((r, g, b)) = f.color {
        style = style.fg(Color::Rgb(r, g, b));
    } else if s.link {
        style = style.fg(th.accent);
    } else if heading.is_some() {
        style = style.fg(th.heading);
    }
    if let Some((r, g, b)) = f.highlight {
        style = style.bg(Color::Rgb(r, g, b));
    }
    if f.caps {
        // Rendered as-is (terminals can't restyle case cheaply per span
        // without changing text); caps text is usually typed in caps.
    }
    if let Some((sloc, lo, hi)) = selection {
        if (sloc.block, sloc.cell, sloc.para) == loc_key && s.end > lo && s.start < hi {
            mods |= Modifier::UNDERLINED | Modifier::BOLD;
        }
    }
    Span::styled(s.text.clone(), style.add_modifier(mods))
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
/// Split `name` into (before, match, after) around the first
/// case-insensitive occurrence of `needle`. Returns `None` when there is
/// no match, or when case folding would make the byte ranges unreliable
/// (so callers simply skip highlighting instead of slicing wrong).
fn split_match<'a>(name: &'a str, needle: &str) -> Option<(&'a str, &'a str, &'a str)> {
    if needle.is_empty() {
        return None;
    }
    let lneedle = needle.to_lowercase();
    let start = name.to_lowercase().find(&lneedle)?;
    let end = start + lneedle.len();
    if end > name.len() || !name.is_char_boundary(start) || !name.is_char_boundary(end) {
        return None;
    }
    if name[start..end].to_lowercase() != lneedle {
        return None;
    }
    Some((&name[..start], &name[start..end], &name[end..]))
}

/// Shorten `s` to fit `w` columns, cutting from the left with an ellipsis.
/// Char-boundary safe.
fn truncate_left(s: &str, w: usize) -> String {
    if s.len() <= w {
        return s.to_string();
    }
    let mut start = s.len().saturating_sub(w.saturating_sub(1));
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &s[start..])
}

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

    // Directory list (live-filtered), kept scrolled so the selection stays
    // visible.
    let list_h = chunks[1].height as usize;
    let start = if dlg.selected + 1 > list_h.max(1) {
        dlg.selected + 1 - list_h.max(1)
    } else {
        0
    };
    let vis = dlg.visible();
    let accent_bold = Style::default().fg(th.accent).add_modifier(Modifier::BOLD);
    let rows: Vec<Line> = vis
        .iter()
        .enumerate()
        .skip(start)
        .take(list_h)
        .map(|(i, &di)| {
            let d = &dlg.dirs[di];
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| d.display().to_string());
            let label = format!("{}/", name);
            let selected = i == dlg.selected;
            let prefix = if selected { "▸ " } else { "  " };
            let prefix_style = if selected {
                accent_bold
            } else {
                Style::default()
            };
            let base_style = if selected {
                accent_bold
            } else {
                Style::default()
            };
            // While a filter is active, highlight the matching part of
            // each folder name so it's clear why the row is shown.
            let mut spans = vec![Span::styled(prefix, prefix_style)];
            if !dlg.filter.is_empty() {
                if let Some((before, hit, after)) = split_match(&label, &dlg.filter) {
                    let hit_style = if selected {
                        accent_bold.add_modifier(Modifier::UNDERLINED)
                    } else {
                        accent_bold
                    };
                    spans.push(Span::styled(before.to_string(), base_style));
                    spans.push(Span::styled(hit.to_string(), hit_style));
                    spans.push(Span::styled(after.to_string(), base_style));
                    return Line::from(spans);
                }
            }
            spans.push(Span::styled(label, base_style));
            Line::from(spans)
        })
        .collect();
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (no matches)",
                Style::default().fg(th.dim),
            ))),
            chunks[1],
        );
    } else {
        frame.render_widget(Paragraph::new(rows), chunks[1]);
    }

    // Destination line: the full path of the highlighted folder, so it's
    // unmistakable where Enter will move the file(s).
    let dest_line = match vis.get(dlg.selected).and_then(|&di| dlg.dirs.get(di)) {
        Some(d) => {
            let mut path = d.display().to_string();
            if !path.ends_with('/') {
                path.push('/');
            }
            let show = truncate_left(&path, chunks[2].width as usize - "→ Move to: ".len());
            Line::from(vec![
                Span::styled("→ Move to: ", accent_bold),
                Span::styled(show, accent_bold),
            ])
        }
        None => Line::from(Span::styled(
            "→ Move to: (no folder selected)",
            Style::default().fg(th.dim),
        )),
    };
    frame.render_widget(Paragraph::new(dest_line), chunks[2]);

    // Filter + sort status line.
    let mut status = vec![Span::styled(
        format!("Sort: {}", dlg.sort.label()),
        Style::default().fg(th.dim),
    )];
    if dlg.filtering || !dlg.filter.is_empty() {
        let caret = if dlg.filtering { "▏" } else { "" };
        status.push(Span::styled("   ", Style::default()));
        status.push(Span::styled(
            format!("Filter: {}{}", dlg.filter, caret),
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(status)), chunks[3]);

    // Message line (prompts and errors).
    let msg_style = if dlg.message.starts_with("Cannot") {
        Style::default().fg(th.danger)
    } else {
        Style::default().fg(th.accent)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(dlg.message.clone(), msg_style))),
        chunks[4],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ select · → open dir · ← up · Enter move here · f or / filter · s/S sort · Esc cancel",
            Style::default().fg(th.dim),
        ))),
        chunks[5],
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
        "↑↓ move · →/Enter open · ← back · / filter · f find · Ctrl+F find in files · 1/2 list/columns · P preview · e edit · o/O explorer · n new · r rename · m move · X extract · d trash · D delete · y/x/p copy/cut/paste · Y path · C preview text · s sort · . hidden · * fav · F favorites · G network · ? help · q quit"
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

/// "Quit fex?" confirmation: Esc with nothing left to dismiss lands here
/// instead of exiting, so a run of Esc presses can't quit by accident.
fn render_quit_popup(frame: &mut Frame, ws: &Workspace, area: Rect) {
    let th = ws.theme();
    let popup = centered_rect(50, 22, area);
    frame.render_widget(Clear, popup);
    let n = ws.tabs.len();
    let tabs = if n == 1 {
        String::from("1 tab open")
    } else {
        format!("{n} tabs open")
    };
    let text = vec![
        Line::from(format!("Quit fex? ({tabs})")),
        Line::from(""),
        Line::from(Span::styled(
            "y / Enter / q: quit    n / Esc: keep going",
            Style::default().fg(th.dim),
        )),
    ];
    let confirm = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Quit ")
            .style(Style::default().fg(th.accent)),
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
        // Ctrl+F match ranges per buffer row: (start, end, is_current).
        // Every match paints in accent while the find bar is open; the
        // current match adds bold on top.
        let mut find_hl: std::collections::HashMap<usize, Vec<(usize, usize, bool)>> =
            std::collections::HashMap::new();
        if ed.find.is_some() {
            let qlen = ed
                .find
                .as_ref()
                .map(|f| f.query.chars().count())
                .unwrap_or(0);
            let current = ed.find_match;
            for (r, c) in ed.find_matches() {
                let is_current = matches!(current, Some((cr, cs, _)) if cr == r && cs == c);
                find_hl
                    .entry(r)
                    .or_default()
                    .push((c, c + qlen, is_current));
            }
            for ranges in find_hl.values_mut() {
                ranges.sort_by_key(|&(_, _, cur)| cur);
            }
        }
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
                // The Ctrl+F matches paint in accent while the find bar is
                // open (it eats every key, so the buffer can't change
                // underneath); the current match adds bold. Intersected
                // with the segment like selections.
                let spans = match find_hl.get(&w.buf_row) {
                    Some(ranges) => {
                        let (ss, se) = w.seg;
                        let mut spans = spans;
                        for &(s, e, current) in ranges {
                            let a = s.clamp(ss, se);
                            let b = e.clamp(ss, se);
                            if a < b {
                                spans = apply_match(spans, a - ss, b - ss, &th, current);
                            }
                        }
                        spans
                    }
                    None => spans,
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
            "Ln {}, Col {} · {} lines · Ctrl+S/O save · Ctrl+Z/Y undo/redo · Shift+arrows select · Ctrl+D dup line · Alt+↑/↓ move line · Tab indent · Ctrl+F find · Ctrl+R replace · Esc close",
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
    bold: bool,
) -> Vec<Span<'static>> {
    let mut hl = Style::default().fg(theme.accent);
    if bold {
        hl = hl.add_modifier(Modifier::BOLD);
    }
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

fn render_help_popup(frame: &mut Frame, ws: &mut Workspace, area: Rect) {
    let th = ws.theme();
    let popup = centered_rect(72, 86, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (?/Esc closes · 1-5 or click switches tabs) ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let tab = ws
        .active_browser()
        .map(|a| a.help_tab)
        .unwrap_or(0)
        .min(HELP_TABS.len() - 1);
    let key_delay_ms = ws.key_delay_ms;
    let alt_nav = ws.alt_nav;
    let show_line_numbers = ws.show_line_numbers;

    // Tab bar: number keys, <-/->, or a mouse click switches tabs.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);
    let mut spans = vec![Span::raw(" ")];
    let mut hits = Vec::new();
    let mut x = chunks[0].x + 1;
    for (i, name) in HELP_TABS.iter().enumerate() {
        let label = format!(" {}:{} ", i + 1, name);
        let w = label.chars().count() as u16;
        let style = if i == tab {
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th.dim)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
        hits.push((x, x + w, chunks[0].y, i));
        x += w + 1;
    }
    ws.help_tab_hits = hits;
    frame.render_widget(Paragraph::new(Line::from(spans)), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(chunks[1].width as usize),
            Style::default().fg(th.dim),
        ))),
        chunks[1],
    );

    let lines: Vec<Line> = match tab {
        1 => help_editor_lines(),
        2 => help_sheet_lines(),
        3 => help_terminal_lines(),
        HELP_TAB_ACCESSIBILITY => help_accessibility_lines(&th, key_delay_ms, alt_nav),
        _ => help_browser_lines(&th, show_line_numbers),
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), chunks[2]);
}

/// Turn key/description pairs into help lines (accented key column).
fn help_key_lines(th: &Theme, rows: &[(&str, String)]) -> Vec<Line<'static>> {
    rows.iter()
        .map(|(keys, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("{keys:<16}"),
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                ),
                Span::raw(desc.clone()),
            ])
        })
        .collect()
}

/// Tab 1: the file-list key reference plus the t/l settings.
fn help_browser_lines(th: &Theme, show_line_numbers: bool) -> Vec<Line<'static>> {
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
        ("n / N", "new file / new directory (.xlsx/.docx/.pptx are created as valid empty documents)"),
        ("r", "rename"),
        ("m", "move the selected file(s) to another folder (popup)"),
        (
            "X",
            "extract the selected archive(s) into a new folder named after each archive (inside an archive tab: extract the selected file or folder next to the archive)",
        ),
        (
            "z",
            "zip the selection into a new archive (acts on the multi-selection too; asks for the archive name)",
        ),
        ("d", "move the selection to the trash (asks first)"),
        ("D", "permanently delete the selection (asks first)"),
        (
            "Ctrl+C / X / V",
            "copy / cut / paste — act on the multi-selection too (y / x / p work too)",
        ),
        ("Y", "copy selected path to the system clipboard (a path on a mapped network drive copies as its original UNC path, not the drive letter)"),
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
            "edit file (text editor) · CSV and xlsx files open the table viewer · docx files open the Word editor",
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
        (
            "o / O",
            "reveal in the OS file explorer: the item's exact folder opens and a file is highlighted in it; a folder is opened itself",
        ),
        ("?", "this help"),
        ("q", "quit"),
        (
            "Esc",
            "clear selection / filter, or ask before quitting (never quits by accident)",
        ),
        ("Ctrl+C", "cancel (never quits the app)"),
        (
            "mouse",
            "click selects a file/folder · double-click opens it · click+drag or Shift+arrows selects text in the editor",
        ),
        ("wheel", "scroll list / editor / table"),
    ];
    let mut lines = help_key_lines(
        th,
        &rows
            .iter()
            .map(|(k, d)| (*k, (*d).to_string()))
            .collect::<Vec<_>>(),
    );
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Settings (press t / l while help is open; accessibility lives on tab 5)",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.extend(help_key_lines(
        th,
        &[
            ("t", format!("cycle color theme (now: {})", th.name)),
            (
                "l",
                format!(
                    "editor line numbers: {}",
                    if show_line_numbers { "on" } else { "off" }
                ),
            ),
        ],
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter on a file opens it with the system default app.",
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

/// Tab 2: the built-in text editor.
fn help_editor_lines() -> Vec<Line<'static>> {
    vec![Line::from(Span::styled(
        "In the editor: arrows/Home/End/PgUp/PgDn move · Shift+arrows (or Shift+Home/End) selects text, plain arrows collapse the selection · Ctrl+Left/Right jump by word (Ctrl+Shift+Left/Right selects by word) · type to edit · paste is instant and undoes as one step (Ctrl+Z removes the whole paste, even when the terminal delivers it character by character) · Ctrl+S/O save · Ctrl+F find text in the buffer (live search: every match is highlighted in the accent color, the current match bold, Enter next match, Shift+Enter previous, Esc close) · Ctrl+R find & replace (Tab switches the Find/Replace fields, Enter replaces the current match, Ctrl+A replaces every match in one undo step, Esc closes) · Ctrl+Z/Y undo/redo (fast typing undoes as one burst) · Ctrl+A select all (Ctrl+E works too, for terminals that grab Ctrl+A) · Ctrl+C/X/V copy/cut/paste (selection, never line numbers; copies also land on the system clipboard, so Cmd+V works anywhere — the terminal itself intercepts Cmd+C, it never reaches fex) · Ctrl+D duplicate line · Alt+↑/↓ move line up/down · Tab indents the selection (Shift+Tab dedents) · Ctrl+Backspace / Ctrl+Delete delete a word · Esc clears selection, then closes (asks if unsaved).",
        Style::default().fg(Color::DarkGray),
    ))]
}

/// Tab 3: the CSV and Excel table viewers.
fn help_sheet_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "In the CSV viewer: rows are numbered · arrows move · Enter edits a cell · Tab next cell · a adds a row · A adds a column · Shift+arrows select a block · Ctrl+C copies the block (or cell) as tab-separated text for Excel/Sheets · Ctrl+V pastes tab-separated clipboard text at the cursor · Ctrl+F find a cell (live search: every match is highlighted, Enter next match, Shift+Enter previous, Esc close) · Ctrl+S (or Ctrl+O) saves · Esc closes.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The Excel viewer (e on an .xlsx file) works the same, plus: [ and ] switch sheets · headers are column letters (A, B, C…) · dates show as dates · formula cells edit as =formula (keep the = to edit the formula, delete it to replace with a plain value; fex never recalculates — Excel refreshes formulas when you open the file there)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The Word viewer (e on a .docx file) edits the document in place: type to edit text (each run keeps its own formatting), arrows/Home/End move, Enter splits a paragraph (a heading continues as Normal), Backspace/Delete merge, Tab jumps between table cells. Ctrl+B / Ctrl+I / Ctrl+U toggle bold/italic/underline on the selection or the word at the cursor, Ctrl+L / E / R / J set left/center/right/justified alignment, Ctrl+S (or Ctrl+O) saves, Esc closes (asking about unsaved changes). Saving rewrites only word/document.xml and keeps every other part of the file — images, headers, styles — byte-for-byte. Paragraphs containing tracked changes, content controls or fields are shown but locked against editing so their structure survives; selections stay within one paragraph.",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

/// Tab 4: terminal tabs, new documents, sessions.
fn help_terminal_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Terminal tabs run your $SHELL (PowerShell on Windows) — almost every key goes straight to it (Ctrl+C is SIGINT there); Ctrl+T / Ctrl+N / Ctrl+W / Ctrl+PgUp/PgDn / Alt+1-9 / Ctrl+G still manage tabs. Shift+PgUp / Shift+PgDn scrolls the shell's history (2000 lines, plain text); End — or any other key — returns to the live view.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Ctrl+N opens a new blank text document in its own tab; Ctrl+S on it opens a save dialog where you browse to a folder, type a name, and Enter saves (existing files ask to overwrite).",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Ctrl+C never quits the app: outside the editor and file list it cancels like Esc.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Tabs are saved between launches: quitting brings back your browser tabs, editors (including unsaved changes), and terminal tabs (a fresh shell in the same folder). Launching as `fex [path]` skips the restore and starts fresh (a file path opens its parent with the file selected). Closing a tab with Ctrl+W discards its saved state for good.",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

/// Tab 5: interactive accessibility settings for folder navigation.
/// Off by default; changes apply immediately and persist.
fn help_accessibility_lines(th: &Theme, key_delay_ms: u64, alt_nav: bool) -> Vec<Line<'static>> {
    let delay_status = if key_delay_ms == 0 {
        "off".to_string()
    } else {
        format!("{key_delay_ms} ms")
    };
    let alt_status = if alt_nav { "on" } else { "off" };
    let mut lines = vec![
        Line::from(Span::styled(
            "Accessibility — Folder Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "These helpers are off by default. Changes apply immediately and are saved.",
            Style::default().fg(th.dim),
        )),
        Line::from(""),
    ];
    lines.extend(help_key_lines(
        th,
        &[
            (
                "d",
                format!(
                    "toggle the key-press delay (now: {delay_status}; a shaky double-hit is ignored)"
                ),
            ),
            (
                "- / +",
                "adjust the delay in 50 ms steps (0 = off, max 5000 ms)".to_string(),
            ),
            (
                "a",
                format!(
                    "require Alt for navigation (now: {alt_status}; arrows and single-key shortcuts only work while Alt is held)"
                ),
            ),
        ],
    ));
    lines.push(Line::from(""));
    lines.extend([
        Line::from(Span::styled(
            "The delay ignores a keypress that lands within the delay of the previous one, so an accidental double-hit can't trash, delete, or jump somewhere. It only applies in folder navigation.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "With Alt-required on, ? and Esc still work without Alt, and Ctrl combinations are unaffected — so this panel (and the feature switch itself) is always reachable.",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
    lines
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
        let line = csv_row_line(
            &THEMES[0],
            &cells,
            &[3, 3],
            0,
            Some(1),
            false,
            2,
            Some(1),
            &CellHighlights::none(),
        );
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
        let head = csv_row_line(
            &THEMES[0],
            &cells,
            &[3, 3],
            0,
            None,
            true,
            2,
            None,
            &CellHighlights::none(),
        );
        let head_text: String = head.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(head_text, "   │ a   │ bb  │");
        assert!(head.spans[2].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn help_tabs_all_render_content() {
        let th = &THEMES[0];
        let tabs = [
            help_browser_lines(th, true),
            help_editor_lines(),
            help_sheet_lines(),
            help_terminal_lines(),
            help_accessibility_lines(th, 0, false),
        ];
        assert_eq!(tabs.len(), HELP_TABS.len());
        for (i, lines) in tabs.iter().enumerate() {
            assert!(!lines.is_empty(), "tab {i} ({}) is empty", HELP_TABS[i]);
        }
    }

    #[test]
    fn help_accessibility_reflects_settings() {
        let th = &THEMES[0];
        let text = |lines: &Vec<Line>| lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
        let off = text(&help_accessibility_lines(th, 0, false));
        assert!(off.contains("now: off"), "got:\n{off}");
        let on = text(&help_accessibility_lines(th, 250, true));
        assert!(on.contains("now: 250 ms"), "got:\n{on}");
        assert!(on.contains("now: on"), "got:\n{on}");
    }

    #[test]
    fn help_browser_shows_theme_and_line_numbers() {
        let th = &THEMES[0];
        let text: String = help_browser_lines(th, false)
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(th.name), "got:\n{text}");
        assert!(text.contains("line numbers: off"), "got:\n{text}");
    }

    #[test]
    fn split_match_finds_case_insensitive_spans() {
        assert_eq!(
            split_match("Documents/", "doc"),
            Some(("", "Doc", "uments/"))
        );
        assert_eq!(
            split_match("my-photos/", "PHO"),
            Some(("my-", "pho", "tos/"))
        );
        assert_eq!(split_match("abc/", ""), None);
        assert_eq!(split_match("abc/", "z"), None);
        assert_eq!(split_match("ab/", "abc"), None);
    }

    #[test]
    fn truncate_left_keeps_char_boundaries() {
        assert_eq!(truncate_left("abcdef", 10), "abcdef");
        assert_eq!(truncate_left("abcdef", 6), "abcdef");
        assert_eq!(truncate_left("abcdef", 4), "…def");
        // Multibyte chars are never split.
        assert_eq!(truncate_left("héllo wörld", 6), "…örld");
    }
}
