use super::*;
use crate::core::{JobInfo, Mode};
use crate::detect::errors::ErrorMatcher;
use crate::event::DoneReason;
use crate::proc::sample::{Member, Sample};
use crate::spec::RunSpec;
use std::rc::Rc;

fn job() -> Rc<JobInfo> {
    Rc::new(JobInfo {
        id: "abc123".into(),
        log: "/x/out.log".into(),
        watcher_pid: 1,
        watcher_start_ticks: 1,
        started_wall: 1000.0,
        clk_tck: 100.0,
    })
}

fn state_with(spec: RunSpec, mode: Mode, t0: Instant) -> State {
    let m = ErrorMatcher::new(&spec.err_patterns).unwrap();
    State::new(Rc::new(spec), Rc::new(m), job(), mode, t0)
}

fn state(steps: &[&str], t0: Instant) -> State {
    state_with(
        RunSpec::new(steps.iter().map(|s| s.to_string()).collect()),
        Mode::Foreground,
        t0,
    )
}

fn secs(t0: Instant, s: f64) -> Instant {
    t0 + Duration::from_secs_f64(s)
}

fn spawned(step: usize, pid: i32) -> Input {
    Input::Spawned {
        step,
        attempt: 0,
        pid,
        pgid: pid,
        start_ticks: pid as u64,
    }
}

fn exited(step: usize, code: i32, t: Instant) -> Input {
    Input::MainExited {
        step,
        code,
        sample: Some(Sample::empty(t)),
    }
}

fn member(pid: i32, cpu: u64) -> Member {
    Member {
        pid,
        start_ticks: pid as u64,
        cpu_ticks: cpu,
        io: Some(0),
        state: 'S',
        cmdline: format!("c{pid}"),
    }
}

fn sample(t: Instant, members: Vec<Member>) -> Sample {
    let cpu = members.iter().map(|m| m.cpu_ticks).sum();
    Sample {
        at: t,
        members,
        cpu_ticks: cpu,
        io_bytes: 0,
        io_unknown: 0,
        tty_read: vec![],
    }
}

/// Feed inputs; return the final state and every effect.
fn run(mut s: State, inputs: Vec<(Input, Instant)>) -> (State, Vec<Effect>) {
    let mut all = vec![];
    for (i, t) in inputs {
        let (ns, fx) = step(s, i, t);
        s = ns;
        all.extend(fx);
    }
    (s, all)
}

fn events(fx: &[Effect]) -> Vec<Event> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Emit(ev) => Some(ev.clone()),
            _ => None,
        })
        .collect()
}

fn words(fx: &[Effect]) -> Vec<String> {
    events(fx)
        .iter()
        .map(|e| {
            crate::event::render_text(e)
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

#[test]
fn success_sequence() {
    let t0 = Instant::now();
    let s = state(&["true", "true"], t0);
    let (s, fx) = run(
        s,
        vec![
            (spawned(0, 10), t0),
            (Input::NoChildren, secs(t0, 1.0)),
            (exited(0, 0, t0), secs(t0, 1.0)),
            (spawned(1, 11), secs(t0, 1.0)),
            (Input::NoChildren, secs(t0, 2.0)),
            (exited(1, 0, t0), secs(t0, 2.0)),
        ],
    );
    assert!(fx.contains(&Effect::Spawn { step: 1, attempt: 0 }));
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::Finish(EndReason::Completed)))
    );
    assert_eq!(words(&fx), vec!["STEP", "STEP"]);
    assert_eq!(exit_of(&s), Exit::Code(0));
    let sum = summary(&s, secs(t0, 2.0));
    let done = events(&sum).pop().unwrap();
    assert_eq!(
        done,
        Event::Done {
            exit: 0,
            reason: DoneReason::Ok,
            dur: 2.0
        }
    );
}

#[test]
fn stop_at_first_failure_and_keep_going() {
    let t0 = Instant::now();
    let inputs = |t0| {
        vec![
            (spawned(0, 10), t0),
            (Input::NoChildren, t0),
            (exited(0, 0, t0), t0),
            (spawned(1, 11), t0),
            (Input::NoChildren, t0),
            (exited(1, 1, t0), t0),
        ]
    };
    let (s, fx) = run(state(&["true", "false", "echo never"], t0), inputs(t0));
    assert!(!fx.contains(&Effect::Spawn { step: 2, attempt: 0 }));
    assert_eq!(exit_of(&s), Exit::Code(1));
    let mut spec = RunSpec::new(vec!["true".into(), "false".into(), "echo".into()]);
    spec.keep_going = true;
    let (_, fx) = run(state_with(spec, Mode::Foreground, t0), inputs(t0));
    assert!(fx.contains(&Effect::Spawn { step: 2, attempt: 0 }));
}

