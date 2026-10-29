use super::*;
use crate::monitor::data::LogLine;

fn st(id: &str) -> Status {
    Status::initial(id, std::path::Path::new("/x/out.log"), 1, "")
}

fn log(n: usize, at_start: bool) -> Log {
    Log {
        lines: (0..n)
            .map(|i| LogLine {
                text: i.to_string(),
                ..LogLine::default()
            })
            .collect(),
        end: 100,
        at_start,
    }
}

fn app(ids: &[&str]) -> App {
    let mut a = App {
        page: 10,
        ..App::default()
    };
    a.set_jobs(ids.iter().map(|i| st(i)).collect());
    a
}

#[test]
fn keys() {
    let none = KeyModifiers::NONE;
    assert_eq!(action(KeyCode::Char('q'), none), Some(Action::Quit));
    assert_eq!(action(KeyCode::Esc, none), Some(Action::Quit));
    assert_eq!(
        action(KeyCode::Char('c'), KeyModifiers::CONTROL),
        Some(Action::Quit)
    );
    assert_eq!(action(KeyCode::Char('c'), none), None);
    assert_eq!(action(KeyCode::Char('o'), none), Some(Action::Toggle));
    assert_eq!(action(KeyCode::PageUp, none), Some(Action::PageUp));
    assert_eq!(action(KeyCode::End, none), Some(Action::End));
}

#[test]
fn selection_moves_and_reloads() {
    let mut a = app(&["a", "b", "c"]);
    assert_eq!(a.apply(Action::Up), Effect::None);
    assert_eq!(a.apply(Action::Down), Effect::Reload);
    assert_eq!(a.selected().unwrap().id, "b");
    a.apply(Action::Down);
    assert_eq!(a.apply(Action::Down), Effect::None);
    assert_eq!(a.sel, 2);
}

#[test]
fn selection_follows_the_job_across_refreshes() {
    let mut a = app(&["a", "b"]);
    a.apply(Action::Down);
    a.scroll = 5;
    // a new job appears on top
    assert!(!a.set_jobs(vec![st("n"), st("a"), st("b")]));
    assert_eq!(a.selected().unwrap().id, "b");
    assert_eq!(a.scroll, 5);
    // the selected job is gone
    assert!(a.set_jobs(vec![st("n")]));
    assert_eq!(a.sel, 0);
    assert_eq!(a.scroll, 0);
    assert!(a.set_jobs(vec![]));
    assert!(a.selected().is_none());
}

#[test]
fn output_scrolls_up_and_back_to_follow() {
    let mut a = app(&["a"]);
    a.set_log(log(15, false));
    assert!(a.following());
    assert_eq!(a.want_lines(), 20);
    // 15 loaded, 20 needed for a page up: load more
    assert_eq!(a.apply(Action::PageUp), Effect::More);
    assert_eq!(a.scroll, 10);
    assert!(!a.following());
    // the file has only 25 lines
    a.set_log(log(25, true));
    assert_eq!(a.scroll, 10);
    assert_eq!(a.apply(Action::PageUp), Effect::None);
    assert_eq!(a.scroll, 15, "clamped: the top line is the first one");
    assert_eq!(a.apply(Action::PageDown), Effect::None);
    assert_eq!(a.scroll, 5);
    assert_eq!(a.apply(Action::End), Effect::More);
    assert!(a.following());
    assert_eq!(a.apply(Action::End), Effect::None);
}

#[test]
fn home_goes_to_the_top() {
    let mut a = app(&["a"]);
    a.set_log(log(30, false));
    assert_eq!(a.apply(Action::Home), Effect::More);
    assert_eq!(a.want_lines(), MAX_LINES);
    a.set_log(log(30, true));
    assert_eq!(a.scroll, 20);
}

#[test]
fn steps_pane_scrolls_from_the_top() {
    let mut a = app(&["a"]);
    assert_eq!(a.apply(Action::Toggle), Effect::Reload);
    assert_eq!(a.pane, Pane::Steps);
    let d = |i: usize| Detail {
        kind: crate::monitor::data::Kind::Code,
        text: i.to_string(),
    };
    a.set_steps((0..25).map(d).collect());
    a.apply(Action::PageDown);
    assert_eq!(a.scroll, 10);
    a.apply(Action::End);
    assert_eq!(a.scroll, 15);
    a.apply(Action::Home);
    assert_eq!(a.scroll, 0);
    a.apply(Action::Toggle);
    assert_eq!(a.pane, Pane::Output);
}

#[test]
fn pan_left_right() {
    let mut a = app(&["a"]);
    a.apply(Action::Left);
    assert_eq!(a.pan, 0);
    a.apply(Action::Right);
    a.apply(Action::Right);
    assert_eq!(a.pan, 16);
    a.apply(Action::Left);
    assert_eq!(a.pan, 8);
}
