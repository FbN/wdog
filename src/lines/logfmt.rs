//! Log line prefixes: `[2] ` for stdout, `[2!] ` for stderr, `[2.1] ` for a retry attempt.

use super::Stream;

pub fn label(step: usize, attempt: u32) -> String {
    if attempt == 0 {
        format!("{step}")
    } else {
        format!("{step}.{attempt}")
    }
}

pub fn format(label: &str, stream: Stream, text: &str) -> String {
    match stream {
        Stream::Out => format!("[{label}] {text}"),
        Stream::Err => format!("[{label}!] {text}"),
    }
}

/// Returns (label, stream, text) or None when the line has no prefix.
pub fn parse(line: &str) -> Option<(&str, Stream, &str)> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find("] ").or_else(|| {
        if rest.ends_with(']') {
            Some(rest.len() - 1)
        } else {
            None
        }
    })?;
    let tag = &rest[..close];
    let text = rest.get(close + 2..).unwrap_or("");
    let (lab, stream) = match tag.strip_suffix('!') {
        Some(l) => (l, Stream::Err),
        None => (tag, Stream::Out),
    };
    if lab.is_empty() || !lab.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some((lab, stream, text))
}

/// The raw text of a log line (prefix removed when present).
pub fn raw(line: &str) -> &str {
    parse(line).map(|(_, _, t)| t).unwrap_or(line)
}

#[cfg(test)]
#[path = "logfmt_tests.rs"]
mod tests;