#[test]
fn signal_killed_step_is_a_code() {
    let t0 = Instant::now();
    let (s, _) = run(
        state(&["sleep 9"], t0),
        vec![
            (spawned(0, 10), t0),
            (Input::NoChildren, t0),
            (exited(0, 137, t0), t0),
        ],
    );
    assert_eq!(exit_of(&s), Exit::Code(137));
}

#[test]
fn max_escalation() {
    let t0 = Instant::now();
    let mut spec = RunSpec::new(vec!["sleep 600".into()]);
    spec.max = Some(Duration::from_secs(1));
    let s = state_with(spec, Mode::Foreground, t0);
    let (s, fx) = run(s, vec![(spawned(0, 10), t0), (Input::MaxReached, secs(t0, 1.0))]);
    assert!(fx.contains(&Effect::Signal(Signal::SIGTERM)));
    assert!(fx.contains(&Effect::ArmGrace(Duration::from_secs(5))));
    assert_eq!(words(&fx), vec!["TIMEOUT"]);
    let (s, fx) = run(s, vec![(Input::GraceExpired, secs(t0, 6.0))]);
    assert!(fx.contains(&Effect::Signal(Signal::SIGKILL)));
    let (s, fx) = run(
        s,
        vec![
            (Input::NoChildren, secs(t0, 6.1)),
            (exited(0, 137, t0), secs(t0, 6.1)),
        ],
    );
    assert!(fx.iter().any(|e| matches!(e, Effect::Finish(EndReason::Max))));
    assert_eq!(exit_of(&s), Exit::Code(124));
}

#[test]
fn external_sigterm_and_sigint_forwarded_as_is() {
    let t0 = Instant::now();
    let (s, fx) = run(
        state(&["sleep 600"], t0),
        vec![(spawned(0, 10), t0), (Input::Signal(Signal::SIGINT), t0)],
    );
    assert!(fx.contains(&Effect::Signal(Signal::SIGINT)));
    assert!(!fx.contains(&Effect::Signal(Signal::SIGTERM)));
    assert_eq!(words(&fx), vec!["STOPPED"]);
    let (s, _) = run(s, vec![(Input::NoChildren, t0), (exited(0, 130, t0), t0)]);
    assert!(s.finished);
    assert_eq!(exit_of(&s), Exit::Signal(Signal::SIGINT));
    assert_eq!(crate::exit::recorded_exit(&exit_of(&s)), 130);
}

#[test]
fn second_signal_kills_in_foreground_only() {
    let t0 = Instant::now();
    let inputs = vec![
        (spawned(0, 10), t0),
        (Input::Signal(Signal::SIGINT), t0),
        (Input::Signal(Signal::SIGINT), t0),
    ];
    let (_, fx) = run(state(&["x"], t0), inputs.clone());
    assert!(fx.contains(&Effect::Signal(Signal::SIGKILL)));
    let sup = state_with(RunSpec::new(vec!["x".into()]), Mode::Supervised, t0);
    let (_, fx) = run(sup, inputs);
    assert!(!fx.contains(&Effect::Signal(Signal::SIGKILL)));
    assert!(fx.contains(&Effect::ArmGrace(Duration::from_secs(30))));
}

#[test]
fn line_prefixes_and_late_orphan_lines() {
    let t0 = Instant::now();
    let line = |step, text: &str| Input::Line {
        step,
        attempt: 0,
        stream: Stream::Out,
        text: text.into(),
    };
    let (_, fx) = run(
        state(&["a", "b"], t0),
        vec![
            (spawned(0, 10), t0),
            (Input::NoChildren, t0),
            (exited(0, 0, t0), t0),
            (spawned(1, 11), t0),
            (line(1, "from two"), t0),
            (line(0, "late from one"), t0),
            (
                Input::Line {
                    step: 1,
                    attempt: 0,
                    stream: Stream::Err,
                    text: "err".into(),
                },
                t0,
            ),
        ],
    );
    let logs: Vec<&Effect> = fx.iter().filter(|e| matches!(e, Effect::AppendLog(_))).collect();
    assert_eq!(
        logs,
        vec![
            &Effect::AppendLog("[2] from two".into()),
            &Effect::AppendLog("[1] late from one".into()),
            &Effect::AppendLog("[2!] err".into())
        ]
    );
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::Print(_))),
        "quiet by default in tests"
    );
}

