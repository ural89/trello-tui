use std::collections::HashMap;
use std::future::Future;

use crossterm::event::Event;
use ratatui::widgets::ListState;
use ratatui_textarea::TextArea;
use tokio::sync::mpsc::UnboundedSender;

use crate::api::TrelloClient;
use crate::config::Config;
use crate::keys::KeyParser;
use crate::model::{Board, Card, List};

/// Default gap Trello uses between positions.
const POS_STEP: f64 = 65536.0;

pub enum Msg {
    Input(Event),
    Boards(anyhow::Result<Vec<Board>>),
    BoardData {
        board_id: String,
        result: anyhow::Result<(Vec<List>, Vec<Card>)>,
    },
    CardCreated {
        temp_id: String,
        result: anyhow::Result<Card>,
    },
    CardSaved {
        card_id: String,
        result: anyhow::Result<Card>,
    },
}

/// A mutation. Writes run one at a time, in order, so rapid edits to the same
/// card (e.g. `JJJ`, or `dd` then `u`) reach Trello in the order they were made.
pub enum Write {
    Create {
        temp_id: String,
        list_id: String,
        name: String,
        pos: f64,
    },
    Update {
        card_id: String,
        fields: Vec<(&'static str, String)>,
    },
}

fn start_writer(client: TrelloClient, tx: UnboundedSender<Msg>) -> UnboundedSender<Write> {
    let (wtx, mut wrx) = tokio::sync::mpsc::unbounded_channel::<Write>();
    tokio::spawn(async move {
        while let Some(job) = wrx.recv().await {
            let msg = match job {
                Write::Create {
                    temp_id,
                    list_id,
                    name,
                    pos,
                } => Msg::CardCreated {
                    temp_id,
                    result: client.create_card(&list_id, &name, pos).await,
                },
                Write::Update { card_id, fields } => {
                    let result = client.update_card(&card_id, &fields).await;
                    Msg::CardSaved { card_id, result }
                }
            };
            if tx.send(msg).is_err() {
                break;
            }
        }
    });
    wtx
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Picker,
    Board,
}

pub enum PromptKind {
    Command,
    Search,
    NewCard { col: usize, index: usize },
    Rename { card_id: String },
}

pub struct Prompt {
    pub kind: PromptKind,
    pub input: TextArea<'static>,
}

pub struct DescEditor {
    pub card_id: String,
    pub card_name: String,
    pub original: String,
    pub textarea: TextArea<'static>,
    pub insert: bool,
    pub pending: Option<char>,
    /// `Some` while typing an ex command (`:w`, `:q`, ...).
    pub cmd: Option<String>,
    pub message: Option<String>,
    pub return_to_detail: bool,
}

pub enum Overlay {
    None,
    Detail { scroll: u16 },
    Help,
    Desc(Box<DescEditor>),
}

pub struct Column {
    pub list: List,
    pub cards: Vec<Card>,
    pub state: ListState,
}

pub struct App {
    pub client: TrelloClient,
    tx: UnboundedSender<Msg>,
    writer: UnboundedSender<Write>,
    /// Queued updates per card; server copies are adopted only once none remain.
    pending_saves: HashMap<String, usize>,
    pub column_width: u16,
    default_board: Option<String>,

    pub screen: Screen,
    pub overlay: Overlay,
    pub prompt: Option<Prompt>,
    pub keys: KeyParser,

    pub boards: Vec<Board>,
    pub boards_loaded: bool,
    pub picker: ListState,

    pub board: Option<Board>,
    pub board_loading: bool,
    pub columns: Vec<Column>,
    pub col: usize,
    /// First visible column; adjusted by the renderer.
    pub col_offset: usize,
    /// Rows moved by Ctrl-d / Ctrl-u; set by the renderer from the column height.
    pub half_page: usize,

    pub search: Option<String>,
    pub status: Option<(String, bool)>,
    pub inflight: usize,
    pub last_archived: Option<Card>,
    temp_seq: u64,
    pub should_quit: bool,
}

impl App {
    pub fn new(config: Config, tx: UnboundedSender<Msg>) -> Self {
        let client = TrelloClient::new(config.api_key, config.token);
        Self {
            writer: start_writer(client.clone(), tx.clone()),
            pending_saves: HashMap::new(),
            client,
            tx,
            column_width: config.column_width,
            default_board: config.default_board,
            screen: Screen::Picker,
            overlay: Overlay::None,
            prompt: None,
            keys: KeyParser::default(),
            boards: Vec::new(),
            boards_loaded: false,
            picker: ListState::default(),
            board: None,
            board_loading: false,
            columns: Vec::new(),
            col: 0,
            col_offset: 0,
            half_page: 10,
            search: None,
            status: None,
            inflight: 0,
            last_archived: None,
            temp_seq: 0,
            should_quit: false,
        }
    }

