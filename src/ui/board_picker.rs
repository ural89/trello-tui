use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::widgets::{Block, BorderType, List, ListItem, Paragraph};

use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let [area] = Layout::horizontal([Constraint::Max(60)])
        .flex(Flex::Center)
        .areas(area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Boards ".bold())
        .title_bottom(" j/k move · Enter open · / search · :q quit ".dim());

    if !app.boards_loaded || app.boards.is_empty() {
        let text = if app.boards_loaded {
            "No open boards."
        } else {
            "Loading boards…"
        };
        f.render_widget(Paragraph::new(text).dim().block(block), area);
        return;
    }

    let search = app.search.as_deref();
    let items: Vec<ListItem> = app
        .boards
        .iter()
        .map(|b| {
            let hit = search.is_some_and(|q| b.name.to_lowercase().contains(q));
            let style = if hit {
                Style::new().yellow().bold()
            } else {
                Style::new()
            };
            ListItem::new(b.name.as_str()).style(style)
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, &mut app.picker);
}
