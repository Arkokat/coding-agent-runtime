//! 3-pane render: header / session list / detail / footer.
//! Modals overlay on top of the session list.

use crate::tui::color::{color_for, style_for, symbol_for};
use crate::tui::state::{NewModal, RenameModal, StatusCounters, TuiState};
use agentd_protocol::Session;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

const HEADER_H: u16 = 1;
const FOOTER_H: u16 = 1;
const STATUS_H: u16 = 1;
const MODAL_INNER_W: u16 = 60;
const MODAL_INNER_H: u16 = 14;

/// Render the full TUI frame for `state` into `frame`.
///
/// Layout (vertical):
///   - header (status counters + cost)
///   - body split horizontally into session list (top) + detail (bottom)
///   - status bar (transient message)
///   - footer (key hints)
///
/// Modals (`show_help`, `rename_modal`, `new_modal`) draw on top via centered
/// `Clear` rectangles, in that order.
pub fn render(frame: &mut Frame, state: &TuiState) {
    let area = frame.area();
    if area.height < 4 || area.width < 20 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_H),
            Constraint::Min(3),
            Constraint::Length(STATUS_H),
            Constraint::Length(FOOTER_H),
        ])
        .split(area);

    render_header(chunks[0], frame, state);

    let body = chunks[1];
    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(body);
    render_session_list(body_chunks[0], frame, state);
    render_detail(body_chunks[1], frame, state);

    render_status_bar(chunks[2], frame, state);
    render_footer(chunks[3], frame);

    if state.show_help {
        render_help_modal(area, frame);
    }
    if let Some(modal) = &state.rename_modal {
        render_rename_modal(area, frame, modal);
    }
    if let Some(modal) = &state.new_modal {
        render_new_modal(area, frame, modal);
    }
}

fn render_header(area: Rect, frame: &mut Frame, state: &TuiState) {
    let c = StatusCounters::from_sessions(&state.sessions);
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!("{} agents", c.working),
            style_for(crate::tui::StatusColor::Working),
        ),
        Span::raw(" · "),
        Span::styled(
            format!("{} waiting", c.waiting),
            style_for(crate::tui::StatusColor::Waiting),
        ),
        Span::raw(" · "),
        Span::styled(
            format!("{} errored", c.errored),
            style_for(crate::tui::StatusColor::Errored),
        ),
        Span::raw(" · "),
        Span::styled(
            format!("${:.2}", total_cost(&state.sessions)),
            Style::default().dim(),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_session_list(area: Rect, frame: &mut Frame, state: &TuiState) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if state.sessions.is_empty() {
        let p = Paragraph::new("  (no active sessions)").dim();
        frame.render_widget(p, inner);
        return;
    }
    let items: Vec<ListItem> = state
        .sessions
        .iter()
        .map(|s| {
            let sym = symbol_for(s.status);
            let style = style_for(color_for(s.status));
            let flash = state.flash_until.get(&s.id).is_some_and(|t| {
                t.saturating_duration_since(std::time::Instant::now())
                    .as_millis()
                    > 0
            });
            let style = if flash {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            };
            let line = Line::from(vec![
                Span::raw(" "),
                Span::styled(sym, style),
                Span::raw("  "),
                Span::styled(
                    truncate(&s.display_name, inner.width.saturating_sub(20) as usize),
                    style,
                ),
                Span::raw("  "),
                Span::styled(format!("{:?}", s.status), style.dim()),
                Span::raw("  "),
                Span::raw(truncate(
                    &s.current_task.clone().unwrap_or_default(),
                    inner.width.saturating_sub(20) as usize,
                )),
            ]);
            ListItem::new(line)
        })
        .collect();
    let mut list_state = ListState::default();
    list_state.select(Some(
        state.selected.min(state.sessions.len().saturating_sub(1)),
    ));
    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, inner, &mut list_state);
}