#[test]
fn spawned_writes_status_and_partial_counts_as_output() {
    let t0 = Instant::now();
    let (s, fx) = run(state(&["a"], t0), vec![(spawned(0, 10), t0)]);
    assert_eq!(fx, vec![Effect::WriteStatus]);
    let (s, _) = run(
        s,
        vec![(
            Input::Partial {
                step: 0,
                stream: Stream::Out,
                text: "50%".into(),
                since: t0,
            },
            secs(t0, 3.0),
        )],
    );
    assert_eq!(s.out.last_output_at, secs(t0, 3.0));
    assert_eq!(s.last_text(), "50%");
}

#[test]
fn stall_then_resumed_and_kill_on_stall() {
    let t0 = Instant::now();
    let mut spec = RunSpec::new(vec!["sleep".into()]);
    spec.stall = Duration::from_secs(10);
    let s = state_with(spec.clone(), Mode::Foreground, t0);
    let quiet = |t| Input::Sample(sample(t, vec![member(10, 5)]));
    let (s, fx) = run(
        s,
        vec![
            (spawned(0, 10), t0),
            (quiet(secs(t0, 2.0)), secs(t0, 2.0)),
            (quiet(secs(t0, 9.0)), secs(t0, 9.0)),
        ],
    );
    assert!(words(&fx).is_empty());
    let (s, fx) = run(s, vec![(quiet(secs(t0, 10.0)), secs(t0, 10.0))]);
    assert_eq!(words(&fx), vec!["STALL"]);
    assert_eq!(s.alerts(), vec!["STALL"]);
    let (_, fx) = run(
        s,
        vec![(
            Input::Line {
                step: 0,
                attempt: 0,
                stream: Stream::Out,
                text: "hi".into(),
            },
            secs(t0, 12.0),
        )],
    );
    assert_eq!(words(&fx), vec!["RESUMED"]);

    spec.kill_on_stall = true;
    let s = state_with(spec, Mode::Foreground, t0);
    let (s, fx) = run(
        s,
        vec![
            (spawned(0, 10), t0),
            (quiet(secs(t0, 1.0)), secs(t0, 1.0)),
            (quiet(secs(t0, 10.0)), secs(t0, 10.0)),
        ],
    );
    assert!(fx.contains(&Effect::Signal(Signal::SIGTERM)));
    let (s, _) = run(
        s,
        vec![
            (Input::NoChildren, secs(t0, 10.5)),
            (exited(0, 143, t0), secs(t0, 10.5)),
        ],
    );
    assert_eq!(exit_of(&s), Exit::Code(126));
    let done = events(&summary(&s, secs(t0, 11.0))).pop().unwrap();
    assert!(matches!(
        done,
        Event::Done {
            exit: 126,
            reason: DoneReason::KillOnStall,
            ..
        }
    ));
}

#[test]
fn cpu_activity_prevents_stall() {
    let t0 = Instant::now();
    let mut spec = RunSpec::new(vec!["busy".into()]);
    spec.stall = Duration::from_secs(2);
    let mut s = state_with(spec, Mode::Foreground, t0);
    s = run(s, vec![(spawned(0, 10), t0)]).0;
    let mut all = vec![];
    for i in 1..10u64 {
        let t = secs(t0, i as f64);
        let (ns, fx) = step(s, Input::Sample(sample(t, vec![member(10, i * 100)])), t);
        s = ns;
        all.extend(fx);
    }
    assert!(!words(&all).contains(&"STALL".to_string()));
}

#[test]
fn orphans_after_grace_and_clean_exit_by_sample() {
    let t0 = Instant::now();
    let s = state(&["sh -c 'sleep 600 &'", "echo two"], t0);
    let with_orphan = Input::MainExited {
        step: 0,
        code: 0,
        sample: Some(sample(t0, vec![member(20, 0)])),
    };
    let (s, fx) = run(
        s,
        vec![
            (spawned(0, 10), t0),
            (with_orphan, t0),
            (Input::Tick, secs(t0, 4.0)),
        ],
    );
    assert!(words(&fx).is_empty(), "still within grace");
    let (s, fx) = run(s, vec![(Input::Tick, secs(t0, 5.0))]);
    assert_eq!(words(&fx), vec!["ORPHANS", "STEP"]);
    assert!(fx.contains(&Effect::Spawn { step: 1, attempt: 0 }));
    assert!(
        s.pids.contains(&(20, 20)),
        "orphan stays recorded for wdog stop and the guard"
    );
    // step 2: the step-1 orphan is still alive but already reported; clean exit
    let (_, fx) = run(
        s,
        vec![
            (spawned(1, 11), secs(t0, 5.0)),
            (
                Input::MainExited {
                    step: 1,
                    code: 0,
                    sample: Some(sample(t0, vec![member(20, 0)])),
                },
                secs(t0, 5.1),
            ),
        ],
    );
    assert_eq!(words(&fx), vec!["STEP"]);
    assert!(fx.iter().any(|e| matches!(e, Effect::Finish(_))));
}

