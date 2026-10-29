//! `wdog monitor`: a live, `top`-like view of the jobs, for humans.
//! It only reads WDOG_HOME; running jobs are never touched.

mod data;
mod model;
mod view;

use crate::event::unix_secs;
use crate::exit::Exit;
use crate::job::dir::{JobPaths, wdog_home};
use anyhow::{Result, bail};
use model::{App, Effect, Pane};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

const REFRESH: Duration = Duration::from_millis(500);

pub fn run() -> Result<Exit> {
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        bail!("monitor needs a terminal; use `wdog status` instead");
    }
    let home = wdog_home()?;
    let mut term = ratatui::init();
    let r = event_loop(&mut term, &home);
    ratatui::restore();
    r
}

fn event_loop(term: &mut ratatui::DefaultTerminal, home: &Path) -> Result<Exit> {
    let mut app = App::default();
    let mut last = None::<Instant>;
    loop {
        if last.is_none_or(|t| t.elapsed() >= REFRESH) {
            app.now = unix_secs(SystemTime::now());
            let changed = app.set_jobs(data::jobs(home));
            load(&mut app, home, changed);
            last = Some(Instant::now());
        }
        term.draw(|f| view::draw(f, &mut app))?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(k) = event::read()? else { continue };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        let Some(a) = model::action(k.code, k.modifiers) else {
            continue;
        };
        match app.apply(a) {
            Effect::Quit => return Ok(Exit::Code(0)),
            Effect::Reload => load(&mut app, home, true),
            Effect::More => load(&mut app, home, false),
            Effect::None => {}
        }
    }
}

/// Load the selected job's log (or steps). `fresh`: a newly selected job.
fn load(app: &mut App, home: &Path, fresh: bool) {
    let Some(st) = app.selected().cloned() else {
        app.set_log(data::Log::default());
        app.set_steps(vec![]);
        return;
    };
    match app.pane {
        Pane::Steps => app.set_steps(data::steps(&JobPaths::new(home, &st.id), &st)),
        Pane::Output => {
            let path = PathBuf::from(&st.log);
            let live = fresh || app.following();
            if !live && (app.log.at_start || app.log.lines.len() >= app.want_lines()) {
                // scrolled up and everything needed is loaded: keep the view still
                return;
            }
            let end = if live { data::log_len(&path) } else { app.log.end };
            app.set_log(data::read_log(&path, end, app.want_lines()));
        }
    }
}
