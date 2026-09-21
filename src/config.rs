use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    api_key: Option<String>,
    token: Option<String>,
    default_board: Option<String>,
    column_width: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub token: String,
    /// Board (by name, case-insensitive) to open on startup.
    pub default_board: Option<String>,
    pub column_width: u16,
}

pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("trello-tui").join("config.toml"))
}

/// Loads `~/.config/trello-tui/config.toml`; `TRELLO_API_KEY` / `TRELLO_TOKEN` override the file.
pub fn load() -> Result<Config> {
    let path = path();
    let file = match &path {
        Some(p) if p.exists() => {
            let raw =
                std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
            toml::from_str::<FileConfig>(&raw)
                .with_context(|| format!("parsing {}", p.display()))?
        }
        _ => FileConfig::default(),
    };

    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let api_key = env("TRELLO_API_KEY").or(file.api_key);
    let token = env("TRELLO_TOKEN").or(file.token);

    let (Some(api_key), Some(token)) = (api_key, token) else {
        let shown = path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.config/trello-tui/config.toml".into());
        bail!(
            "Missing Trello credentials.\n\n\
             1. Get an API key: create a Power-Up at https://trello.com/power-ups/admin\n   \
                and open its \"API key\" tab.\n\
             2. Get a token by visiting (replace YOUR_KEY):\n   \
                https://trello.com/1/authorize?expiration=never&scope=read,write&response_type=token&key=YOUR_KEY\n\
             3. Put them in {shown}:\n\n   \
                api_key = \"...\"\n   \
                token = \"...\"\n   \
                # default_board = \"My board\"   (optional)\n\n\
             Or set TRELLO_API_KEY and TRELLO_TOKEN."
        );
    };

    Ok(Config {
        api_key,
        token,
        default_board: file.default_board,
        column_width: file.column_width.unwrap_or(32).max(12),
    })
}
