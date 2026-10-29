//! Drawing: the job table on the left, the output or the steps on the right.

use super::data::{Detail, Kind, LogLine};
use super::model::{App, Pane};
use crate::cmd::status::StepState;
use crate::duration::fmt_secs;
use crate::job::status::Status;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Row, Table, TableState};

const LEFT_WIDTH: u16 = 46;
/// The NAME column, when some job has a name.
const NAME_WIDTH: usize = 14;

/// Pure: the width of the NAME column (0: no job has a name).
pub fn name_width(jobs: &[Status]) -> usize {
    let w = jobs
        .iter()
        .filter_map(|s| s.name.as_ref())
        .map(|n| n.chars().count())
        .max();
    w.map_or(0, |w| w.clamp(4, NAME_WIDTH))
}

/// Pure: `name (id)`, or the id.
pub fn label(st: &Status) -> String {
    match &st.name {
        Some(n) => format!("{n} ({})", st.id),
        None => st.id.clone(),
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let [body, foot] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(f.area());
    let nw = name_width(&app.jobs);
    let extra = if nw > 0 { nw as u16 + 1 } else { 0 };
    let left_w = (LEFT_WIDTH + extra).min(body.width / 2);
    let [left, right] = Layout::horizontal([Constraint::Length(left_w), Constraint::Min(10)]).areas(body);
    // borders take one row above and below
    app.page = right.height.saturating_sub(2).max(1) as usize;
    draw_jobs(f, app, left);
    draw_right(f, app, right);
    draw_footer(f, app, foot);
}

/// Pure: idle seconds (live jobs) and run time, as shown in the table.
pub fn times(st: &Status, now: f64) -> (String, String) {
    let idle = if st.is_live_state() {
        fmt_secs((now - st.last_activity_at).max(0.0).floor())
    } else {
        "-".into()
    };
    let run = fmt_secs((st.ended_at.unwrap_or(now) - st.started_at).floor());
    (idle, run)
}

/// Alerts of a live job; once it ended, its state says enough.
fn live_alerts(st: &Status) -> &[String] {
    if st.is_live_state() { &st.alerts } else { &[] }
}

/// Pure: the STATUS cell: the state, a failing exit code, then any alerts.
pub fn status_text(st: &Status) -> String {
    let mut s = st.state.clone();
    if let Some(e) = st.exit.filter(|e| *e != 0) {
        s.push_str(&format!(" {e}"));
    }
    for a in live_alerts(st) {
        s.push(' ');
        s.push_str(a);
    }
    s
}

fn status_style(st: &Status) -> Style {
    let red = ["STALL", "PROMPT", "ERROR_LIVE", "ORPHANS"];
    let alerts = live_alerts(st);
    if alerts.iter().any(|a| red.contains(&a.as_str())) {
        return Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
    }
    if !alerts.is_empty() {
        return Style::new().fg(Color::Yellow);
    }
    match st.state.as_str() {
        "done" => Style::new().fg(Color::Green),
        "running" | "starting" => Style::new().fg(Color::Cyan),
        "stopped" => Style::new().fg(Color::Yellow),
        _ => Style::new().fg(Color::Red),
    }
}

fn draw_jobs(f: &mut Frame, app: &App, area: Rect) {
    let nw = name_width(&app.jobs);
    let mut head = vec!["ID"];
    let mut widths = vec![Constraint::Length(6)];
    if nw > 0 {
        head.push("NAME");
        widths.push(Constraint::Length(nw as u16));
    }
    head.extend(["STATUS", "IDLE", "STEPS", "TIME"]);
    widths.extend([
        Constraint::Min(8),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(6),
    ]);
    let header = Row::new(head).style(Style::new().bold());
    let rows = app.jobs.iter().map(|st| {
        let (idle, run) = times(st, app.now);
        let mut cells = vec![Span::raw(st.id.clone())];
        if nw > 0 {
            cells.push(Span::raw(st.name.clone().unwrap_or_default()));
        }
        cells.extend([
            Span::styled(status_text(st), status_style(st)),
            Span::raw(idle),
            Span::raw(format!("{}/{}", st.step, st.total)),
            Span::raw(run),
        ]);
        Row::new(cells)
    });
    let t = Table::new(rows, widths)
        .header(header)
        .block(Block::bordered().title(" wdog jobs "))
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = TableState::default().with_selected((!app.jobs.is_empty()).then_some(app.sel));
    f.render_stateful_widget(t, area, &mut state);
}

fn draw_right(f: &mut Frame, app: &App, area: Rect) {
    let Some(st) = app.selected() else {
        let p = Paragraph::new(vec![
            Line::raw("No jobs."),
            Line::raw(""),
            Line::raw("Start one with: wdog start -- 'cmd'"),
        ])
        .block(Block::bordered());
        f.render_widget(p, area);
        return;
    };
    let page = app.page;
    let (title, lines) = match app.pane {
        Pane::Output => {
            let n = app.log.lines.len();
            let bottom = n - app.scroll.min(n);
            let top = bottom.saturating_sub(page);
            let where_ = if app.following() {
                "following".to_string()
            } else {
                format!("{} lines up · End to follow", app.scroll)
            };
            let lines = app.log.lines[top..bottom]
                .iter()
                .map(|l| log_line(l, app.pan))
                .collect::<Vec<_>>();
            (format!(" {} output · {where_} ", label(st)), lines)
        }
        Pane::Steps => {
            let end = (app.scroll + page).min(app.steps.len());
            let lines = app.steps[app.scroll.min(end)..end]
                .iter()
                .map(|d| detail_line(d, app.pan))
                .collect::<Vec<_>>();
            (format!(" {} steps ", label(st)), lines)
        }
    };
    let p = Paragraph::new(lines).block(Block::bordered().title(title));
    f.render_widget(p, area);
}

fn skip(s: &str, n: usize) -> String {
    s.chars().skip(n).collect()
}

fn log_line(l: &LogLine, pan: usize) -> Line<'static> {
    let tag = if l.err {
        Span::styled(
            format!("{:>4} ", format!("{}!", l.tag)),
            Style::new().fg(Color::Yellow),
        )
    } else {
        Span::styled(format!("{:>4} ", l.tag), Style::new().fg(Color::DarkGray))
    };
    Line::from(vec![tag, Span::raw(skip(&l.text, pan))])
}

fn detail_line(d: &Detail, pan: usize) -> Line<'static> {
    let style = match d.kind {
        Kind::Info => Style::new().fg(Color::DarkGray),
        Kind::Code => Style::new(),
        Kind::Step(s) => match s {
            StepState::Ok => Style::new().fg(Color::Green).bold(),
            StepState::Failed | StepState::Interrupted => Style::new().fg(Color::Red).bold(),
            StepState::Running => Style::new().fg(Color::Cyan).bold(),
            StepState::Pending | StepState::NotRun => Style::new().fg(Color::DarkGray).bold(),
        },
    };
    Line::styled(skip(&d.text, pan), style)
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys = " q quit  ↑↓ job  PgUp/PgDn scroll  Home/End top/follow  ←→ pan  o output/steps";
    let n = app.jobs.len();
    let count = format!("{n} job{} ", if n == 1 { "" } else { "s" });
    let w = area.width as usize;
    let pad = w.saturating_sub(keys.chars().count() + count.chars().count());
    let line = Line::from(vec![
        Span::raw(keys),
        Span::raw(" ".repeat(pad)),
        Span::raw(count),
    ])
    .style(Style::new().fg(Color::DarkGray));
    f.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
