mod actions;
mod api;
mod app;
mod cli;
mod config;
mod keys;
mod model;
mod ui;

use anyhow::Result;
use clap::Parser;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::app::{App, Msg};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let config = match config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    };

    // Subcommands (and the older `--boards`) run once without starting the UI.
    let command = match cli.command {
        None if cli.boards => Some(cli::Command::boards()),
        c => c,
    };
    if let Some(command) = command {
        if let Err(e) = cli::run(command, config).await {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let (tx, mut rx) = unbounded_channel();
    let mut app = App::new(config, tx.clone());
    // Installs a panic hook that restores the terminal.
    let mut terminal = ratatui::init();

    std::thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            if tx.send(Msg::Input(ev)).is_err() {
                break;
            }
        }
    });

    app.load_boards();
    let result = run(&mut terminal, &mut app, &mut rx).await;
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &mut UnboundedReceiver<Msg>,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        let Some(msg) = rx.recv().await else { break };
        app.handle(msg);
        while let Ok(msg) = rx.try_recv() {
            app.handle(msg);
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
