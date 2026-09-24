//! Non-interactive subcommands (`trello-tui add ...`), for scripts and agents.

use std::io::Read;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::api::TrelloClient;
use crate::config::Config;
use crate::model::{Board, Card, List};

#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "Run without a subcommand to start the TUI."
)]
pub struct Cli {
    /// Print boards and exit (same as `boards`).
    #[arg(long, hide = true)]
    pub boards: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Args)]
pub struct Output {
    /// Print JSON instead of tab-separated text.
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct BoardArg {
    /// Board id or name (defaults to `default_board` from the config).
    #[arg(long, short)]
    board: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List open boards.
    Boards {
        #[command(flatten)]
        out: Output,
    },
    /// List the lists on a board.
    Lists {
        #[command(flatten)]
        board: BoardArg,
        #[command(flatten)]
        out: Output,
    },
    /// List the cards on a board.
    Cards {
        #[command(flatten)]
        board: BoardArg,
        /// Only cards in this list (id or name).
        #[arg(long, short)]
        list: Option<String>,
        #[command(flatten)]
        out: Output,
    },
    /// Show one card.
    Show {
        /// Card id, short link or URL.
        card: String,
        #[command(flatten)]
        out: Output,
    },
    /// Add a card and print its id and URL.
    Add {
        /// Card title.
        name: String,
        #[command(flatten)]
        board: BoardArg,
        /// List id or name (defaults to the board's first list).
        #[arg(long, short)]
        list: Option<String>,
        /// Description; `-` reads it from stdin.
        #[arg(long, short)]
        desc: Option<String>,
        /// Put the card at the top of the list instead of the bottom.
        #[arg(long)]
        top: bool,
        #[command(flatten)]
        out: Output,
    },
    /// Change a card's name or description.
    Edit {
        /// Card id, short link or URL.
        card: String,
        #[arg(long, short)]
        name: Option<String>,
        /// Description; `-` reads it from stdin.
        #[arg(long, short)]
        desc: Option<String>,
        #[command(flatten)]
        out: Output,
    },
    /// Move a card to another list on its board.
    Move {
        /// Card id, short link or URL.
        card: String,
        /// Target list id or name.
        #[arg(long, short)]
        list: String,
        /// Put the card at the top of the list instead of the bottom.
        #[arg(long)]
        top: bool,
        #[command(flatten)]
        out: Output,
    },
    /// Archive a card.
    Archive {
        /// Card id, short link or URL.
        card: String,
    },
    /// Add a comment to a card.
    Comment {
        /// Card id, short link or URL.
        card: String,
        /// Comment text; `-` reads it from stdin.
        text: String,
    },
}

impl Command {
    pub fn boards() -> Self {
        Command::Boards {
            out: Output { json: false },
        }
    }
}

pub async fn run(command: Command, config: Config) -> Result<()> {
    let client = TrelloClient::new(config.api_key, config.token);
    let default_board = config.default_board;
    let board = |arg: BoardArg| {
        let client = &client;
        let default_board = default_board.clone();
        async move {
            let Some(query) = arg.board.or(default_board) else {
                bail!("no board given: pass --board or set default_board in the config");
            };
            let boards = client.boards().await?;
            let b = find(&boards, "board", &query, |b| (&b.id, &b.name))?;
            Ok::<Board, anyhow::Error>(b.clone())
        }
    };

    match command {
        Command::Boards { out } => {
            let boards = client.boards().await?;
            print_rows(out, &boards, |b| vec![&b.id, &b.name])
        }
        Command::Lists { board: arg, out } => {
            let b = board(arg).await?;
            let lists = sorted_lists(&client, &b.id).await?;
            print_rows(out, &lists, |l| vec![&l.id, &l.name])
        }
        Command::Cards {
            board: arg,
            list,
            out,
        } => {
            let b = board(arg).await?;
            let (lists, mut cards) =
                tokio::try_join!(sorted_lists(&client, &b.id), client.cards(&b.id))?;
            if let Some(q) = list {
                let l = find(&lists, "list", &q, |l| (&l.id, &l.name))?;
                cards.retain(|c| c.id_list == l.id);
            }
            // Board order: by list, then by position within the list.
            let list_index = |c: &Card| lists.iter().position(|l| l.id == c.id_list);
            cards.sort_by(|a, b| {
                (list_index(a), a.pos)
                    .partial_cmp(&(list_index(b), b.pos))
                    .unwrap()
            });
            let rows: Vec<CardRow> = cards
                .into_iter()
                .map(|card| CardRow {
                    list: lists
                        .iter()
                        .find(|l| l.id == card.id_list)
                        .map(|l| l.name.clone())
                        .unwrap_or_default(),
                    card,
                })
                .collect();
            print_rows(out, &rows, |r| vec![&r.card.id, &r.list, &r.card.name])
        }
        Command::Show { card, out } => {
            let card = client.card(&card_ref(&card)).await?;
            if out.json {
                return print_json(&card);
            }
            println!(
                "id\t{}\nurl\t{}\nlist\t{}",
                card.id, card.short_url, card.id_list
            );
            println!("name\t{}", card.name);
            if let Some(due) = &card.due {
                println!(
                    "due\t{due}{}",
                    if card.due_complete { " (done)" } else { "" }
                );
            }
            let labels: Vec<_> = card.labels.iter().map(|l| l.name.as_str()).collect();
            if !labels.is_empty() {
                println!("labels\t{}", labels.join(", "));
            }
            if !card.desc.is_empty() {
                println!("\n{}", card.desc);
            }
            Ok(())
        }
        Command::Add {
            name,
            board: arg,
            list,
            desc,
            top,
            out,
        } => {
            let b = board(arg).await?;
            let lists = sorted_lists(&client, &b.id).await?;
            let l = match &list {
                Some(q) => find(&lists, "list", q, |l| (&l.id, &l.name))?,
                None => lists
                    .first()
                    .with_context(|| format!("board '{}' has no lists", b.name))?,
            };
            let desc = desc.map(read_arg).transpose()?.unwrap_or_default();
            let pos = if top { "top" } else { "bottom" };
            let card = client
                .create_card_with(&[
                    ("idList", &l.id),
                    ("name", &name),
                    ("desc", &desc),
                    ("pos", pos),
                ])
                .await?;
            print_card(out, &card)
        }
        Command::Edit {
            card,
            name,
            desc,
            out,
        } => {
            let mut fields = Vec::new();
            if let Some(name) = name {
                fields.push(("name", name));
            }
            if let Some(desc) = desc {
                fields.push(("desc", read_arg(desc)?));
            }
            if fields.is_empty() {
                bail!("nothing to change: pass --name and/or --desc");
            }
            let card = client.update_card(&card_ref(&card), &fields).await?;
            print_card(out, &card)
        }
        Command::Move {
            card,
            list,
            top,
            out,
        } => {
            let card = client.card(&card_ref(&card)).await?;
            let lists = sorted_lists(&client, &card.id_board).await?;
            let l = find(&lists, "list", &list, |l| (&l.id, &l.name))?;
            let pos = if top { "top" } else { "bottom" };
            let card = client
                .update_card(
                    &card.id,
                    &[("idList", l.id.clone()), ("pos", pos.to_string())],
                )
                .await?;
            print_card(out, &card)
        }
        Command::Archive { card } => {
            let card = client
                .update_card(&card_ref(&card), &[("closed", "true".into())])
                .await?;
            println!("{}", card.id);
            Ok(())
        }
        Command::Comment { card, text } => {
            client.add_comment(&card_ref(&card), &read_arg(text)?).await
        }
    }
}

#[derive(Serialize)]
struct CardRow {
    #[serde(flatten)]
    card: Card,
    list: String,
}

async fn sorted_lists(client: &TrelloClient, board_id: &str) -> Result<Vec<List>> {
    let mut lists = client.lists(board_id).await?;
    lists.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    Ok(lists)
}

/// Finds an item by exact id, then exact name (case-insensitive), then a unique name substring.
fn find<'a, T>(
    items: &'a [T],
    what: &str,
    query: &str,
    key: impl Fn(&T) -> (&String, &String),
) -> Result<&'a T> {
    if let Some(item) = items.iter().find(|i| key(i).0 == query) {
        return Ok(item);
    }
    let q = query.to_lowercase();
    if let Some(item) = items.iter().find(|i| key(i).1.to_lowercase() == q) {
        return Ok(item);
    }
    let matches: Vec<&T> = items
        .iter()
        .filter(|i| key(i).1.to_lowercase().contains(&q))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => bail!("no {what} matches '{query}'"),
        many => {
            let names: Vec<_> = many.iter().map(|i| key(i).1.as_str()).collect();
            bail!("'{query}' matches several {what}s: {}", names.join(", "))
        }
    }
}

