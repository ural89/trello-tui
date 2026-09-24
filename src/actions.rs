//! Turns key presses into state changes and API calls.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Style;
use ratatui_textarea::{CursorMove, DataCursor, TextArea, WrapMode};

use ratatui::widgets::ListState;

use crate::app::{
    App, Column, Confirm, DescEditor, Overlay, Prompt, PromptKind, Screen, Write, pos_between,
};
use crate::keys::Action;
use crate::model::{Card, List};

impl App {
    pub fn on_event(&mut self, ev: Event) {
        // Resize and other events just trigger the redraw that follows every message.
        let Event::Key(key) = ev else { return };
        if key.kind != KeyEventKind::Press {
            return;
        }
        if self.prompt.is_some() {
            return self.on_prompt_key(key);
        }
        if matches!(self.overlay, Overlay::Desc(_)) {
            return self.on_desc_key(key);
        }
        if matches!(self.overlay, Overlay::Help) {
            self.overlay = Overlay::None;
            return;
        }

        let Some(action) = self.keys.feed(key) else {
            return;
        };
        self.status = None;

        match action {
            Action::Quit => self.should_quit = true,
            Action::Command => self.open_prompt(PromptKind::Command, ""),
            Action::Search => self.open_prompt(PromptKind::Search, ""),
            Action::NextMatch => self.search_next(true),
            Action::PrevMatch => self.search_next(false),
            Action::Help => self.overlay = Overlay::Help,
            _ if matches!(self.overlay, Overlay::Detail { .. }) => self.on_detail_action(action),
            _ => match self.screen {
                Screen::Picker => self.on_picker_action(action),
                Screen::Board => self.on_board_action(action),
            },
        }
    }

    // ---- screens --------------------------------------------------------

    fn on_picker_action(&mut self, action: Action) {
        if let Some(t) = nav_target(
            self.picker.selected(),
            self.boards.len(),
            action,
            self.half_page,
        ) {
            self.picker.select(Some(t));
            return;
        }
        match action {
            Action::Open | Action::Right(_) => {
                if let Some(b) = self.picker.selected().and_then(|i| self.boards.get(i)) {
                    self.open_board(b.clone());
                }
            }
            Action::Back if self.search.is_some() => self.search = None,
            Action::Back if self.board.is_some() => self.screen = Screen::Board,
            Action::Back | Action::Close => self.should_quit = true,
            Action::Refresh => self.load_boards(),
            _ => {}
        }
    }

    fn on_board_action(&mut self, action: Action) {
        let len = self.current_column().map_or(0, |c| c.cards.len());
        if let Some(t) = nav_target(self.row(), len, action, self.half_page) {
            self.select_row(t);
            return;
        }
        match action {
            Action::Left(n) => self.col = self.col.saturating_sub(n),
            Action::Right(n) => {
                self.col = (self.col + n).min(self.columns.len().saturating_sub(1));
            }
            Action::Open => {
                if self.current_card().is_some() {
                    self.overlay = Overlay::Detail { scroll: 0 };
                }
            }
            Action::Back if self.search.is_some() => self.search = None,
            Action::Back | Action::Close => self.show_picker(),
            Action::NewList => {
                let index = if self.columns.is_empty() {
                    0
                } else {
                    self.col + 1
                };
                self.open_prompt(PromptKind::NewList { index }, "");
            }
            Action::NewBelow | Action::NewAbove => {
                if self.columns.is_empty() {
                    return self.error("This board has no lists — press A to add one");
                }
                if self.current_column().is_some_and(|c| c.list.is_pending()) {
                    return self.info("List is still being created…");
                }
                let index = match (self.row(), action) {
                    (Some(r), Action::NewBelow) => r + 1,
                    (Some(r), _) => r,
                    (None, _) => 0,
                };
                self.open_prompt(
                    PromptKind::NewCard {
                        col: self.col,
                        index,
                    },
                    "",
                );
            }
            Action::Rename => self.start_rename(true),
            Action::Change => self.start_rename(false),
            Action::EditDesc => self.open_desc_editor(false),
            Action::Archive => self.archive_current(),
            Action::Delete => self.confirm_delete_card(),
            Action::ArchiveList => self.confirm_archive_list(),
            Action::Undo => self.undo_archive(),
            Action::MoveLeft => self.move_across(-1),
            Action::MoveRight => self.move_across(1),
            Action::MoveDown(n) => self.move_within(n as isize),
            Action::MoveUp(n) => self.move_within(-(n as isize)),
            Action::Refresh => self.reload_board(),
            _ => {}
        }
    }

