//! Parse `/proc/<pid>/stat`.

use super::ParseError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatFields {
    pub pid: i32,
    pub comm: String,
    pub state: char,
    pub ppid: i32,
    pub pgrp: i32,
    pub utime: u64,
    pub stime: u64,
    pub starttime: u64,
}

/// `comm` sits in parentheses and may contain spaces and `)`: split on the last `)`.
pub fn parse_stat(s: &str) -> Result<StatFields, ParseError> {
    let err = |m: &str| ParseError(format!("stat: {m}"));
    let open = s.find('(').ok_or_else(|| err("no ("))?;
    let close = s.rfind(')').ok_or_else(|| err("no )"))?;
    if close < open {
        return Err(err("bad parens"));
    }
    let pid = s[..open].trim().parse().map_err(|_| err("pid"))?;
    let comm = s[open + 1..close].to_string();
    // fields after comm start at field 3 (state)
    let rest: Vec<&str> = s[close + 1..].split_whitespace().collect();
    if rest.len() < 20 {
        return Err(err("truncated"));
    }
    let num = |i: usize, name: &str| -> Result<u64, ParseError> { rest[i].parse().map_err(|_| err(name)) };
    let inum = |i: usize, name: &str| -> Result<i32, ParseError> { rest[i].parse().map_err(|_| err(name)) };
    let state = rest[0].chars().next().ok_or_else(|| err("state"))?;
    Ok(StatFields {
        pid,
        comm,
        state,
        ppid: inum(1, "ppid")?,
        pgrp: inum(2, "pgrp")?,
        utime: num(11, "utime")?,
        stime: num(12, "stime")?,
        starttime: num(19, "starttime")?,
    })
}

#[cfg(test)]
#[path = "stat_tests.rs"]
mod tests;
