use super::*;
use crate::monitor::data::Log;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn screen(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| draw(f, app)).unwrap();
    let buf = t.backend().buffer();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn job(id: &str, state: &str) -> Status {
    let mut st = Status::initial(id, std::path::Path::new("/x/out.log"), 2, "");
    st.state = state.into();
    st.started_at = 1000.0;
    st.last_activity_at = 1090.0;
    st.step = 1;
    st
}

#[test]
fn table_and_output() {
    let mut failed = job("35b6f6", "failed");
    failed.exit = Some(1);
    failed.ended_at = Some(1013.0);
    let mut stalled = job("aa11bb", "running");
    stalled.alerts = vec!["STALL".into()];
    let mut app = App {
        now: 1100.0,
        ..App::default()
    };
    app.set_jobs(vec![stalled, failed]);
    app.set_log(Log {
        lines: vec![
            super::super::data::log_line("[1] compiling"),
            super::super::data::log_line("[1!] warning: x"),
        ],
        end: 10,
        at_start: true,
    });
    let s = screen(&mut app, 100, 12);
    assert!(s.contains("ID"), "{s}");
    assert!(s.contains("STATUS"));
    assert!(s.contains("aa11bb running STALL"), "{s}");
    assert!(s.contains("35b6f6 failed 1"), "{s}");
    assert!(s.contains("10s"), "idle of the running job: {s}");
    assert!(s.contains("13s"), "run time of the failed job: {s}");
    assert!(s.contains("aa11bb output · following"), "{s}");
    assert!(s.contains("   1 compiling"), "{s}");
    assert!(s.contains("  1! warning: x"), "{s}");
    assert!(s.contains("q quit"));
    assert!(s.contains("2 jobs"));
    assert_eq!(app.page, 9);
}

#[test]
fn no_jobs() {
    let mut app = App::default();
    let s = screen(&mut app, 80, 8);
    assert!(s.contains("No jobs."), "{s}");
}

#[test]
fn cells() {
    let mut st = job("a", "running");
    assert_eq!(times(&st, 1100.0), ("10s".into(), "1m40s".into()));
    st.alerts = vec!["BUSY".into(), "SILENCE".into()];
    assert_eq!(status_text(&st), "running BUSY SILENCE");
    st.state = "done".into();
    st.exit = Some(0);
    st.ended_at = Some(1041.0);
    assert_eq!(status_text(&st), "done");
    assert_eq!(times(&st, 9999.0).0, "-");
}