    fn on_detail_action(&mut self, action: Action) {
        let Overlay::Detail { scroll } = &mut self.overlay else {
            return;
        };
        match action {
            Action::Back | Action::Close | Action::Open => self.overlay = Overlay::None,
            Action::Down(n) => *scroll = scroll.saturating_add(n as u16),
            Action::Up(n) => *scroll = scroll.saturating_sub(n as u16),
            Action::Top => *scroll = 0,
            Action::EditDesc => self.open_desc_editor(true),
            Action::Rename => self.start_rename(true),
            Action::Change => self.start_rename(false),
            Action::Archive => {
                self.overlay = Overlay::None;
                self.archive_current();
            }
            Action::Delete => self.confirm_delete_card(),
            Action::MoveLeft => self.move_across(-1),
            Action::MoveRight => self.move_across(1),
            _ => {}
        }
    }

    fn show_picker(&mut self) {
        self.screen = Screen::Picker;
        if let Some(b) = &self.board
            && let Some(i) = self.boards.iter().position(|x| x.id == b.id)
        {
            self.picker.select(Some(i));
        }
    }

    // ---- card operations ------------------------------------------------

    /// The selected card, unless it's still being created on the server.
    fn editable_card(&mut self) -> Option<Card> {
        let card = self.current_card()?.clone();
        if card.is_pending() {
            self.info("Card is still being created…");
            return None;
        }
        Some(card)
    }

    fn start_rename(&mut self, keep_name: bool) {
        if let Some(card) = self.editable_card() {
            let initial = if keep_name { card.name.as_str() } else { "" };
            self.open_prompt(
                PromptKind::Rename {
                    card_id: card.id.clone(),
                },
                initial,
            );
        }
    }

    fn create_card(&mut self, c: usize, index: usize, name: String) {
        let Some(column) = self.columns.get(c) else {
            return;
        };
        let index = index.min(column.cards.len());
        let list_id = column.list.id.clone();
        let temp_id = self.next_temp_id();
        let card = Card {
            id: temp_id.clone(),
            name: name.clone(),
            desc: String::new(),
            id_list: list_id.clone(),
            id_board: String::new(),
            pos: 0.0,
            closed: false,
            labels: Vec::new(),
            due: None,
            due_complete: false,
            short_url: String::new(),
        };
        self.columns[c].cards.insert(index, card);
        let pos = self.reposition(c, index);
        self.col = c;
        self.columns[c].state.select(Some(index));
        self.write(Write::Create {
            temp_id,
            list_id,
            name,
            pos,
        });
        // Keep adding cards below until Esc, like staying in insert mode.
        self.open_prompt(
            PromptKind::NewCard {
                col: c,
                index: index + 1,
            },
            "",
        );
    }

    fn create_list(&mut self, index: usize, name: String) {
        let Some(board_id) = self.board.as_ref().map(|b| b.id.clone()) else {
            return;
        };
        let index = index.min(self.columns.len());
        let prev = index.checked_sub(1).map(|p| self.columns[p].list.pos);
        let next = self.columns.get(index).map(|n| n.list.pos);
        let pos = pos_between(prev, next);
        let temp_id = self.next_temp_id();
        self.columns.insert(
            index,
            Column {
                list: List {
                    id: temp_id.clone(),
                    name: name.clone(),
                    pos,
                },
                cards: Vec::new(),
                state: ListState::default(),
            },
        );
        self.col = index;
        self.write(Write::CreateList {
            temp_id,
            board_id,
            name,
            pos,
        });
        // Keep adding lists to the right until Esc.
        self.open_prompt(PromptKind::NewList { index: index + 1 }, "");
    }

    fn rename_card(&mut self, card_id: &str, name: String) {
        let Some((c, r)) = self.find_card(card_id) else {
            return;
        };
        if self.columns[c].cards[r].name == name {
            return;
        }
        self.columns[c].cards[r].name = name.clone();
        self.save_card(card_id, vec![("name", name)]);
    }

    fn archive_current(&mut self) {
        let Some(card) = self.editable_card() else {
            return;
        };
        let (c, r) = (self.col, self.row().unwrap_or(0));
        self.columns[c].cards.remove(r);
        self.clamp_selection(c);
        self.info(format!("Archived \"{}\" — press u to undo", card.name));
        self.save_card(&card.id, vec![("closed", "true".into())]);
        self.last_archived = Some(card);
    }