    // ---- async plumbing -------------------------------------------------

    /// Runs an API call in the background; its result comes back as a `Msg`.
    pub fn spawn<F, Fut>(&mut self, f: F)
    where
        F: FnOnce(TrelloClient) -> Fut,
        Fut: Future<Output = Msg> + Send + 'static,
    {
        self.inflight += 1;
        let tx = self.tx.clone();
        let fut = f(self.client.clone());
        tokio::spawn(async move {
            let _ = tx.send(fut.await);
        });
    }

    pub fn load_boards(&mut self) {
        self.spawn(|c| async move { Msg::Boards(c.boards().await) });
    }

    pub fn open_board(&mut self, board: Board) {
        self.screen = Screen::Board;
        self.overlay = Overlay::None;
        if self.board.as_ref().is_some_and(|b| b.id == board.id) && !self.columns.is_empty() {
            return;
        }
        self.columns.clear();
        self.col = 0;
        self.col_offset = 0;
        self.last_archived = None;
        self.board = Some(board);
        self.reload_board();
    }

    pub fn reload_board(&mut self) {
        let Some(board) = &self.board else { return };
        let board_id = board.id.clone();
        self.board_loading = true;
        self.spawn(|c| async move {
            let result = tokio::try_join!(c.lists(&board_id), c.cards(&board_id));
            Msg::BoardData { board_id, result }
        });
    }

    pub fn write(&mut self, job: Write) {
        if let Write::Update { card_id, .. } = &job {
            *self.pending_saves.entry(card_id.clone()).or_default() += 1;
        }
        self.inflight += 1;
        let _ = self.writer.send(job);
    }

