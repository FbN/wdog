//! The monitor's state and what each key does. Pure: loading is done by the caller.

use super::data::{Detail, Log};
use crate::job::status::Status;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

/// Lines `Home` loads at most when jumping to the top of a log.
pub const MAX_LINES: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Output,
    Steps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    Right,
    Toggle,
}

/// What the caller must do after an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
    /// Another job was selected: load its log and steps from scratch.
    Reload,
    /// More log lines are needed above the loaded ones.
    More,
}

pub fn action(code: KeyCode, mods: KeyModifiers) -> Option<Action> {
    Some(match code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home | KeyCode::Char('g') => Action::Home,
        KeyCode::End | KeyCode::Char('G') => Action::End,
        KeyCode::Left => Action::Left,
        KeyCode::Right => Action::Right,
        KeyCode::Char('o') => Action::Toggle,
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub struct App {
    pub jobs: Vec<Status>,
    pub sel: usize,
    pub pane: Pane,
    /// Output: lines up from the newest (0 = follow). Steps: lines down from the top.
    pub scroll: usize,
    /// Characters hidden on the left of the output.
    pub pan: usize,
    pub log: Log,
    pub steps: Vec<Detail>,
    /// Height of the right pane's text area, set when drawing.
    pub page: usize,
    pub now: f64,
}

impl Default for App {
    fn default() -> App {
        App {
            jobs: vec![],
            sel: 0,
            pane: Pane::Output,
            scroll: 0,
            pan: 0,
            log: Log::default(),
            steps: vec![],
            page: 40,
            now: 0.0,
        }
    }
}

impl App {
    pub fn selected(&self) -> Option<&Status> {
        self.jobs.get(self.sel)
    }

    /// The output follows new lines (not scrolled up).
    pub fn following(&self) -> bool {
        self.scroll == 0
    }

    /// Log lines to load: what is shown, plus a page above it.
    pub fn want_lines(&self) -> usize {
        self.scroll.saturating_add(self.page.max(1) * 2).min(MAX_LINES)
    }

    /// Replace the job list, keeping the selected job. Returns true when the
    /// selected job changed (it was removed, or there was none).
    pub fn set_jobs(&mut self, jobs: Vec<Status>) -> bool {
        let old = self.selected().map(|s| s.id.clone());
        self.jobs = jobs;
        let pos = old
            .as_ref()
            .and_then(|id| self.jobs.iter().position(|s| &s.id == id));
        self.sel = pos.unwrap_or(0).min(self.jobs.len().saturating_sub(1));
        let changed = pos.is_none();
        if changed {
            self.scroll = 0;
            self.pan = 0;
        }
        changed
    }

    pub fn set_log(&mut self, log: Log) {
        self.log = log;
        self.clamp();
    }

    pub fn set_steps(&mut self, steps: Vec<Detail>) {
        self.steps = steps;
        self.clamp();
    }

    /// Keep the scroll inside what exists.
    fn clamp(&mut self) {
        let page = self.page.max(1);
        match self.pane {
            Pane::Output if self.log.at_start => {
                self.scroll = self.scroll.min(self.log.lines.len().saturating_sub(page));
            }
            Pane::Output => {}
            Pane::Steps => self.scroll = self.scroll.min(self.steps.len().saturating_sub(page)),
        }
    }

    pub fn apply(&mut self, a: Action) -> Effect {
        let page = self.page.max(1);
        match a {
            Action::Quit => return Effect::Quit,
            Action::Up | Action::Down => {
                let next = match a {
                    Action::Up => self.sel.saturating_sub(1),
                    _ => (self.sel + 1).min(self.jobs.len().saturating_sub(1)),
                };
                if next == self.sel {
                    return Effect::None;
                }
                self.sel = next;
                self.scroll = 0;
                self.pan = 0;
                return Effect::Reload;
            }
            Action::Left => self.pan = self.pan.saturating_sub(8),
            Action::Right => self.pan += 8,
            Action::Toggle => {
                self.pane = match self.pane {
                    Pane::Output => Pane::Steps,
                    Pane::Steps => Pane::Output,
                };
                self.scroll = 0;
                return Effect::Reload;
            }
            Action::PageUp | Action::Home => {
                let up = if a == Action::Home { MAX_LINES } else { page };
                match self.pane {
                    Pane::Output => {
                        self.scroll = self.scroll.saturating_add(up).min(MAX_LINES);
                        self.clamp();
                        if self.log.lines.len() < self.scroll + page && !self.log.at_start {
                            return Effect::More;
                        }
                    }
                    Pane::Steps => self.scroll = self.scroll.saturating_sub(up),
                }
            }
            Action::PageDown | Action::End => {
                let down = if a == Action::End { usize::MAX } else { page };
                match self.pane {
                    Pane::Output => {
                        let was = self.scroll;
                        self.scroll = self.scroll.saturating_sub(down);
                        if was > 0 && self.scroll == 0 {
                            // back to following: read up to the current end
                            return Effect::More;
                        }
                    }
                    Pane::Steps => {
                        self.scroll = self.scroll.saturating_add(down);
                        self.clamp();
                    }
                }
            }
        }
        Effect::None
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
