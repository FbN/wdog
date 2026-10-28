use crate::lines::Stream;
use crate::proc::sample::Sample;
use nix::sys::signal::Signal;
use std::time::Instant;

/// Everything that can happen during a run. Inputs that need I/O to be complete (draining a
/// pipe, taking a sample) are completed by the shell before they are sent.
#[derive(Debug, Clone)]
pub enum Input {
    Spawned {
        step: usize,
        attempt: u32,
        pid: i32,
        pgid: i32,
        start_ticks: u64,
    },
    Line {
        step: usize,
        attempt: u32,
        stream: Stream,
        text: String,
    },
    /// The unterminated tail of a stream changed ("" = no tail).
    Partial {
        step: usize,
        stream: Stream,
        text: String,
        since: Instant,
    },
    /// Sent after the step's pipes were drained, with a fresh sample.
    MainExited {
        step: usize,
        code: i32,
        sample: Option<Sample>,
    },
    /// waitpid(-1) returned ECHILD: every descendant is gone.
    NoChildren,
    Sample(Sample),
    Tick,
    /// SIGINT/SIGTERM/SIGHUP to wdog.
    Signal(Signal),
    LogFull {
        bytes: u64,
    },
    MaxReached,
    /// TERM -> KILL escalation.
    GraceExpired,
}
