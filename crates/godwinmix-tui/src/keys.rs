//! Keys to actions, kept apart from what the actions do.
//!
//! The map is small and it is the whole of it: `docs/reference/keyboard.md` is
//! generated from nothing, it is written against this file, and the `?` sheet
//! shows the same list. The number keys match the web UI's map
//! (`ui/shell/keymap.js`), so an operator moving between the two surfaces does
//! not have to learn a second set.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Quit,
    NextPane,
    PrevPane,
    Up,
    Down,
    Top,
    Bottom,
    /// Take the nth source on screen, 1 to 9.
    TakeSlot(usize),
    /// Cut to the slate.
    TakeBlack,
    /// Take the selected source, for when there are more than nine.
    TakeSelected,
    Revert,
    Mute,
    /// Decibels, plus or minus one.
    Gain(f64),
    /// Start an ad break, or end the one that is running.
    AdBreak,
    /// Jump to the outputs pane.
    Outputs,
    /// Start or stop the selected output.
    StartStop,
    FilterStart,
    Help,
    /// A character typed into the filter or the ad break prompt.
    Type(char),
    Backspace,
    Accept,
    Cancel,
    /// Nothing this UI does anything with.
    None,
}

/// What the key means while the screen is taking commands.
pub fn normal(key: KeyEvent) -> Action {
    if key.kind == KeyEventKind::Release {
        return Action::None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Action::Quit,
            _ => Action::None,
        };
    }
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        // Escape clears the filter and closes the help. It does not quit: a
        // mixer that goes off the screen because somebody leant on Escape is
        // a mixer nobody trusts.
        KeyCode::Esc => Action::Cancel,
        KeyCode::Tab => Action::NextPane,
        KeyCode::BackTab => Action::PrevPane,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Home => Action::Top,
        KeyCode::End => Action::Bottom,
        KeyCode::Char('0') => Action::TakeBlack,
        KeyCode::Char(c @ '1'..='9') => Action::TakeSlot(c as usize - '0' as usize),
        KeyCode::Enter => Action::TakeSelected,
        KeyCode::Char('r') => Action::Revert,
        KeyCode::Char('m') => Action::Mute,
        KeyCode::Char('+') | KeyCode::Char('=') => Action::Gain(1.0),
        KeyCode::Char('-') | KeyCode::Char('_') => Action::Gain(-1.0),
        KeyCode::Char('a') => Action::AdBreak,
        KeyCode::Char('o') => Action::Outputs,
        KeyCode::Char('s') => Action::StartStop,
        KeyCode::Char('/') => Action::FilterStart,
        KeyCode::Char('?') => Action::Help,
        _ => Action::None,
    }
}

/// What the key means while a line is being typed: the filter, or the clip for
/// an ad break.
pub fn typing(key: KeyEvent) -> Action {
    if key.kind == KeyEventKind::Release {
        return Action::None;
    }
    match key.code {
        KeyCode::Esc => Action::Cancel,
        KeyCode::Enter => Action::Accept,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' => {
            Action::Cancel
        }
        KeyCode::Char(c) => Action::Type(c),
        _ => Action::None,
    }
}

/// Every key the `?` sheet and the reference page list, in one table so the
/// two cannot disagree with the code.
pub const HELP: &[(&str, &str)] = &[
    ("1 to 9", "take the nth source on screen"),
    ("0", "cut to black"),
    ("Enter", "take the selected source"),
    ("r", "revert to the shot before this one"),
    ("m", "mute or unmute the selected source"),
    ("+ / -", "the selected source's fader, one decibel a step"),
    ("a", "start an ad break, or end the one running"),
    ("o", "go to the outputs pane"),
    ("s", "start or stop the selected output"),
    ("/", "filter the list in the focused pane"),
    ("Tab", "move between sources, outputs and alerts"),
    ("up / down", "move the selection (k and j do the same)"),
    ("?", "this help"),
    ("q", "quit"),
];
