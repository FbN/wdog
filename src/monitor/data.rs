//! What the monitor reads from WDOG_HOME: job statuses, the log tail, the step list.

use crate::cmd::status::{StepState, effective, step_line, step_rows};
use crate::event::{Event, parse_json};
use crate::job::dir::{JobPaths, list_jobs};
use crate::job::status::{self, Status};
use crate::job::tailer::lines_before;
use crate::lines::{Stream, logfmt};
use crate::spec::RunSpec;
use std::path::Path;

/// One log line, split into the step tag and the cleaned text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LogLine {
    /// `2`, `2.1` (retry), or empty for a line without a prefix
    pub tag: String,
    pub err: bool,
    pub text: String,
}

/// The loaded end of a log: `lines` are the last lines before byte `end`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Log {
    pub lines: Vec<LogLine>,
    pub end: u64,
    /// `lines` begin at the start of the file
    pub at_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Step(StepState),
    Code,
}

/// One line of the steps pane.
#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub kind: Kind,
    pub text: String,
}

/// Every job, newest start first; a live job whose watcher is gone shows as `lost`.
pub fn jobs(home: &Path) -> Vec<Status> {
    let mut v: Vec<Status> = list_jobs(home)
        .into_iter()
        .filter_map(|(id, _)| status::read(&JobPaths::new(home, &id).status).ok())
        .map(effective)
        .collect();
    sort_jobs(&mut v);
    v
}

/// Pure: by start time, newest first. Unlike the status file's mtime, this does
/// not change while a job runs, so rows stay where they are.
pub fn sort_jobs(v: &mut [Status]) {
    v.sort_by(|a, b| {
        b.started_at
            .total_cmp(&a.started_at)
            .then_with(|| a.id.cmp(&b.id))
    });
}

pub fn log_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// The last `n` lines before byte `end`.
pub fn read_log(path: &Path, end: u64, n: usize) -> Log {
    let (lines, at_start) = lines_before(path, end, n).unwrap_or((vec![], true));
    Log {
        lines: lines.iter().map(|l| log_line(l)).collect(),
        end: end.min(log_len(path)),
        at_start,
    }
}

/// Pure: split the `[n] ` / `[n!] ` prefix off a log line and clean the text.
pub fn log_line(l: &str) -> LogLine {
    match logfmt::parse(l) {
        Some((tag, stream, text)) => LogLine {
            tag: tag.to_string(),
            err: stream == Stream::Err,
            text: clean(text),
        },
        None => LogLine {
            text: clean(l),
            ..LogLine::default()
        },
    }
}

/// Pure: what a terminal would show for one line, without anything that could
/// move the cursor or change colours: escape sequences and control characters
/// are dropped, tabs become spaces, and only the text after the last `\r` is kept.
pub fn clean(s: &str) -> String {
    let s = s.rsplit('\r').find(|p| !p.is_empty()).unwrap_or("");
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.next() {
                // CSI: parameters, then one final byte in @..~
                Some('[') => {
                    for c in it.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or ESC \
                Some(']') => {
                    while let Some(c) = it.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' {
                            it.next_if_eq(&'\\');
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\t' => {
                let col = out.chars().count();
                out.extend(std::iter::repeat_n(' ', 8 - col % 8));
            }
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// The steps pane: working dir, env files, then each step's state and script.
pub fn steps(p: &JobPaths, st: &Status) -> Vec<Detail> {
    let Some(spec) = std::fs::read(&p.spec)
        .ok()
        .and_then(|d| serde_json::from_slice::<RunSpec>(&d).ok())
    else {
        return vec![info(format!("no spec.json for job {}", st.id))];
    };
    let text = std::fs::read_to_string(&p.events).unwrap_or_default();
    let events: Vec<Event> = text.lines().filter_map(parse_json).collect();
    step_details(&spec, &events, st)
}

/// Pure: the steps pane from the spec, the events and the status.
pub fn step_details(spec: &RunSpec, events: &[Event], st: &Status) -> Vec<Detail> {
    let mut out = vec![];
    if let Some(cwd) = &spec.cwd {
        out.push(info(format!("cwd: {}", cwd.display())));
    }
    for f in &spec.env_files {
        out.push(info(format!("env-file: {}", f.display())));
    }
    if !out.is_empty() {
        out.push(info(String::new()));
    }
    let names: Vec<String> = (0..spec.steps.len()).map(|i| spec.step_name(i)).collect();
    let total = spec.steps.len();
    for (r, script) in step_rows(&names, events, st).iter().zip(&spec.steps) {
        out.push(Detail {
            kind: Kind::Step(r.state),
            text: step_line(r, total).trim_start().to_string(),
        });
        // `--steps-file` adds this line to every step
        let script = script.strip_prefix("set -e\n").unwrap_or(script);
        for l in script.lines() {
            out.push(Detail {
                kind: Kind::Code,
                text: format!("    {}", clean(l)),
            });
        }
    }
    out
}

fn info(text: String) -> Detail {
    Detail {
        kind: Kind::Info,
        text,
    }
}

#[cfg(test)]
#[path = "data_tests.rs"]
mod tests;
