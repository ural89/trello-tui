use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use super::{centered, label_color};
use crate::app::{App, DescEditor};

pub fn draw_detail(f: &mut Frame, area: Rect, app: &App, scroll: u16) {
    let (Some(card), Some(column)) = (app.current_card(), app.current_column()) else {
        return;
    };
    let area = centered(area, 70, 80);

    let field = |name: &'static str| Span::styled(format!("{name:<8}"), Style::new().dim());
    let mut lines = vec![Line::from(vec![
        field("List"),
        Span::raw(column.list.name.as_str()),
    ])];
    if !card.labels.is_empty() {
        let mut spans = vec![field("Labels")];
        for l in &card.labels {
            let text = if l.name.is_empty() {
                l.color.clone().unwrap_or_default()
            } else {
                l.name.clone()
            };
            spans.push(Span::styled(
                format!("● {text}  "),
                Style::new().fg(label_color(l.color.as_deref())),
            ));
        }
        lines.push(Line::from(spans));
    }
    if let Some(due) = &card.due {
        let when = due.get(..16).unwrap_or(due).replace('T', " ");
        let mut spans = vec![field("Due"), Span::raw(format!("{when} UTC"))];
        if card.due_complete {
            spans.push(Span::styled("  ✓ done", Style::new().green()));
        }
        lines.push(Line::from(spans));
    }
    if !card.short_url.is_empty() {
        lines.push(Line::from(vec![
            field("Link"),
            Span::raw(card.short_url.as_str()).underlined(),
        ]));
    }
    lines.push(Line::raw(""));
    if card.desc.is_empty() {
        lines.push(Line::styled(
            "No description — press e to add one.",
            Style::new().dim(),
        ));
    } else {
        lines.extend(card.desc.lines().map(Line::raw));
    }

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().cyan())
        .title(Span::styled(
            format!(" {} ", card.name),
            Style::new().bold(),
        ))
        .title_bottom(
            " e edit desc · r rename · H/L move · dd archive · D delete · j/k scroll · Esc close "
                .dim(),
        );
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

pub fn draw_desc(f: &mut Frame, area: Rect, ed: &DescEditor) {
    let area = centered(area, 80, 80);
    let mode = if ed.insert {
        " -- INSERT -- · Esc normal mode · Ctrl-s save & close ".to_string()
    } else {
        " NORMAL · i insert · :w save · :q quit · ZZ save & close ".to_string()
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if ed.insert {
            Style::new().green()
        } else {
            Style::new().cyan()
        })
        .title(Span::styled(
            format!(" Description: {} ", ed.card_name),
            Style::new().bold(),
        ))
        .title_bottom(mode.dim());
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let [text, cmdline] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    f.render_widget(&ed.textarea, text);
    let line = match (&ed.cmd, &ed.message) {
        (Some(cmd), _) => Line::raw(format!(":{cmd}█")),
        (None, Some(msg)) => Line::styled(msg.as_str(), Style::new().yellow()),
        _ => Line::raw(""),
    };
    f.render_widget(Paragraph::new(line), cmdline);
}
