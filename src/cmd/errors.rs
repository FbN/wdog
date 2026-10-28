//! `wdog errors <id>` (BG-5): search out.log again with the job's patterns.

use crate::console;
use crate::detect::errors::{ErrorMatcher, scan};
use crate::event::{Event, quote, render_json};
use crate::exit::Exit;
use crate::job::dir::{job, wdog_home};
use crate::job::status;
use crate::spec::RunSpec;
use anyhow::{Context, Result};

pub fn run(id: &str, json: bool) -> Result<Exit> {
    let home = wdog_home()?;
    let paths = job(&home, id)?;
    let spec: RunSpec =
        serde_json::from_slice(&std::fs::read(&paths.spec).context("cannot read spec.json")?)?;
    let m = ErrorMatcher::new(&spec.err_patterns)?;
    let log = status::read(&paths.status)
        .map(|s| s.log.into())
        .unwrap_or(paths.log.clone());
    let text = std::fs::read(&log)
        .with_context(|| format!("cannot read {}", std::path::Path::new(&log).display()))?;
    let text = String::from_utf8_lossy(&text);
    let hits = scan(text.lines(), &m);
    if json {
        let e = Event::Errors {
            count: hits.len(),
            items: hits,
        };
        console::out(&render_json(&e, std::time::SystemTime::now()));
        return Ok(Exit::Code(0));
    }
    for (n, t) in &hits {
        console::out(&format!("log:{n} {}", quote(t)));
    }
    console::out(&format!("[wdog] ERRORS {}", hits.len()));
    Ok(Exit::Code(0))
}
