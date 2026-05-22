use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nix::sys::signal::Signal;

use crate::app::SortBy;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Quit,
    Reset,
    SelectNext,
    SelectPrev,
    RequestKill(Signal),
    ConfirmKill,
    CancelKill,
    SortBy(SortBy),
    CycleViewSize,
}

pub fn map_key(k: KeyEvent, kill_pending: bool) -> Option<Action> {
    if k.kind != KeyEventKind::Press {
        return None;
    }
    if kill_pending {
        return Some(match k.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Action::ConfirmKill,
            _ => Action::CancelKill,
        });
    }
    match k.code {
        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('r') => Some(Action::Reset),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::SelectNext),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::SelectPrev),
        KeyCode::Char('d') => Some(Action::RequestKill(Signal::SIGTERM)),
        KeyCode::Char('D') => Some(Action::RequestKill(Signal::SIGKILL)),
        KeyCode::Char('c') => Some(Action::SortBy(SortBy::Cpu)),
        KeyCode::Char('m') => Some(Action::SortBy(SortBy::Memory)),
        KeyCode::Char('v') => Some(Action::CycleViewSize),
        _ => None,
    }
}
