//! Hidden `wdog __supervise <id>`: the engine of a background job, detached and silent.

use crate::core::Mode;
use crate::engine::{Sink, run_job};
use crate::exit::{Exit, recorded_exit};
use crate::job::dir::{job, wdog_home};
use crate::spec::RunSpec;
use anyhow::{Context, Result, bail};

pub fn run(id: &str) -> Result<Exit> {
    let home = wdog_home()?;
    let paths = job(&home, id)?;
    let data = std::fs::read(&paths.spec).context("cannot read spec.json")?;
    let spec: RunSpec = serde_json::from_slice(&data).context("bad spec.json")?;
    if spec.v > 1 {
        bail!("spec.json was written by a newer wdog");
    }
    let exit = run_job(&paths, spec, Mode::Supervised, Sink::Silent)?;
    // nobody waits on the supervisor: record the code, exit normally
    Ok(Exit::Code(recorded_exit(&exit)))
}
