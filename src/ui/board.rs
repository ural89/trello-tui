use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, HighlightSpacing, List, ListItem, Paragraph};

use super::label_color;
use crate::app::App;
use crate::model::Card;

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    if app.columns.is_empty() {
        let text = if app.board_loading {
            "Loading board…"
        } else {
            "This board has no lists. Press A to add one."
        };
        f.render_widget(Paragraph::new(text).dim(), area);
        return;
    }

    // The focused list is drawn at double width; the rest keep `column_width`.
    let width = app.column_width.min(area.width).max(1);
    let wide = width.saturating_mul(2).min(area.width);
    let visible = 1 + ((area.width - wide) / width) as usize;
    let len = app.columns.len();
    app.col = app.col.min(len - 1);
    if app.col < app.col_offset {
        app.col_offset = app.col;
    } else if app.col >= app.col_offset + visible {
        app.col_offset = app.col + 1 - visible;
    }
    app.col_offset = app.col_offset.min(len.saturating_sub(visible));
    let (start, end) = (app.col_offset, (app.col_offset + visible).min(len));
    app.half_page = (area.height.saturating_sub(2) / 2).max(1) as usize;

    // When lists overflow the screen, the focused one also takes the leftover
    // (narrower than a column) so the board fills the full width.
    let wide = if len > visible {
        area.width - (visible as u16 - 1) * width
    } else {
        wide
    };
    let slots = Layout::horizontal(
        (start..end).map(|ci| Constraint::Length(if ci == app.col { wide } else { width })),
    )
    .split(area);
    let search = app.search.as_deref();

    for (slot, ci) in (start..end).enumerate() {
        let focused = ci == app.col;
        let column = &mut app.columns[ci];

        let mut title = format!(" {} ({}) ", column.list.name, column.cards.len());
        if ci == start && start > 0 {
            title.insert(0, '◀');
        }
        if ci + 1 == end && end < len {
            title.push('▶');
        }
        let (border, border_type) = if focused {
            (Style::new().fg(Color::Cyan), BorderType::Thick)
        } else {
            (Style::new().fg(Color::DarkGray), BorderType::Rounded)
        };
        let block = Block::bordered()
            .border_type(border_type)
            .border_style(border)
            .title(Span::styled(
                title,
                if focused {
                    Style::new().bold()
                } else {
                    Style::new()
                },
            ));

        let items: Vec<ListItem> = column.cards.iter().map(|c| card_item(c, search)).collect();
        let highlight = if focused {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new().add_modifier(Modifier::DIM)
        };
        let list = List::new(items)
            .block(block)
            .highlight_style(highlight)
            .highlight_symbol(if focused { "▶ " } else { "› " })
            .highlight_spacing(HighlightSpacing::Always);
        f.render_stateful_widget(list, slots[slot], &mut column.state);
    }
}

fn card_item<'a>(card: &'a Card, search: Option<&str>) -> ListItem<'a> {
    let mut spans: Vec<Span> = card
        .labels
        .iter()
        .filter(|l| l.color.is_some())
        .take(4)
        .map(|l| Span::styled("●", Style::new().fg(label_color(l.color.as_deref()))))
        .collect();
    if !spans.is_empty() {
        spans.push(Span::raw(" "));
    }

    let mut name = Style::new();
    if search.is_some_and(|q| card.name.to_lowercase().contains(q)) {
        name = name.yellow().bold();
    }
    if card.is_pending() {
        name = name.italic().dark_gray();
    }
    spans.push(Span::styled(card.name.as_str(), name));

    if let Some(due) = &card.due {
        let date = due.get(5..10).unwrap_or(due); // MM-DD
        let style = if card.due_complete {
            Style::new().green()
        } else {
            Style::new().dim()
        };
        spans.push(Span::styled(format!(" ⏱{date}"), style));
    }
    if !card.desc.is_empty() {
        spans.push(Span::styled(" ≡", Style::new().dim()));
    }
    ListItem::new(Line::from(spans))
}