fn render_detail(area: Rect, frame: &mut Frame, state: &TuiState) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(session) = state.selected_session() else {
        let p = Paragraph::new("  (select a session)").dim();
        frame.render_widget(p, inner);
        return;
    };
    let pct = match (session.context_used_tokens, session.context_total_tokens) {
        (Some(u), Some(t)) if t > 0 => u32::try_from(u.saturating_mul(100) / t).unwrap_or(u32::MAX),
        _ => 0,
    };
    let cost = session
        .cost_usd
        .map_or_else(|| "$0.00".into(), |c| format!("${c:.2}"));
    let tokens = match (session.context_used_tokens, session.context_total_tokens) {
        (Some(u), Some(t)) => format!("{u}/{t} ({pct}%)"),
        _ => "n/a".into(),
    };
    let lines = vec![
        Line::from(format!(
            "{} in {}",
            session.display_name, session.working_dir
        )),
        Line::from(format!(
            "  task: {}",
            session.current_task.clone().unwrap_or_else(|| "n/a".into())
        )),
        Line::from(format!("  tokens: {tokens}")),
        Line::from(format!("  cost: {cost}")),
        Line::from(format!("  status: {:?}", session.status)),
    ];
    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn render_status_bar(area: Rect, frame: &mut Frame, state: &TuiState) {
    let now = std::time::Instant::now();
    let msg = state.status_message.as_ref().and_then(|(m, t)| {
        if now.duration_since(*t) < crate::tui::STATUS_MESSAGE_DURATION {
            Some(m.clone())
        } else {
            None
        }
    });
    let text = msg.unwrap_or_else(|| " ".into());
    frame.render_widget(Paragraph::new(text), area);
}

fn render_footer(area: Rect, frame: &mut Frame) {
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("c", Style::default().bold()),
        Span::raw("=create  "),
        Span::styled("r", Style::default().bold()),
        Span::raw("=rename  "),
        Span::styled("j", Style::default().bold()),
        Span::raw("=jump  "),
        Span::styled("x", Style::default().bold()),
        Span::raw("=kill  "),
        Span::styled("?", Style::default().bold()),
        Span::raw("=help  "),
        Span::styled("q", Style::default().bold()),
        Span::raw("=quit"),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_help_modal(area: Rect, frame: &mut Frame) {
    let modal = centered_rect(MODAL_INNER_W, MODAL_INNER_H, area);
    frame.render_widget(Clear, modal);
    let block = Block::default().borders(Borders::ALL).title(" Help ");
    let text = vec![
        Line::from("Navigation:"),
        Line::from("  j/k or arrows  move selection"),
        Line::from("  g/G            top/bottom"),
        Line::from(""),
        Line::from("Actions:"),
        Line::from("  Enter          jump to selected session"),
        Line::from("  r              rename session"),
        Line::from("  c              create new session"),
        Line::from("  x              kill selected session"),
        Line::from(""),
        Line::from("Misc:"),
        Line::from("  ?              toggle this help"),
        Line::from("  q              quit"),
        Line::from(""),
        Line::from("Press ? or Esc to close."),
    ];
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        modal,
    );
}

fn render_rename_modal(area: Rect, frame: &mut Frame, modal: &RenameModal) {
    let m = centered_rect(MODAL_INNER_W, 5, area);
    frame.render_widget(Clear, m);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Rename session ");
    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  New name: "),
            Span::styled(&modal.input, Style::default().bold()),
            Span::styled("_", Style::default().reversed()),
        ]),
        Line::from(""),
        Line::from("  Enter to commit · Esc to cancel"),
    ];
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        m,
    );
}

fn render_new_modal(area: Rect, frame: &mut Frame, modal: &NewModal) {
    let h = u16::try_from(modal.recents.len())
        .ok()
        .and_then(|n| n.checked_add(6))
        .map_or(0, |n| n.min(area.height.saturating_sub(2)));
    let m = centered_rect(MODAL_INNER_W + 8, h, area);
    frame.render_widget(Clear, m);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" New session ");
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::raw("  "),
            Span::styled(&modal.query, Style::default().bold()),
            Span::styled("_", Style::default().reversed()),
        ]),
        Line::from(""),
        Line::from("  Recent:"),
    ];
    for (i, (path, _ts)) in modal.recents.iter().enumerate() {
        let prefix = if i == 0 { "> " } else { "  " };
        let display = path.to_string_lossy();
        let matches = if modal.query.is_empty() {
            true
        } else {
            display.to_lowercase().contains(&modal.query.to_lowercase())
        };
        let style = if !matches {
            Style::default().dim()
        } else if i == 0 {
            Style::default().bold()
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::raw(prefix),
            Span::styled(display, style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("  Enter to create · Esc to cancel"));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        m,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

fn total_cost(sessions: &[Session]) -> f64 {
    sessions
        .iter()
        .filter_map(|s| s.cost_usd)
        .fold(0.0_f64, |acc, x| acc + x)
}
