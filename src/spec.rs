//! RunSpec: everything a run needs, stored as spec.json for background jobs.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

pub const SPEC_VERSION: u32 = 1;

/// AN-6 built-in patterns, matched as literal, case-sensitive text.
pub const DEFAULT_ERR_PATTERNS: [&str; 7] = [
    "FATAL",
    "Traceback",
    "panic:",
    "Segmentation fault",
    "ECONNREFUSED",
    "EADDRINUSE",
    "out of memory",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputMode {
    pub head: usize,
    pub tail: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSpec {
    pub v: u32,
    pub steps: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    /// Absolute paths only; values are never stored.
    pub env_files: Vec<PathBuf>,
    pub log: Option<PathBuf>,
    pub output: OutputMode,
    pub quiet: bool,
    pub json: bool,
    #[serde(with = "ms")]
    pub stall: Duration,
    #[serde(with = "ms")]
    pub interval: Duration,
    #[serde(with = "ms")]
    pub prompt_grace: Duration,
    #[serde(with = "ms")]
    pub orphan_grace: Duration,
    #[serde(with = "ms_opt")]
    pub busy: Option<Duration>,
    /// Final list of regexes, defaults already merged.
    pub err_patterns: Vec<String>,
    #[serde(with = "ms_opt")]
    pub max: Option<Duration>,
    pub kill_on_stall: bool,
    pub keep_going: bool,
    pub stdin_tty: bool,
    pub retry: u32,
}

impl RunSpec {
    /// A spec with defaults, for tests.
    #[cfg(test)]
    pub fn new(steps: Vec<String>) -> RunSpec {
        RunSpec {
            v: SPEC_VERSION,
            steps,
            cwd: None,
            env: vec![],
            env_files: vec![],
            log: None,
            output: OutputMode { head: 0, tail: 20 },
            quiet: true,
            json: false,
            stall: Duration::from_secs(120),
            interval: Duration::from_secs(2),
            prompt_grace: Duration::from_secs(5),
            orphan_grace: Duration::from_secs(5),
            busy: None,
            err_patterns: merge_patterns(&[], false),
            max: None,
            kill_on_stall: false,
            keep_going: false,
            stdin_tty: false,
            retry: 0,
        }
    }
}

/// Built-in patterns (escaped, literal) followed by the user's `--err` regexes.
pub fn merge_patterns(user: &[String], no_default: bool) -> Vec<String> {
    let mut out: Vec<String> = if no_default {
        vec![]
    } else {
        DEFAULT_ERR_PATTERNS.iter().map(|p| regex::escape(p)).collect()
    };
    out.extend(user.iter().cloned());
    out
}

mod ms {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;
    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_millis() as u64)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(d)?))
    }
}

mod ms_opt {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;
    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.serialize_some(&(d.as_millis() as u64)),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        Ok(Option::<u64>::deserialize(d)?.map(Duration::from_millis))
    }
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
