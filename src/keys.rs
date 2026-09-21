//! Normal-mode key parser: counts (`5j`), two-key sequences (`gg`, `dd`, `cw`).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Left(usize),
    Right(usize),
    Down(usize),
    Up(usize),
    Top,
    /// `G` jumps to the last card, `5G` to the 5th.
    Goto(Option<usize>),
    HalfPageDown,
    HalfPageUp,
    Open,
    /// `Esc`: close / go back.
    Back,
    /// `q`: like `Back`, but quits from the board picker.
    Close,
    Quit,
    NewBelow,
    NewAbove,
    /// `r`: edit the card name.
    Rename,
    /// `cw` / `cc`: replace the card name.
    Change,
    EditDesc,
    Archive,
    Undo,
    MoveLeft,
    MoveRight,
    MoveDown(usize),
    MoveUp(usize),
    Search,
    NextMatch,
    PrevMatch,
    Command,
    Help,
    Refresh,
}

#[derive(Debug, Default)]
pub struct KeyParser {
    count: Option<usize>,
    pending: Option<char>,
}

impl KeyParser {
    pub fn reset(&mut self) {
        self.count = None;
        self.pending = None;
    }

    /// Keys typed so far that haven't resolved to an action (vim's `showcmd`).
    pub fn pending_display(&self) -> String {
        let mut s = self.count.map(|c| c.to_string()).unwrap_or_default();
        if let Some(p) = self.pending {
            s.push(p);
        }
        s
    }

    pub fn feed(&mut self, key: KeyEvent) -> Option<Action> {
        use Action::*;

        if key.code == KeyCode::Esc {
            let had_pending = self.count.is_some() || self.pending.is_some();
            self.reset();
            return (!had_pending).then_some(Back);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            self.reset();
            return match key.code {
                KeyCode::Char('d') => Some(HalfPageDown),
                KeyCode::Char('u') => Some(HalfPageUp),
                KeyCode::Char('c') => Some(Quit),
                KeyCode::Char('r') => Some(Refresh),
                _ => None,
            };
        }

        let c = match key.code {
            KeyCode::Char(c) => c,
            other => {
                let n = self.count.take().unwrap_or(1);
                self.pending = None;
                return match other {
                    KeyCode::Enter => Some(Open),
                    KeyCode::Left => Some(Left(n)),
                    KeyCode::Right => Some(Right(n)),
                    KeyCode::Down => Some(Down(n)),
                    KeyCode::Up => Some(Up(n)),
                    _ => None,
                };
            }
        };

        if let Some(p) = self.pending.take() {
            self.count = None;
            return match (p, c) {
                ('g', 'g') => Some(Top),
                ('d', 'd') => Some(Archive),
                ('c', 'w') | ('c', 'c') => Some(Change),
                _ => None,
            };
        }

        if let Some(d) = c.to_digit(10)
            && (d != 0 || self.count.is_some())
        {
            let n = self
                .count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(d as usize);
            self.count = Some(n.min(99_999));
            return None;
        }

        if matches!(c, 'g' | 'd' | 'c') {
            self.pending = Some(c);
            return None;
        }

        let count = self.count.take();
        let n = count.unwrap_or(1);
        Some(match c {
            'h' => Left(n),
            'l' => Right(n),
            'j' => Down(n),
            'k' => Up(n),
            'G' => Goto(count),
            'H' => MoveLeft,
            'L' => MoveRight,
            'J' => MoveDown(n),
            'K' => MoveUp(n),
            'o' => NewBelow,
            'O' => NewAbove,
            'r' => Rename,
            'e' => EditDesc,
            'u' => Undo,
            '/' => Search,
            'n' => NextMatch,
            'N' => PrevMatch,
            ':' => Command,
            '?' => Help,
            'q' => Close,
            'R' => Refresh,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;

    fn feed_str(p: &mut KeyParser, s: &str) -> Vec<Action> {
        s.chars()
            .filter_map(|c| p.feed(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)))
            .collect()
    }

    #[test]
    fn simple_motions() {
        let mut p = KeyParser::default();
        assert_eq!(
            feed_str(&mut p, "hjkl"),
            vec![Left(1), Down(1), Up(1), Right(1)]
        );
    }

    #[test]
    fn counts() {
        let mut p = KeyParser::default();
        assert_eq!(feed_str(&mut p, "5j12k"), vec![Down(5), Up(12)]);
        assert_eq!(feed_str(&mut p, "3J"), vec![MoveDown(3)]);
        assert_eq!(feed_str(&mut p, "G7G"), vec![Goto(None), Goto(Some(7))]);
    }

    #[test]
    fn zero_is_not_a_count_start() {
        let mut p = KeyParser::default();
        assert_eq!(feed_str(&mut p, "0"), vec![]);
        assert_eq!(p.pending_display(), "");
        assert_eq!(feed_str(&mut p, "10j"), vec![Down(10)]);
    }

    #[test]
    fn sequences() {
        let mut p = KeyParser::default();
        assert_eq!(feed_str(&mut p, "gg"), vec![Top]);
        assert_eq!(feed_str(&mut p, "dd"), vec![Archive]);
        assert_eq!(feed_str(&mut p, "cw"), vec![Change]);
        // Invalid second key cancels the sequence.
        assert_eq!(feed_str(&mut p, "gxj"), vec![Down(1)]);
    }

    #[test]
    fn pending_display_and_escape() {
        let mut p = KeyParser::default();
        feed_str(&mut p, "4d");
        assert_eq!(p.pending_display(), "4d");
        // Esc with something pending only clears it.
        assert_eq!(
            p.feed(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            None
        );
        assert_eq!(p.pending_display(), "");
        assert_eq!(
            p.feed(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Back)
        );
    }

    #[test]
    fn ctrl_keys() {
        let mut p = KeyParser::default();
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert_eq!(p.feed(ctrl('d')), Some(HalfPageDown));
        assert_eq!(p.feed(ctrl('c')), Some(Quit));
    }
}