    fn confirm_delete_card(&mut self) {
        let Some(card) = self.editable_card() else {
            return;
        };
        self.open_prompt(
            PromptKind::Confirm {
                question: format!("Delete \"{}\" permanently? (y/N) ", card.name),
                action: Confirm::DeleteCard { card_id: card.id },
            },
            "",
        );
    }

    fn confirm_archive_list(&mut self) {
        let Some(column) = self.current_column() else {
            return;
        };
        if column.list.is_pending() || column.cards.iter().any(|c| c.is_pending()) {
            return self.info("List is still being created…");
        }
        let question = format!(
            "Archive list \"{}\" and its {} cards? (y/N) ",
            column.list.name,
            column.cards.len()
        );
        let list_id = column.list.id.clone();
        self.open_prompt(
            PromptKind::Confirm {
                question,
                action: Confirm::ArchiveList { list_id },
            },
            "",
        );
    }

    fn run_confirmed(&mut self, action: Confirm) {
        match action {
            Confirm::DeleteCard { card_id } => {
                let Some((c, r)) = self.find_card(&card_id) else {
                    return;
                };
                let card = self.columns[c].cards.remove(r);
                self.clamp_selection(c);
                if self.overlay_is_detail() {
                    self.overlay = Overlay::None;
                }
                self.info(format!("Deleted \"{}\"", card.name));
                self.write(Write::DeleteCard { card_id });
            }
            Confirm::ArchiveList { list_id } => {
                let Some(c) = self.columns.iter().position(|col| col.list.id == list_id) else {
                    return;
                };
                let column = self.columns.remove(c);
                self.col = self.col.min(self.columns.len().saturating_sub(1));
                self.info(format!("Archived list \"{}\"", column.list.name));
                self.write(Write::ArchiveList { list_id });
            }
        }
    }

    fn overlay_is_detail(&self) -> bool {
        matches!(self.overlay, Overlay::Detail { .. })
    }

    fn undo_archive(&mut self) {
        let Some(mut card) = self.last_archived.take() else {
            return self.info("Nothing to undo");
        };
        let Some(c) = self
            .columns
            .iter()
            .position(|col| col.list.id == card.id_list)
        else {
            return self.error("The card's list no longer exists");
        };
        card.closed = false;
        let i = self.columns[c].cards.partition_point(|x| x.pos < card.pos);
        let (id, name) = (card.id.clone(), card.name.clone());
        self.columns[c].cards.insert(i, card);
        self.col = c;
        self.columns[c].state.select(Some(i));
        self.save_card(&id, vec![("closed", "false".into())]);
        self.info(format!("Restored \"{name}\""));
    }

    fn move_within(&mut self, delta: isize) {
        let Some(card) = self.editable_card() else {
            return;
        };
        let c = self.col;
        let r = self.row().unwrap_or(0);
        let last = self.columns[c].cards.len() as isize - 1;
        let t = (r as isize + delta).clamp(0, last) as usize;
        if t == r {
            return;
        }
        let moved = self.columns[c].cards.remove(r);
        self.columns[c].cards.insert(t, moved);
        let pos = self.reposition(c, t);
        self.columns[c].state.select(Some(t));
        self.save_card(&card.id, vec![("pos", pos.to_string())]);
    }

    fn move_across(&mut self, dir: isize) {
        let Some(card) = self.editable_card() else {
            return;
        };
        let c = self.col;
        let t = c as isize + dir;
        if t < 0 || t as usize >= self.columns.len() {
            return;
        }
        let t = t as usize;
        if self.columns[t].list.is_pending() {
            return self.info("List is still being created…");
        }
        let r = self.row().unwrap_or(0);
        let mut moved = self.columns[c].cards.remove(r);
        self.clamp_selection(c);

        let list_id = self.columns[t].list.id.clone();
        moved.id_list = list_id.clone();
        let i = r.min(self.columns[t].cards.len());
        self.columns[t].cards.insert(i, moved);
        let pos = self.reposition(t, i);
        self.col = t;
        self.columns[t].state.select(Some(i));
        self.save_card(
            &card.id,
            vec![("idList", list_id), ("pos", pos.to_string())],
        );
    }

    // ---- search ---------------------------------------------------------

