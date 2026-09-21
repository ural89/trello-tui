use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Overlay, PromptKind, Screen};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    if let Some(prompt) = &app.prompt {
        let prefix = match &prompt.kind {
            PromptKind::Command => ":".to_string(),
            PromptKind::Search => "/".to_string(),
            PromptKind::NewCard { col, .. } => {
                let list = app.columns.get(*col).map_or("", |c| c.list.name.as_str());
                format!(" New card in {list}: ")
            }
            PromptKind::Rename { .. } => " Rename: ".to_string(),
        };
        let [p, input] = Layout::horizontal([
            Constraint::Length(prefix.chars().count() as u16),
            Constraint::Min(1),
        ])
        .areas(area);
        f.render_widget(Paragraph::new(prefix).bold(), p);
        f.render_widget(&prompt.input, input);
        return;
    }

    let (mode, color) = match &app.overlay {
        Overlay::Desc(ed) if ed.insert => (" INSERT ", Color::Green),
        _ => (" NORMAL ", Color::Blue),
    };
    let location = match (app.screen, &app.board) {
        (Screen::Board, Some(b)) => b.name.as_str(),
        _ => "Boards",
    };

    let mut right = app.keys.pending_display();
    if app.inflight > 0 {
        right.push_str(" ⟳");
    }
    right.push_str("  ? help ");

    let [left, mid, right_area] = Layout::horizontal([
        Constraint::Length(mode.len() as u16 + location.chars().count() as u16 + 2),
        Constraint::Min(0),
        Constraint::Length(right.chars().count() as u16),
    ])
    .areas(area);

    f.render_widget(
        Line::from(vec![
            Span::styled(mode, Style::new().bg(color).fg(Color::Black).bold()),
            Span::styled(format!(" {location} "), Style::new().bold()),
        ]),
        left,
    );
    if let Some((msg, is_err)) = &app.status {
        let style = if *is_err {
            Style::new().red()
        } else {
            Style::new().green()
        };
        f.render_widget(Line::styled(format!(" {msg}"), style), mid);
    }
    f.render_widget(
        Line::styled(right, Style::new().dim()).right_aligned(),
        right_area,
    );
}
