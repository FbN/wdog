//! Exit codes (spec §5.3) and how the process ends (overview decision 8).

use crate::event::DoneReason;
use nix::sys::signal::Signal;

pub const WDOG_FAILED: i32 = 125;
pub const LIMIT: i32 = 124;
pub const KILLED_ON_STALL: i32 = 126;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    /// wdog was stopped by this signal: re-raise it after cleanup.
    Signal(Signal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Completed,
    Max,
    KilledOnStall,
    Stopped(Signal),
}

/// Exit code of a step from its wait status: the code, or 128 + signal.
pub fn status_code(st: nix::sys::wait::WaitStatus) -> i32 {
    use nix::sys::wait::WaitStatus as W;
    match st {
        W::Exited(_, c) => c,
        W::Signaled(_, s, _) => 128 + s as i32,
        _ => WDOG_FAILED,
    }
}

/// `results` holds each finished step's exit code, in order.
pub fn run_exit(results: &[i32], end: EndReason, keep_going: bool) -> Exit {
    match end {
        EndReason::Max => Exit::Code(LIMIT),
        EndReason::KilledOnStall => Exit::Code(KILLED_ON_STALL),
        EndReason::Stopped(s) => Exit::Signal(s),
        EndReason::Completed => {
            let mut failing = results.iter().filter(|c| **c != 0);
            let code = if keep_going {
                failing.next_back()
            } else {
                failing.next()
            };
            Exit::Code(code.copied().unwrap_or(0))
        }
    }
}

pub fn done_reason(results: &[i32], end: EndReason) -> DoneReason {
    match end {
        EndReason::Max => DoneReason::Max,
        EndReason::KilledOnStall => DoneReason::KillOnStall,
        EndReason::Stopped(_) => DoneReason::Stopped,
        EndReason::Completed if results.iter().any(|c| *c != 0) => DoneReason::StepFailed,
        EndReason::Completed => DoneReason::Ok,
    }
}

/// The number written to `status.json` and the `DONE` line.
pub fn recorded_exit(e: &Exit) -> i32 {
    match e {
        Exit::Code(c) => *c,
        Exit::Signal(s) => 128 + *s as i32,
    }
}

/// End the process. For `Signal`, restore the default action, unblock it and raise it, so the
/// caller sees "killed by signal" (async-signal never restores SIG_DFL itself: spike row 14).
pub fn apply(e: Exit) -> ! {
    crate::console::flush();
    match e {
        Exit::Code(c) => std::process::exit(c),
        Exit::Signal(s) => {
            use nix::sys::signal::{
                SaFlags, SigAction, SigHandler, SigSet, SigmaskHow, sigaction, sigprocmask,
            };
            let dfl = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
            // SAFETY: restoring the default handler is always sound.
            let _ = unsafe { sigaction(s, &dfl) };
            let mut set = SigSet::empty();
            set.add(s);
            let _ = sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&set), None);
            let _ = nix::sys::signal::raise(s);
            std::thread::sleep(std::time::Duration::from_millis(100));
            std::process::exit(128 + s as i32)
        }
    }
}

#[cfg(test)]
#[path = "exit_tests.rs"]
mod tests;
