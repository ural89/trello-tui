mod board;
mod board_picker;
mod card;
mod help;
mod statusline;

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Color;

use crate::app::{App, Overlay, Screen};

pub fn draw(f: &mut Frame, app: &mut App) {
    let [main, status] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    match app.screen {
        Screen::Picker => board_picker::draw(f, main, app),
        Screen::Board => board::draw(f, main, app),
    }
    match &app.overlay {
        Overlay::None => {}
        Overlay::Detail { scroll } => card::draw_detail(f, main, app, *scroll),
        Overlay::Desc(ed) => card::draw_desc(f, main, ed),
        Overlay::Help => help::draw(f, main),
    }
    statusline::draw(f, status, app);
}

/// A rect of `w` x `h` percent of `area`, centered.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Percentage(w)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Percentage(h)])
        .flex(Flex::Center)
        .areas(area);
    area
}

/// Terminal color for a Trello label color like `"green"` or `"red_dark"`.
pub fn label_color(color: Option<&str>) -> Color {
    match color
        .unwrap_or_default()
        .split('_')
        .next()
        .unwrap_or_default()
    {
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "orange" => Color::Rgb(255, 159, 26),
        "red" => Color::Red,
        "purple" => Color::Magenta,
        "blue" => Color::Blue,
        "sky" => Color::Cyan,
        "lime" => Color::LightGreen,
        "pink" => Color::LightMagenta,
        "black" => Color::DarkGray,
        _ => Color::Gray,
    }
}