#[test]
fn error_live_only_while_main_alive() {
    let t0 = Instant::now();
    let line = |text: &str| Input::Line {
        step: 0,
        attempt: 0,
        stream: Stream::Err,
        text: text.into(),
    };
    let (s, fx) = run(
        state(&["x"], t0),
        vec![(spawned(0, 10), t0), (line("FATAL boom"), t0)],
    );
    assert_eq!(words(&fx), vec!["ERROR_LIVE"]);
    let (s, fx) = run(
        s,
        vec![
            (Input::NoChildren, t0),
            (exited(0, 1, t0), t0),
            (line("panic: late"), t0),
        ],
    );
    assert!(!words(&fx).contains(&"ERROR_LIVE".to_string()));
    let sum = events(&summary(&s, t0));
    assert!(matches!(&sum[0], Event::Errors { count: 2, .. }));
}

#[test]
fn text_prompt_after_grace() {
    let t0 = Instant::now();
    let (s, _) = run(
        state(&["x"], t0),
        vec![
            (spawned(0, 10), t0),
            (
                Input::Partial {
                    step: 0,
                    stream: Stream::Err,
                    text: "Continue? (y/N) ".into(),
                    since: t0,
                },
                t0,
            ),
        ],
    );
    let (s, fx) = run(s, vec![(Input::Tick, secs(t0, 4.0))]);
    assert!(words(&fx).is_empty());
    let (_, fx) = run(s, vec![(Input::Tick, secs(t0, 5.0))]);
    assert_eq!(
        events(&fx),
        vec![Event::Prompt {
            line: "Continue? (y/N)".into(),
            via: "text".into()
        }]
    );
}

#[test]
fn retry_then_success() {
    let t0 = Instant::now();
    let mut spec = RunSpec::new(vec!["flaky".into()]);
    spec.retry = 2;
    let s = state_with(spec, Mode::Foreground, t0);
    let (s, fx) = run(
        s,
        vec![
            (spawned(0, 10), t0),
            (Input::NoChildren, t0),
            (exited(0, 1, t0), t0),
        ],
    );
    assert_eq!(words(&fx), vec!["STEP", "RETRY"]);
    assert!(fx.contains(&Effect::Spawn { step: 0, attempt: 1 }));
    let (s, fx) = run(
        s,
        vec![
            (
                Input::Spawned {
                    step: 0,
                    attempt: 1,
                    pid: 11,
                    pgid: 11,
                    start_ticks: 11,
                },
                t0,
            ),
            (
                Input::Line {
                    step: 0,
                    attempt: 1,
                    stream: Stream::Out,
                    text: "ok".into(),
                },
                t0,
            ),
            (Input::NoChildren, t0),
            (exited(0, 0, t0), t0),
        ],
    );
    assert!(fx.contains(&Effect::AppendLog("[1.1] ok".into())));
    assert_eq!(exit_of(&s), Exit::Code(0));
}

#[test]
fn log_full_once() {
    let t0 = Instant::now();
    let (s, fx) = run(
        state(&["x"], t0),
        vec![
            (Input::LogFull { bytes: 100 }, t0),
            (Input::LogFull { bytes: 100 }, t0),
        ],
    );
    assert_eq!(words(&fx), vec!["LOG_FULL"]);
    assert!(s.log_truncated);
}

#[test]
fn head_tail_summary() {
    let t0 = Instant::now();
    let mut spec = RunSpec::new(vec!["seq".into()]);
    spec.output = crate::spec::OutputMode { head: 2, tail: 2 };
    let mut s = state_with(spec, Mode::Foreground, t0);
    for i in 1..=10 {
        s = step(
            s,
            Input::Line {
                step: 0,
                attempt: 0,
                stream: Stream::Out,
                text: i.to_string(),
            },
            t0,
        )
        .0;
    }
    let ev = events(&summary(&s, t0));
    assert_eq!(
        ev[0],
        Event::Head {
            n: 2,
            lines: vec!["1".into(), "2".into()]
        }
    );
    assert_eq!(ev[1], Event::Skipped { lines: 6 });
    assert_eq!(
        ev[2],
        Event::Tail {
            n: 2,
            lines: vec!["9".into(), "10".into()]
        }
    );
}