    fn search_next(&mut self, forward: bool) {
        let Some(query) = self.search.clone() else {
            return self.info("No previous search");
        };
        let matches = |name: &str| name.to_lowercase().contains(&query);

        // Every navigable item as (column, row), in reading order.
        let (order, current): (Vec<Pos>, Option<Pos>) = match self.screen {
            Screen::Picker => (
                (0..self.boards.len()).map(|i| (0, i)).collect(),
                self.picker.selected().map(|r| (0, r)),
            ),
            Screen::Board => (
                self.columns
                    .iter()
                    .enumerate()
                    .flat_map(|(c, col)| (0..col.cards.len()).map(move |r| (c, r)))
                    .collect(),
                self.row().map(|r| (self.col, r)),
            ),
        };
        let name_at = |(c, r): (usize, usize)| match self.screen {
            Screen::Picker => self.boards[r].name.as_str(),
            Screen::Board => self.columns[c].cards[r].name.as_str(),
        };

        let len = order.len();
        let (start, skip) = match current.and_then(|cur| order.iter().position(|&p| p == cur)) {
            Some(i) => (i, 1),
            None => (
                order.iter().position(|&(c, _)| c >= self.col).unwrap_or(0),
                0,
            ),
        };
        let found = (0..len).map(|k| k + skip).find_map(|step| {
            let idx = if forward {
                (start + step) % len
            } else {
                (start + len * 2 - step) % len
            };
            matches(name_at(order[idx])).then_some(order[idx])
        });

        match (found, self.screen) {
            (None, _) => self.error(format!("Pattern not found: {query}")),
            (Some((_, r)), Screen::Picker) => self.picker.select(Some(r)),
            (Some((c, r)), Screen::Board) => {
                self.col = c;
                self.columns[c].state.select(Some(r));
            }
        }
    }

    // ---- bottom-line prompt ---------------------------------------------

    fn open_prompt(&mut self, kind: PromptKind, initial: &str) {
        let mut input = TextArea::new(vec![initial.to_string()]);
        input.move_cursor(CursorMove::End);
        input.set_cursor_line_style(Style::default());
        self.keys.reset();
        self.prompt = Some(Prompt { kind, input });
    }

