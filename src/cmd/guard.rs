//! Hidden `wdog __guard <id>`: detach (own session, double fork), then watch the pipe.

use crate::exit::Exit;
use crate::job::dir::{job, wdog_home};
use anyhow::Result;
use nix::sys::signal::{SigHandler, Signal, signal};
use nix::unistd::{ForkResult, fork, setsid};

pub fn run(id: &str) -> Result<Exit> {
    let home = wdog_home()?;
    let paths = job(&home, id)?;
    let _ = setsid();
    // SAFETY: single-threaded at this point; the parent only exits.
    match unsafe { fork() }? {
        ForkResult::Parent { .. } => return Ok(Exit::Code(0)),
        ForkResult::Child => {}
    }
    for s in [Signal::SIGINT, Signal::SIGHUP, Signal::SIGPIPE] {
        // SAFETY: ignoring signals is always sound.
        let _ = unsafe { signal(s, SigHandler::SigIgn) };
    }
    let _ = std::fs::write(&paths.guard_pid, format!("{}\n", std::process::id()));
    crate::exec::guard::run(&paths)?;
    Ok(Exit::Code(0))
}
