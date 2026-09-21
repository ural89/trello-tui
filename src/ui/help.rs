use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::centered;

const KEYS: &[(&str, &str)] = &[
    ("", "Navigation"),
    ("h / l", "previous / next list"),
    ("j / k", "next / previous card (with count: 5j)"),
    ("gg / G / 5G", "first / last / 5th card"),
    ("Ctrl-d / Ctrl-u", "half page down / up"),
    ("Enter", "open card / board"),
    ("Esc / q", "close / back to boards; q on boards quits"),
    ("", "Cards"),
    (
        "o / O",
        "new card below / above (Enter adds another, Esc stops)",
    ),
    ("r / cw", "edit name / replace name"),
    ("e", "edit description (vim-style editor)"),
    ("dd / u", "archive / undo archive"),
    ("H / L", "move card to previous / next list"),
    ("J / K", "move card down / up (with count)"),
    ("", "Search & commands"),
    ("/  n  N", "search, next / previous match"),
    (":b [name]", "switch board"),
    (":r  R  Ctrl-r", "refresh"),
    (":noh", "clear search highlight"),
    (":q  Ctrl-c", "quit"),
];

pub fn draw(f: &mut Frame, area: Rect) {
    let area = centered(area, 70, 90);
    let lines: Vec<Line> = KEYS
        .iter()
        .map(|&(k, d)| {
            if k.is_empty() {
                Line::styled(d, Style::new().bold().cyan())
            } else {
                Line::from(vec![
                    Span::styled(format!("  {k:<18}"), Style::new().yellow()),
                    Span::raw(d),
                ])
            }
        })
        .collect();
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Keys ".bold())
        .title_bottom(" any key to close ".dim());
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
}
