//! Parse `/proc/<pid>/io`: keep `rchar` and `wchar` (files and sockets, AN-2).

use super::{IoCounters, ParseError};

pub fn parse_io(s: &str) -> Result<IoCounters, ParseError> {
    let mut r = None;
    let mut w = None;
    for line in s.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim().parse::<u64>().ok();
        match k.trim() {
            "rchar" => r = v,
            "wchar" => w = v,
            _ => {}
        }
    }
    match (r, w) {
        (Some(rchar), Some(wchar)) => Ok(IoCounters { rchar, wchar }),
        _ => Err(ParseError("io: missing rchar/wchar".into())),
    }
}

#[cfg(test)]
#[path = "io_tests.rs"]
mod tests;
