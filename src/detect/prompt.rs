//! Waiting for input (AN-4): a question-like unterminated line, or a member blocked reading a
//! terminal, for the grace period.

use std::time::{Duration, Instant};

const KEYWORDS: [&str; 16] = [
    "y/n",
    "yes/no",
    "(y)",
    "[y]",
    "password",
    "passphrase",
    "continue",
    "proceed",
    "are you sure",
    "press any key",
    "press enter",
    "enter ",
    "overwrite",
    "username",
    "login:",
    "confirm",
];

pub fn looks_like_prompt(partial: &str) -> bool {
    let t = partial.trim_end();
    if t.is_empty() || t.chars().count() > 200 {
        return false;
    }
    let l = t.to_lowercase();
    if l.contains('%') || l.contains("eta ") || l.ends_with("eta") {
        return false;
    }
    if !t.ends_with(['?', ':', '>', ']', ')']) {
        return false;
    }
    t.ends_with('?') || t.ends_with(">>>") || KEYWORDS.iter().any(|k| l.contains(k))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Prompt {
    pub fired: bool,
    pub tty_since: Option<Instant>,
}

/// Text path: `partial` is the unterminated tail and when it last changed.
pub fn check_text(
    p: Prompt,
    partial: Option<(&str, Instant)>,
    grace: Duration,
    now: Instant,
) -> (Prompt, bool) {
    if p.fired {
        return (p, false);
    }
    match partial {
        Some((text, since)) if now.saturating_duration_since(since) >= grace && looks_like_prompt(text) => {
            (Prompt { fired: true, ..p }, true)
        }
        _ => (p, false),
    }
}

/// TTY path: some member blocked on a terminal read, with no output, for the grace period.
pub fn check_tty(
    p: Prompt,
    blocked: bool,
    last_output: Instant,
    grace: Duration,
    now: Instant,
) -> (Prompt, bool) {
    if !blocked {
        return (Prompt { tty_since: None, ..p }, false);
    }
    let since = p.tty_since.unwrap_or(now);
    let p = Prompt {
        tty_since: Some(since),
        ..p
    };
    let quiet_since = since.max(last_output);
    if !p.fired && now.saturating_duration_since(quiet_since) >= grace {
        (Prompt { fired: true, ..p }, true)
    } else {
        (p, false)
    }
}

/// New output re-arms the detector.
pub fn rearm(_p: Prompt) -> Prompt {
    Prompt::default()
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
