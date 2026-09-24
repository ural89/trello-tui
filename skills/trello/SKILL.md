---
name: trello
description: Read and update the user's Trello boards from the shell with the `trello-tui` CLI - list boards, lists and cards, add cards (e.g. to track a task or bug), edit, move between lists, comment on and archive cards. Use when the user mentions Trello, a board, or asks to add/track/move a task or card.
---

# Trello via `trello-tui`

`trello-tui <subcommand>` runs once and exits. Never run `trello-tui` without a subcommand:
that starts an interactive TUI that will hang your shell.

## Commands

```sh
trello-tui boards                                  # id  name
trello-tui lists  -b <board>                       # id  name   (in board order)
trello-tui cards  -b <board> [-l <list>]           # id  list  name
trello-tui show   <card>                           # fields, then description
trello-tui add    "<title>" -b <board> [-l <list>] [-d "<desc>"] [--top]   # prints: id  url
trello-tui edit   <card> [-n "<new title>"] [-d "<new desc>"]
trello-tui move   <card> -l <list> [--top]         # list on the card's own board
trello-tui comment <card> "<text>"
trello-tui archive <card>
```

Add `--json` to `boards`, `lists`, `cards`, `show`, `add`, `edit` and `move` for machine-readable
output (card fields: `id`, `name`, `desc`, `idList`, `idBoard`, `pos`, `labels`, `due`,
`dueComplete`, `shortUrl`; `cards --json` adds `list`, the list's name).

## Arguments

- `<board>` / `<list>`: an id, the exact name (case-insensitive), or a part of the name that
  matches only one. If it matches several, the command fails and names them: retry with the
  full name. `-b` can be omitted when the user has set `default_board` in the config.
- `<card>`: a card id, short link, or `https://trello.com/c/...` URL. Keep the id or URL
  printed by `add` if you'll touch the card again.
- `add` without `-l` puts the card in the board's first list. New cards go to the bottom
  unless you pass `--top`.
- `-d -` (and `comment <card> -`) reads the text from stdin. Use it for multi-line
  descriptions or anything with quotes:

  ```sh
  trello-tui add "Fix login redirect" -b Work -l "To Do" -d - <<'EOF'
  Users land on /404 after SSO.
  Repro: log in from /settings.
  EOF
  ```

## Behaviour to know

- Exit code 0 on success. Errors go to stderr with a non-zero exit code. Check it before
  reporting success.
- Descriptions are Markdown. `edit -d` replaces the whole description. To append, `show` it
  first and send the combined text.
- `archive` is reversible from Trello's UI. There is no delete command: don't try to delete
  cards another way.
- Discover names first (`boards`, then `lists -b <board>`) instead of guessing list names like
  "Done" or "In Progress".
- Ask the user before archiving or moving cards you didn't create in this session.

## Setup (if a command reports missing credentials)

Credentials come from `~/.config/trello-tui/config.toml` (`api_key`, `token`, optional
`default_board`) or the `TRELLO_API_KEY` / `TRELLO_TOKEN` env vars. Don't ask the user to
paste the token into the chat: point them to the file.
