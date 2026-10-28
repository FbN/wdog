//! `wdog run`: the engine in the foreground.

use crate::cli::{RunArgs, to_spec};
use crate::core::Mode;
use crate::engine::{Sink, run_job};
use crate::exec::spawn::in_tty_foreground;
use crate::exit::Exit;
use crate::job::dir::{new_job, wdog_home};
use anyhow::{Result, bail};

pub fn run(a: &RunArgs) -> Result<Exit> {
    // SAFETY: isatty has no preconditions.
    let stdout_tty = unsafe { nix::libc::isatty(1) == 1 };
    let spec = to_spec(a, stdout_tty)?;
    if spec.stdin_tty && !in_tty_foreground() {
        bail!("--stdin-tty needs wdog in the terminal foreground");
    }
    let home = wdog_home()?;
    crate::job::gc::maybe_run(&home);
    let paths = new_job(&home)?;
    run_job(&paths, spec, Mode::Foreground, Sink::Console)
}