    pub fn save_card(&mut self, card_id: &str, fields: Vec<(&'static str, String)>) {
        self.write(Write::Update {
            card_id: card_id.to_string(),
            fields,
        });
    }

    pub fn info(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), false));
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), true));
    }

    pub fn next_temp_id(&mut self) -> String {
        self.temp_seq += 1;
        format!("tmp-{}", self.temp_seq)
    }

    // ---- message handling -----------------------------------------------

    pub fn handle(&mut self, msg: Msg) {
        if !matches!(msg, Msg::Input(_)) {
            self.inflight = self.inflight.saturating_sub(1);
        }
        match msg {
            Msg::Input(ev) => self.on_event(ev),
            Msg::Boards(Ok(boards)) => {
                self.boards = boards;
                self.boards_loaded = true;
                let sel = self.picker.selected().unwrap_or(0);
                self.picker
                    .select((!self.boards.is_empty()).then(|| sel.min(self.boards.len() - 1)));
                if let Some(name) = self.default_board.take() {
                    let name = name.to_lowercase();
                    match self.boards.iter().find(|b| b.name.to_lowercase() == name) {
                        Some(b) => self.open_board(b.clone()),
                        None => self.error(format!("default_board '{name}' not found")),
                    }
                }
            }
            Msg::Boards(Err(e)) => {
                self.boards_loaded = true;
                self.error(format!("Loading boards failed: {e:#}"));
            }
            Msg::BoardData { board_id, result } => {
                if self.board.as_ref().map(|b| &b.id) != Some(&board_id) {
                    return; // switched boards meanwhile
                }
                self.board_loading = false;
                match result {
                    Ok((lists, cards)) => self.set_board_data(lists, cards),
                    Err(e) => self.error(format!("Loading board failed: {e:#}")),
                }
            }
            Msg::CardCreated { temp_id, result } => {
                let found = self.find_card(&temp_id);
                match (result, found) {
                    (Ok(card), Some((c, r))) => self.columns[c].cards[r] = card,
                    (Ok(_), None) => {} // board reloaded or switched meanwhile
                    (Err(e), found) => {
                        if let Some((c, r)) = found {
                            self.columns[c].cards.remove(r);
                            self.clamp_selection(c);
                        }
                        self.error(format!("Creating card failed: {e:#}"));
                    }
                }
            }
            Msg::CardSaved { card_id, result } => {
                let remaining = match self.pending_saves.get_mut(&card_id) {
                    Some(n) if *n > 1 => {
                        *n -= 1;
                        *n
                    }
                    _ => {
                        self.pending_saves.remove(&card_id);
                        0
                    }
                };
                match result {
                    // Local state was already updated optimistically. Once the
                    // last queued write lands, adopt the server's copy (Trello
                    // may rebalance `pos`). Archived cards were removed locally.
                    Ok(card) if remaining == 0 && !card.closed => {
                        if let Some((c, r)) = self.find_card(&card.id) {
                            self.columns[c].cards[r] = card;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        self.error(format!("Saving failed, reloaded board: {e:#}"));
                        self.reload_board();
                    }
                }
            }
        }
    }

    fn set_board_data(&mut self, mut lists: Vec<List>, cards: Vec<Card>) {
        // Keep the cursor on the same card / list across reloads.
        let prev_list = self.current_column().map(|c| c.list.id.clone());
        let prev_card = self.current_card().map(|c| c.id.clone());
        let prev_rows: Vec<(String, Option<usize>)> = self
            .columns
            .iter()
            .map(|c| (c.list.id.clone(), c.state.selected()))
            .collect();

        lists.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        self.columns = lists
            .into_iter()
            .map(|list| Column {
                list,
                cards: Vec::new(),
                state: ListState::default(),
            })
            .collect();
        for card in cards {
            if let Some(col) = self.columns.iter_mut().find(|c| c.list.id == card.id_list) {
                col.cards.push(card);
            }
        }
        for col in &mut self.columns {
            col.cards.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            let prev = prev_rows
                .iter()
                .find(|(id, _)| *id == col.list.id)
                .and_then(|p| p.1);
            col.state.select(prev);
        }
        for c in 0..self.columns.len() {
            self.clamp_selection(c);
        }

        self.col = prev_list
            .and_then(|id| self.columns.iter().position(|c| c.list.id == id))
            .unwrap_or(0);
        if let Some(id) = prev_card
            && let Some((c, r)) = self.find_card(&id)
        {
            self.col = c;
            self.columns[c].state.select(Some(r));
        }
    }

    // ---- board helpers --------------------------------------------------

    pub fn current_column(&self) -> Option<&Column> {
        self.columns.get(self.col)
    }

    pub fn row(&self) -> Option<usize> {
        self.current_column().and_then(|c| c.state.selected())
    }

    pub fn current_card(&self) -> Option<&Card> {
        let col = self.current_column()?;
        col.cards.get(col.state.selected()?)
    }

    pub fn find_card(&self, id: &str) -> Option<(usize, usize)> {
        self.columns.iter().enumerate().find_map(|(c, col)| {
            col.cards
                .iter()
                .position(|card| card.id == id)
                .map(|r| (c, r))
        })
    }

    /// Keeps a column's selection valid: `None` when empty, in range otherwise.
    pub fn clamp_selection(&mut self, c: usize) {
        let Some(col) = self.columns.get_mut(c) else {
            return;
        };
        let sel = if col.cards.is_empty() {
            None
        } else {
            Some(col.state.selected().unwrap_or(0).min(col.cards.len() - 1))
        };
        col.state.select(sel);
    }

    pub fn select_row(&mut self, row: usize) {
        let Some(col) = self.columns.get_mut(self.col) else {
            return;
        };
        if !col.cards.is_empty() {
            col.state.select(Some(row.min(col.cards.len() - 1)));
        }
    }

    /// Recomputes `pos` for the card at `(c, i)` from its neighbours and returns it.
    pub fn reposition(&mut self, c: usize, i: usize) -> f64 {
        let cards = &mut self.columns[c].cards;
        let prev = i.checked_sub(1).map(|p| cards[p].pos);
        let next = cards.get(i + 1).map(|n| n.pos);
        let pos = pos_between(prev, next);
        cards[i].pos = pos;
        pos
    }
}

/// A Trello `pos` that sorts between `prev` and `next`.
pub fn pos_between(prev: Option<f64>, next: Option<f64>) -> f64 {
    match (prev, next) {
        (Some(a), Some(b)) => (a + b) / 2.0,
        (Some(a), None) => a + POS_STEP,
        (None, Some(b)) => b / 2.0,
        (None, None) => POS_STEP,
    }
}

#[cfg(test)]
mod tests {
    use super::pos_between;

    #[test]
    fn positions() {
        assert_eq!(pos_between(None, None), 65536.0);
        assert_eq!(pos_between(Some(100.0), Some(200.0)), 150.0);
        assert_eq!(pos_between(Some(100.0), None), 65636.0);
        assert_eq!(pos_between(None, Some(100.0)), 50.0);
        let p = pos_between(Some(1.0), Some(1.5));
        assert!(p > 1.0 && p < 1.5);
    }
}
