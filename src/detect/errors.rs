//! Error patterns (AN-6, BG-5): every log line is matched; `ERROR_LIVE` fires while the main
//! process lives, at most once per 10 s window.

use regex::RegexSet;
use std::time::{Duration, Instant};

pub const LIVE_WINDOW: Duration = Duration::from_secs(10);
pub const SUMMARY_ITEMS: usize = 5;

#[derive(Debug, Clone)]
pub struct ErrorMatcher {
    set: RegexSet,
}

impl ErrorMatcher {
    pub fn new(patterns: &[String]) -> Result<ErrorMatcher, regex::Error> {
        Ok(ErrorMatcher {
            set: RegexSet::new(patterns)?,
        })
    }
    pub fn is_match(&self, text: &str) -> bool {
        !self.set.is_empty() && self.set.is_match(text)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Errors {
    pub count: usize,
    pub items: Vec<(u64, String)>,
    pub last_live: Option<Instant>,
}

/// A matching line. `live` = it came from the current step while its main process lives.
/// Returns true when `ERROR_LIVE` should fire.
pub fn on_match(mut e: Errors, line_no: u64, text: &str, live: bool, now: Instant) -> (Errors, bool) {
    e.count += 1;
    if e.items.len() < SUMMARY_ITEMS {
        e.items.push((line_no, text.to_string()));
    }
    let fire = live
        && e.last_live
            .is_none_or(|t| now.saturating_duration_since(t) >= LIVE_WINDOW);
    if fire {
        e.last_live = Some(now);
    }
    (e, fire)
}

/// `wdog errors`: scan log lines (raw text, 1-based line numbers).
pub fn scan<'a>(lines: impl Iterator<Item = &'a str>, m: &ErrorMatcher) -> Vec<(u64, String)> {
    lines
        .enumerate()
        .filter_map(|(i, l)| {
            let raw = crate::lines::logfmt::raw(l);
            m.is_match(raw).then(|| (i as u64 + 1, raw.to_string()))
        })
        .collect()
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