    fn on_prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = &mut self.prompt else {
            return;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let empty = prompt.input.lines().iter().all(|l| l.is_empty());
        let is_ex = matches!(prompt.kind, PromptKind::Command | PromptKind::Search);
        if matches!(prompt.kind, PromptKind::Confirm { .. }) {
            let Some(Prompt {
                kind: PromptKind::Confirm { action, .. },
                ..
            }) = self.prompt.take()
            else {
                return;
            };
            if matches!(key.code, KeyCode::Char('y' | 'Y')) && !ctrl {
                self.run_confirmed(action);
            }
            return;
        }
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Char('c') if ctrl => self.prompt = None,
            KeyCode::Backspace if empty && is_ex => self.prompt = None,
            KeyCode::Char('u') if ctrl => {
                prompt.input.move_cursor(CursorMove::End);
                prompt.input.delete_line_by_head();
            }
            KeyCode::Char('w') if ctrl => {
                prompt.input.delete_word();
            }
            KeyCode::Enter => {
                let Some(prompt) = self.prompt.take() else {
                    return;
                };
                let text = prompt.input.into_lines().join(" ");
                self.submit_prompt(prompt.kind, text);
            }
            _ => {
                prompt.input.input(key);
            }
        }
    }

    fn submit_prompt(&mut self, kind: PromptKind, text: String) {
        let text = text.trim().to_string();
        match kind {
            PromptKind::Command => self.run_command(&text),
            PromptKind::Search => {
                if !text.is_empty() {
                    self.search = Some(text.to_lowercase());
                }
                self.search_next(true);
            }
            PromptKind::NewCard { col, index } if !text.is_empty() => {
                self.create_card(col, index, text)
            }
            PromptKind::NewList { index } if !text.is_empty() => self.create_list(index, text),
            PromptKind::Rename { card_id } if !text.is_empty() => self.rename_card(&card_id, text),
            _ => {}
        }
    }

    fn run_command(&mut self, input: &str) {
        let (cmd, arg) = input.split_once(' ').unwrap_or((input, ""));
        let arg = arg.trim();
        match cmd {
            "" => {}
            "q" | "q!" | "qa" | "qa!" | "quit" | "wq" | "x" => self.should_quit = true,
            "b" | "board" | "boards" if arg.is_empty() => {
                self.overlay = Overlay::None;
                self.show_picker();
            }
            "b" | "board" => {
                let needle = arg.to_lowercase();
                let found = self
                    .boards
                    .iter()
                    .find(|b| b.name.to_lowercase() == needle)
                    .or_else(|| {
                        self.boards
                            .iter()
                            .find(|b| b.name.to_lowercase().contains(&needle))
                    })
                    .cloned();
                match found {
                    Some(b) => self.open_board(b),
                    None => self.error(format!("No board matching \"{arg}\"")),
                }
            }
            "r" | "e" | "e!" | "refresh" => match self.screen {
                Screen::Picker => self.load_boards(),
                Screen::Board => self.reload_board(),
            },
            "h" | "help" => self.overlay = Overlay::Help,
            "noh" | "nohlsearch" => self.search = None,
            n if n.parse::<usize>().is_ok() => {
                let action = Action::Goto(n.parse().ok());
                match self.screen {
                    Screen::Picker => self.on_picker_action(action),
                    Screen::Board => self.on_board_action(action),
                }
            }
            _ => self.error(format!("Not a command: {cmd}")),
        }
    }

    // ---- description editor ---------------------------------------------

    fn open_desc_editor(&mut self, return_to_detail: bool) {
        let Some(card) = self.editable_card() else {
            return;
        };
        let lines: Vec<String> = card.desc.lines().map(String::from).collect();
        let empty = lines.is_empty();
        let mut textarea = TextArea::new(if empty { vec![String::new()] } else { lines });
        textarea.set_wrap_mode(WrapMode::Word);
        textarea.set_cursor_line_style(Style::default());
        self.overlay = Overlay::Desc(Box::new(DescEditor {
            card_id: card.id,
            card_name: card.name,
            original: card.desc,
            textarea,
            insert: false,
            pending: None,
            cmd: None,
            message: None,
            return_to_detail,
        }));
    }

    fn close_desc(&mut self, save: bool) {
        let Overlay::Desc(ed) = std::mem::replace(&mut self.overlay, Overlay::None) else {
            return;
        };
        if save {
            self.commit_desc(&ed.card_id, ed.textarea.lines().join("\n"), &ed.original);
        }
        if ed.return_to_detail {
            self.overlay = Overlay::Detail { scroll: 0 };
        }
    }

    fn commit_desc(&mut self, card_id: &str, desc: String, original: &str) {
        if desc == original {
            return;
        }
        if let Some((c, r)) = self.find_card(card_id) {
            self.columns[c].cards[r].desc = desc.clone();
        }
        self.save_card(card_id, vec![("desc", desc)]);
    }

    fn desc_command(&mut self, cmd: &str) {
        let Overlay::Desc(ed) = &mut self.overlay else {
            return;
        };
        let text = ed.textarea.lines().join("\n");
        let modified = text != ed.original;
        match cmd {
            "w" => {
                let (id, original) = (
                    ed.card_id.clone(),
                    std::mem::replace(&mut ed.original, text.clone()),
                );
                ed.message = Some("Saved".into());
                self.commit_desc(&id, text, &original);
            }
            "wq" | "x" => self.close_desc(true),
            "q" if modified => {
                ed.message = Some("No write since last change (add ! to override)".into());
            }
            "q" | "q!" => self.close_desc(false),
            "" => {}
            other => ed.message = Some(format!("Not a command: {other}")),
        }
    }

    fn on_desc_key(&mut self, key: KeyEvent) {
        let Overlay::Desc(ed) = &mut self.overlay else {
            return;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('s') {
            return self.close_desc(true);
        }

        if ed.insert {
            if key.code == KeyCode::Esc {
                ed.insert = false;
                if ed.textarea.cursor().1 > 0 {
                    ed.textarea.move_cursor(CursorMove::Back);
                }
            } else {
                ed.textarea.input(key);
            }
            return;
        }

        if let Some(cmd) = &mut ed.cmd {
            match key.code {
                KeyCode::Esc => ed.cmd = None,
                KeyCode::Enter => {
                    let cmd = ed.cmd.take().unwrap_or_default();
                    self.desc_command(cmd.trim());
                }
                KeyCode::Backspace => {
                    if cmd.pop().is_none() {
                        ed.cmd = None;
                    }
                }
                KeyCode::Char(c) => cmd.push(c),
                _ => {}
            }
            return;
        }

        ed.message = None;
        let ta = &mut ed.textarea;

        if let Some(p) = ed.pending.take() {
            match (p, key.code) {
                ('d', KeyCode::Char('d')) => delete_line(ta),
                ('g', KeyCode::Char('g')) => ta.move_cursor(CursorMove::Top),
                ('Z', KeyCode::Char('Z')) => self.close_desc(true),
                ('Z', KeyCode::Char('Q')) => self.close_desc(false),
                _ => {}
            }
            return;
        }

        let DataCursor(row, col) = ta.cursor();
        let line_len = ta.lines()[row].chars().count();
        match key.code {
            KeyCode::Char('r') if ctrl => {
                ta.redo();
            }
            KeyCode::Char(c @ ('d' | 'g' | 'Z')) => ed.pending = Some(c),
            KeyCode::Char('i') => ed.insert = true,
            KeyCode::Char('a') => {
                if col < line_len {
                    ta.move_cursor(CursorMove::Forward);
                }
                ed.insert = true;
            }
            KeyCode::Char('A') => {
                ta.move_cursor(CursorMove::End);
                ed.insert = true;
            }
            KeyCode::Char('I') => {
                ta.move_cursor(CursorMove::Head);
                ed.insert = true;
            }
            KeyCode::Char('o') => {
                ta.move_cursor(CursorMove::End);
                ta.insert_newline();
                ed.insert = true;
            }
            KeyCode::Char('O') => {
                ta.move_cursor(CursorMove::Head);
                ta.insert_newline();
                ta.move_cursor(CursorMove::Up);
                ed.insert = true;
            }
            KeyCode::Char('h') | KeyCode::Left if col > 0 => ta.move_cursor(CursorMove::Back),
            KeyCode::Char('l') | KeyCode::Right if col < line_len => {
                ta.move_cursor(CursorMove::Forward)
            }
            KeyCode::Char('j') | KeyCode::Down => ta.move_cursor(CursorMove::Down),
            KeyCode::Char('k') | KeyCode::Up => ta.move_cursor(CursorMove::Up),
            KeyCode::Char('w') => ta.move_cursor(CursorMove::WordForward),
            KeyCode::Char('b') => ta.move_cursor(CursorMove::WordBack),
            KeyCode::Char('e') => ta.move_cursor(CursorMove::WordEnd),
            KeyCode::Char('0') | KeyCode::Home => ta.move_cursor(CursorMove::Head),
            KeyCode::Char('$') | KeyCode::End => ta.move_cursor(CursorMove::End),
            KeyCode::Char('G') => ta.move_cursor(CursorMove::Bottom),
            KeyCode::Char('x') => {
                ta.delete_next_char();
            }
            KeyCode::Char('D') => {
                ta.delete_line_by_end();
            }
            KeyCode::Char('p') => {
                ta.paste();
            }
            KeyCode::Char('u') => {
                ta.undo();
            }
            KeyCode::Char(':') => ed.cmd = Some(String::new()),
            KeyCode::Esc => {
                if ta.lines().join("\n") == ed.original {
                    self.close_desc(false);
                } else {
                    ed.message = Some("Unsaved changes — :w to save, :q! to discard".into());
                }
            }
            _ => {}
        }
    }
}

