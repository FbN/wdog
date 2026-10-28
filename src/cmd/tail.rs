//! `wdog tail <id> [-n N] [-f]` (BG-3).

use crate::console;
use crate::exit::Exit;
use crate::job::dir::{job, wdog_home};
use crate::job::status;
use crate::job::tailer::{Tailer, last_lines};
use crate::lines::logfmt;
use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;

pub fn run(id: &str, n: usize, follow: bool, raw: bool) -> Result<Exit> {
    let home = wdog_home()?;
    let paths = job(&home, id)?;
    let log: PathBuf = status::read(&paths.status)
        .map(|s| s.log.into())
        .unwrap_or(paths.log.clone());
    let show = |l: &str| {
        if raw {
            console::out(l)
        } else {
            console::out(logfmt::raw(l))
        }
    };
    for l in last_lines(&log, n).unwrap_or_default() {
        show(&l);
    }
    if !follow {
        return Ok(Exit::Code(0));
    }
    let mut t = Tailer::from_end(&log);
    let ex = async_executor::LocalExecutor::new();
    async_io::block_on(ex.run(async {
        loop {
            async_io::Timer::after(Duration::from_millis(200)).await;
            let lines = t.read_lines();
            for l in &lines {
                show(l);
            }
            let done = status::read(&paths.status)
                .map(|s| !crate::cmd::status::effective(s).is_live_state())
                .unwrap_or(true);
            if done && lines.is_empty() {
                for l in t.read_lines() {
                    show(&l);
                }
                break;
            }
        }
    }));
    Ok(Exit::Code(0))
}