/// Accepts a card id, short link, or `https://trello.com/c/<shortLink>/...` URL.
fn card_ref(arg: &str) -> String {
    match arg.split_once("/c/") {
        Some((_, rest)) => rest.split('/').next().unwrap_or(rest).to_string(),
        None => arg.to_string(),
    }
}

/// `-` means "read from stdin".
fn read_arg(arg: String) -> Result<String> {
    if arg != "-" {
        return Ok(arg);
    }
    let mut s = String::new();
    std::io::stdin()
        .read_to_string(&mut s)
        .context("reading stdin")?;
    Ok(s.trim_end_matches('\n').to_string())
}

fn print_json<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_rows<T: Serialize>(
    out: Output,
    items: &[T],
    cols: impl Fn(&T) -> Vec<&String>,
) -> Result<()> {
    if out.json {
        return print_json(items);
    }
    for item in items {
        let cols: Vec<&str> = cols(item).into_iter().map(String::as_str).collect();
        println!("{}", cols.join("\t"));
    }
    Ok(())
}

fn print_card(out: Output, card: &Card) -> Result<()> {
    if out.json {
        return print_json(card);
    }
    println!("{}\t{}", card.id, card.short_url);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(id: &str, name: &str) -> List {
        List {
            id: id.into(),
            name: name.into(),
            pos: 0.0,
        }
    }

    #[test]
    fn find_lists() {
        let lists = [list("a1", "To Do"), list("b2", "Doing"), list("c3", "Done")];
        fn key(l: &List) -> (&String, &String) {
            (&l.id, &l.name)
        }
        assert_eq!(find(&lists, "list", "b2", key).unwrap().name, "Doing");
        assert_eq!(find(&lists, "list", "to do", key).unwrap().id, "a1");
        assert_eq!(find(&lists, "list", "doi", key).unwrap().id, "b2");
        // Every name contains "do".
        assert!(find(&lists, "list", "do", key).is_err());
        assert!(find(&lists, "list", "nope", key).is_err());
    }

    #[test]
    fn card_refs() {
        assert_eq!(card_ref("abc123"), "abc123");
        assert_eq!(card_ref("https://trello.com/c/XyZ12/34-some-card"), "XyZ12");
        assert_eq!(card_ref("https://trello.com/c/XyZ12"), "XyZ12");
    }
}
