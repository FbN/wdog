//! `[wdog]` events: one enum, rendered as text lines or JSON objects.

use crate::duration::fmt_secs;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PREFIX: &str = "[wdog]";
const MAX_TEXT: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoneReason {
    Ok,
    StepFailed,
    Max,
    KillOnStall,
    Stopped,
}

impl DoneReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DoneReason::Ok => "ok",
            DoneReason::StepFailed => "step_failed",
            DoneReason::Max => "max",
            DoneReason::KillOnStall => "kill_on_stall",
            DoneReason::Stopped => "stopped",
        }
    }
}

/// Durations are seconds (`f64`); `cpu` is CPU seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Event {
    Start {
        id: String,
        log: String,
    },
    Env {
        file: String,
        loaded: usize,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keys: Vec<String>,
    },
    Step {
        n: usize,
        total: usize,
        ok: bool,
        exit: i32,
        dur: f64,
        cmd: String,
    },
    Stall {
        after: f64,
        cpu: f64,
        io: Option<u64>,
        pids: usize,
        unknown: usize,
        last: String,
    },
    Resumed {
        after: f64,
    },
    Prompt {
        line: String,
        via: String,
    },
    Orphans {
        pids: Vec<(i32, String)>,
    },
    ErrorLive {
        log_line: u64,
        text: String,
    },
    Busy {
        after: f64,
        cpu: f64,
    },
    Errors {
        count: usize,
        items: Vec<(u64, String)>,
    },
    Tail {
        n: usize,
        lines: Vec<String>,
    },
    Head {
        n: usize,
        lines: Vec<String>,
    },
    Skipped {
        lines: u64,
    },
    Timeout {
        after: f64,
    },
    Ready {
        what: String,
    },
    Stopped {
        signal: String,
    },
    Retry {
        n: u32,
        max: u32,
        exit: i32,
    },
    Lost {
        killed: usize,
    },
    LogFull {
        bytes: u64,
    },
    Done {
        exit: i32,
        reason: DoneReason,
        dur: f64,
    },
    Error {
        msg: String,
    },
}

impl Event {
    /// Alerts end `wdog wait` early (phase 6).
    pub fn is_alert(&self) -> bool {
        matches!(
            self,
            Event::Stall { .. }
                | Event::Prompt { .. }
                | Event::Orphans { .. }
                | Event::ErrorLive { .. }
                | Event::Busy { .. }
        )
    }
}

/// Truncate to 120 chars with `…`.
pub fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_TEXT {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(MAX_TEXT - 1).collect();
        out.push('…');
        out
    }
}

/// `"text"` with `"` and `\` escaped, clipped.
pub fn quote(s: &str) -> String {
    let c = clip(s);
    format!("\"{}\"", c.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn fmt_bytes(b: u64) -> String {
    const K: f64 = 1024.0;
    let f = b as f64;
    if b < 1024 {
        format!("{b}B")
    } else if f < K * K {
        format!("{:.0}KiB", f / K)
    } else if f < K * K * K {
        format!("{:.0}MiB", f / K / K)
    } else {
        format!("{:.1}GiB", f / K / K / K)
    }
}

/// Text form. Multi-line for `TAIL`/`HEAD` (header line followed by the raw lines).
pub fn render_text(e: &Event) -> String {
    let body = match e {
        Event::Start { id, log } => format!("id={id} log={log}"),
        Event::Env { file, loaded, .. } => format!("ENV loaded={loaded} file={file}"),
        Event::Step {
            n,
            total,
            ok,
            exit,
            dur,
            cmd,
        } => {
            if *ok {
                format!("STEP {n}/{total} OK   {}  {}", fmt_secs(*dur), clip(cmd))
            } else {
                format!(
                    "STEP {n}/{total} FAIL exit={exit} {}  {}",
                    fmt_secs(*dur),
                    clip(cmd)
                )
            }
        }
        Event::Stall {
            after,
            cpu,
            io,
            pids,
            unknown,
            last,
        } => {
            let io = match io {
                Some(b) => fmt_bytes(*b),
                None => "?".to_string(),
            };
            let unk = if *unknown > 0 {
                format!(" unknown={unknown}")
            } else {
                String::new()
            };
            format!(
                "STALL {} cpu={cpu:.1}s io={io} pids={pids}{unk} last={}",
                fmt_secs(*after),
                quote(last)
            )
        }
        Event::Resumed { after } => format!("RESUMED after {}", fmt_secs(*after)),
        Event::Prompt { line, via } => format!("PROMPT line={} via={via}", quote(line)),
        Event::Orphans { pids } => {
            let ids: Vec<String> = pids.iter().map(|(p, _)| p.to_string()).collect();
            let cmds: Vec<String> = pids.iter().take(5).map(|(_, c)| quote(c)).collect();
            format!("ORPHANS pids={} {}", ids.join(","), cmds.join(" | "))
        }
        Event::ErrorLive { log_line, text } => format!("ERROR_LIVE log:{log_line} {}", quote(text)),
        Event::Busy { after, cpu } => format!("BUSY {} cpu={cpu:.1}s", fmt_secs(*after)),
        Event::Errors { count, items } => {
            let it: Vec<String> = items
                .iter()
                .map(|(n, t)| format!("log:{n} {}", quote(t)))
                .collect();
            format!("ERRORS {count}: {}", it.join(" | "))
        }
        Event::Tail { n, lines } => block("TAIL", *n, lines),
        Event::Head { n, lines } => block("HEAD", *n, lines),
        Event::Skipped { lines } => format!("SKIPPED {lines} lines"),
        Event::Timeout { after } => format!("TIMEOUT after={}", fmt_secs(*after)),
        Event::Ready { what } => format!("READY {what}"),
        Event::Stopped { signal } => format!("STOPPED sig={signal}"),
        Event::Retry { n, max, exit } => format!("RETRY {n}/{max} exit={exit}"),
        Event::Lost { killed } => format!("LOST killed={killed}"),
        Event::LogFull { bytes } => format!("LOG_FULL at={}", fmt_bytes(*bytes)),
        Event::Done { exit, reason, dur } => {
            format!("DONE exit={exit} reason={} {}", reason.as_str(), fmt_secs(*dur))
        }
        Event::Error { msg } => format!("ERROR {msg}"),
    };
    format!("{PREFIX} {body}")
}

fn block(word: &str, n: usize, lines: &[String]) -> String {
    let mut s = format!("{word} {n}");
    for l in lines {
        s.push('\n');
        s.push_str(l);
    }
    s
}

pub fn unix_secs(t: SystemTime) -> f64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `{"v":1,"ts":...,"event":"STALL",...}`
pub fn render_json(e: &Event, ts: SystemTime) -> String {
    let mut v = serde_json::to_value(e).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(m) = &mut v {
        let mut out = serde_json::Map::new();
        out.insert("v".into(), 1.into());
        out.insert(
            "ts".into(),
            serde_json::json!((unix_secs(ts) * 1000.0).round() / 1000.0),
        );
        for (k, val) in std::mem::take(m) {
            out.insert(k, val);
        }
        return serde_json::Value::Object(out).to_string();
    }
    v.to_string()
}

/// Parse one `events.jsonl` line back into an event (readers: `wait`, `sleep`).
pub fn parse_json(line: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("v").and_then(|x| x.as_u64()).unwrap_or(1) > 1 {
        return None;
    }
    serde_json::from_value(v).ok()
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