/// (column, row) of a card, or (0, index) of a board in the picker.
type Pos = (usize, usize);

/// vim `dd`: removes the cursor line (into the yank buffer, so `p` pastes it).
fn delete_line(ta: &mut TextArea) {
    let DataCursor(row, _) = ta.cursor();
    let n = ta.lines().len();
    if n == 1 {
        ta.move_cursor(CursorMove::Head);
        ta.delete_line_by_end();
    } else if row + 1 < n {
        ta.move_cursor(CursorMove::Head);
        ta.start_selection();
        ta.move_cursor(CursorMove::Down);
        ta.move_cursor(CursorMove::Head);
        ta.cut();
    } else {
        ta.move_cursor(CursorMove::Up);
        ta.move_cursor(CursorMove::End);
        ta.start_selection();
        ta.move_cursor(CursorMove::Bottom);
        ta.move_cursor(CursorMove::End);
        ta.cut();
        ta.move_cursor(CursorMove::Head);
    }
}

/// Where a vertical motion lands in a list of `len` items, if `action` is one.
fn nav_target(cur: Option<usize>, len: usize, action: Action, half: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let cur = cur.unwrap_or(0);
    let last = len - 1;
    Some(match action {
        Action::Down(n) => cur.saturating_add(n).min(last),
        Action::Up(n) => cur.saturating_sub(n),
        Action::Top => 0,
        Action::Goto(None) => last,
        Action::Goto(Some(n)) => n.saturating_sub(1).min(last),
        Action::HalfPageDown => cur.saturating_add(half).min(last),
        Action::HalfPageUp => cur.saturating_sub(half),
        _ => return None,
    })
}
