//! The reducer. Pure: no I/O, no clock (`now` is passed in).

use super::{Effect, Input, MainProc, State};
use crate::detect::{busy, errors, orphans, prompt, stall};
use crate::event::Event;
use crate::exit::{self, EndReason, Exit};
use crate::lines::{Stream, logfmt};
use crate::proc::sample::{Activity, activity};
use nix::sys::signal::Signal;
use std::time::{Duration, Instant};

/// After SIGKILL, finish at the latest this long after, even if the reaper saw no ECHILD
/// (members outside our subtree that kept the group).
const KILL_SETTLE: Duration = Duration::from_secs(1);

pub fn step(mut s: State, input: Input, now: Instant) -> (State, Vec<Effect>) {
    let mut fx = vec![];
    apply(&mut s, input, now, &mut fx);
    (s, fx)
}

fn apply(s: &mut State, input: Input, now: Instant, fx: &mut Vec<Effect>) {
    match input {
        Input::Spawned {
            step,
            attempt,
            pid,
            pgid,
            start_ticks,
        } => {
            if step != s.step || attempt != s.attempt {
                return;
            }
            s.main = Some(MainProc { pid, start_ticks });
            s.no_children = false;
            if !s.pgids.iter().any(|(g, _, _)| *g == pgid) {
                s.pgids.push((pgid, pid, start_ticks));
            }
            if !s.pids.contains(&(pid, start_ticks)) {
                s.pids.push((pid, start_ticks));
            }
            fx.push(Effect::WriteStatus);
        }
        Input::Line {
            step,
            attempt,
            stream,
            text,
        } => on_line(s, step, attempt, stream, text, now, fx),
        Input::Partial {
            step,
            stream,
            text,
            since,
        } => {
            if step == s.step {
                let slot = &mut s.out.partial[stream_idx(stream)];
                *slot = if text.is_empty() {
                    None
                } else {
                    Some((text, since))
                };
                s.detectors.prompt = prompt::rearm(s.detectors.prompt);
            }
            output_activity(s, now, fx);
        }
        Input::MainExited { step, code, sample } => {
            if step != s.step || s.main.is_none() {
                return;
            }
            s.main = None;
            s.main_code = Some(code);
            if let Some(sm) = sample {
                s.pids = sm.members.iter().map(|m| (m.pid, m.start_ticks)).collect();
                s.pids.extend(s.detectors.orphans.reported.iter().copied());
                s.last_sample = Some(sm);
            }
            s.detectors.orphans.waiting_since = Some(now);
            try_complete(s, now, fx);
        }
        Input::NoChildren => {
            s.no_children = true;
            try_complete(s, now, fx);
            check_finish(s, now, fx);
        }
        Input::Sample(sm) => on_sample(s, sm, now, fx),
        Input::Tick => {
            if s.main.is_some() && s.ending.is_none() {
                let newest = s.out.partial.iter().flatten().max_by_key(|(_, t)| *t).cloned();
                let (p, fire) = prompt::check_text(
                    s.detectors.prompt,
                    newest.as_ref().map(|(t, i)| (t.as_str(), *i)),
                    s.spec.prompt_grace,
                    now,
                );
                s.detectors.prompt = p;
                if fire {
                    let line = newest.map(|(t, _)| t).unwrap_or_default();
                    emit(
                        fx,
                        Event::Prompt {
                            line: line.trim_end().to_string(),
                            via: "text".into(),
                        },
                    );
                }
            }
            try_complete(s, now, fx);
            check_finish(s, now, fx);
        }
        Input::Signal(sig) => {
            if s.finished {
                return;
            }
            if s.ending.is_none() {
                s.signal = Some(sig);
                emit(
                    fx,
                    Event::Stopped {
                        signal: sig.as_str().to_string(),
                    },
                );
                start_ending(s, EndReason::Stopped(sig), sig, s.mode.grace(), fx);
                try_complete(s, now, fx);
                check_finish(s, now, fx);
            } else if s.mode == super::Mode::Foreground && !s.killing {
                kill(s, now, fx);
            }
        }
        Input::LogFull { bytes } => {
            if !s.log_truncated {
                s.log_truncated = true;
                emit(fx, Event::LogFull { bytes });
            }
        }
        Input::MaxReached => {
            if s.ending.is_none() && !s.finished {
                let after = s.spec.max.unwrap_or_default().as_secs_f64();
                emit(fx, Event::Timeout { after });
                start_ending(s, EndReason::Max, Signal::SIGTERM, Duration::from_secs(5), fx);
                try_complete(s, now, fx);
                check_finish(s, now, fx);
            }
        }
        Input::GraceExpired => {
            if s.ending.is_some() && !s.killing && !s.finished {
                kill(s, now, fx);
            }
        }
    }
}

fn stream_idx(st: Stream) -> usize {
    match st {
        Stream::Out => 0,
        Stream::Err => 1,
    }
}

fn emit(fx: &mut Vec<Effect>, e: Event) {
    fx.push(Effect::Emit(e));
    fx.push(Effect::WriteStatus);
}

fn on_line(
    s: &mut State,
    step: usize,
    attempt: u32,
    stream: Stream,
    text: String,
    now: Instant,
    fx: &mut Vec<Effect>,
) {
    s.out.log_line_no += 1;
    let line_no = s.out.log_line_no;
    fx.push(Effect::AppendLog(logfmt::format(
        &logfmt::label(step + 1, attempt),
        stream,
        &text,
    )));
    if !s.spec.quiet {
        fx.push(match stream {
            Stream::Out => Effect::Print(text.clone()),
            Stream::Err => Effect::PrintErr(text.clone()),
        });
    }
    if s.matcher.is_match(&text) {
        let live = step == s.step && attempt == s.attempt && s.main.is_some() && s.ending.is_none();
        let (e, fire) = errors::on_match(std::mem::take(&mut s.detectors.errors), line_no, &text, live, now);
        s.detectors.errors = e;
        if fire {
            emit(
                fx,
                Event::ErrorLive {
                    log_line: line_no,
                    text: text.clone(),
                },
            );
        }
    }
    if step == s.step {
        s.out.partial[stream_idx(stream)] = None;
        s.detectors.prompt = prompt::rearm(s.detectors.prompt);
    }
    s.out.tail.push(text.clone());
    if !s.out.head.is_full() {
        s.out.head.push(text.clone());
    }
    s.out.last_lines.push(text);
    output_activity(s, now, fx);
}

fn output_activity(s: &mut State, now: Instant, fx: &mut Vec<Effect>) {
    s.out.last_output_at = now;
    s.status_dirty = true;
    s.detectors.busy = busy::on_output(s.detectors.busy);
    activity_seen(s, now, fx);
}

fn activity_seen(s: &mut State, now: Instant, fx: &mut Vec<Effect>) {
    s.last_activity_at = now;
    let (st, resumed) = stall::on_activity(s.detectors.stall, now);
    s.detectors.stall = st;
    if let Some(after) = resumed {
        emit(fx, Event::Resumed { after });
    }
}

fn on_sample(s: &mut State, sm: crate::proc::sample::Sample, now: Instant, fx: &mut Vec<Effect>) {
    let act = s.last_sample.as_ref().map(|p| activity(p, &sm));
    let (dcpu, dio) = match &s.last_sample {
        Some(p) => (
            sm.cpu_ticks.saturating_sub(p.cpu_ticks),
            sm.io_bytes.saturating_sub(p.io_bytes),
        ),
        None => (0, 0),
    };
    match act {
        Some(Activity::Cpu(_)) | Some(Activity::Io(_)) | Some(Activity::Spawn) | Some(Activity::Unknown) => {
            activity_seen(s, now, fx)
        }
        Some(Activity::None) | None => {}
    }

    // known members: write status at once when a new one shows up (the guard needs it)
    let mut pids: Vec<(i32, u64)> = sm.members.iter().map(|m| (m.pid, m.start_ticks)).collect();
    for r in &s.detectors.orphans.reported {
        if !pids.contains(r) {
            pids.push(*r);
        }
    }
    if let Some(m) = s.main {
        if !pids.contains(&(m.pid, m.start_ticks)) {
            pids.push((m.pid, m.start_ticks));
        }
    }
    let new_member = pids.iter().any(|p| !s.pids.contains(p));
    s.pids = pids;

    let step_running = s.main.is_some() && s.ending.is_none();
    if step_running && act == Some(Activity::None) {
        let (st, fire) = stall::check(s.detectors.stall, s.last_activity_at, s.spec.stall, now);
        s.detectors.stall = st;
        if let Some(after) = fire {
            let io = if sm.io_unknown == sm.members.len() {
                None
            } else {
                Some(dio)
            };
            emit(
                fx,
                Event::Stall {
                    after,
                    cpu: dcpu as f64 / s.job.clk_tck,
                    io,
                    pids: sm.members.len(),
                    unknown: sm.io_unknown,
                    last: s.last_text(),
                },
            );
            if s.spec.kill_on_stall && st.episode.is_some_and(|e| e.alerts == 1) {
                start_ending(
                    s,
                    EndReason::KilledOnStall,
                    Signal::SIGTERM,
                    Duration::from_secs(5),
                    fx,
                );
            }
        }
    }
    if step_running && s.spec.stdin_tty {
        let blocked = !sm.tty_read.is_empty() || sm.members.iter().any(|m| m.state == 'T');
        let (p, fire) = prompt::check_tty(
            s.detectors.prompt,
            blocked,
            s.out.last_output_at,
            s.spec.prompt_grace,
            now,
        );
        s.detectors.prompt = p;
        if fire {
            emit(
                fx,
                Event::Prompt {
                    line: s.last_text().trim_end().to_string(),
                    via: "tty".into(),
                },
            );
        }
    }
    if let (true, Some(th)) = (step_running, s.spec.busy) {
        let rising = matches!(act, Some(Activity::Cpu(_)));
        let (b, fire) = busy::on_sample(s.detectors.busy, rising, sm.cpu_ticks, th, now);
        s.detectors.busy = b;
        if let Some((after, ticks)) = fire {
            emit(
                fx,
                Event::Busy {
                    after,
                    cpu: ticks as f64 / s.job.clk_tck,
                },
            );
        }
    }
    s.last_sample = Some(sm);
    try_complete(s, now, fx);
    check_finish(s, now, fx);
    if new_member || s.status_dirty {
        s.status_dirty = false;
        fx.push(Effect::WriteStatus);
    }
}

fn start_ending(s: &mut State, reason: EndReason, sig: Signal, grace: Duration, fx: &mut Vec<Effect>) {
    s.ending = Some(reason);
    fx.push(Effect::Signal(sig));
    fx.push(Effect::ArmGrace(grace));
    fx.push(Effect::WriteStatus);
}

fn kill(s: &mut State, now: Instant, fx: &mut Vec<Effect>) {
    s.killing = true;
    s.killed_at = Some(now);
    fx.push(Effect::Signal(Signal::SIGKILL));
}

/// The main process exited: decide whether the step is complete (AN-5).
fn try_complete(s: &mut State, now: Instant, fx: &mut Vec<Effect>) {
    if s.main_code.is_none() || s.finished {
        return;
    }
    if s.ending.is_some() {
        complete_step(s, now, fx);
        return;
    }
    let members = s
        .last_sample
        .as_ref()
        .map(|x| x.members.as_slice())
        .unwrap_or(&[]);
    let left = orphans::leftovers(members, None, &s.detectors.orphans.reported);
    match orphans::decide(
        &s.detectors.orphans,
        s.no_children,
        &left,
        s.spec.orphan_grace,
        now,
    ) {
        orphans::Decision::Wait => {}
        orphans::Decision::Finish => complete_step(s, now, fx),
        orphans::Decision::Alert(list) => {
            let ids: Vec<(i32, u64)> = left.iter().map(|m| (m.pid, m.start_ticks)).collect();
            s.detectors.orphans.reported.extend(ids);
            emit(fx, Event::Orphans { pids: list });
            complete_step(s, now, fx);
        }
    }
}

fn reset_step_detectors(s: &mut State) {
    s.detectors.stall = Default::default();
    s.detectors.prompt = Default::default();
    s.detectors.busy = Default::default();
    s.detectors.orphans.waiting_since = None;
    s.out.partial = [None, None];
}

fn complete_step(s: &mut State, now: Instant, fx: &mut Vec<Effect>) {
    let Some(code) = s.main_code.take() else { return };
    let dur = now.saturating_duration_since(s.step_started).as_secs_f64();
    emit(
        fx,
        Event::Step {
            n: s.step + 1,
            total: s.total(),
            ok: code == 0,
            exit: code,
            dur,
            cmd: s.spec.step_name(s.step),
        },
    );
    reset_step_detectors(s);
    if s.ending.is_some() {
        s.results.push(code);
        check_finish(s, now, fx);
        return;
    }
    if code != 0 && s.attempt < s.spec.retry {
        s.attempt += 1;
        emit(
            fx,
            Event::Retry {
                n: s.attempt,
                max: s.spec.retry,
                exit: code,
            },
        );
        begin_step(s, now, fx);
        return;
    }
    s.results.push(code);
    if (code == 0 || s.spec.keep_going) && s.step + 1 < s.total() {
        s.step += 1;
        s.attempt = 0;
        begin_step(s, now, fx);
    } else {
        finish(s, EndReason::Completed, fx);
    }
}

fn begin_step(s: &mut State, now: Instant, fx: &mut Vec<Effect>) {
    s.step_started = now;
    s.last_activity_at = now;
    s.no_children = false;
    fx.push(Effect::Spawn {
        step: s.step,
        attempt: s.attempt,
    });
}

/// While ending: finish once the step is closed and the tree is gone.
fn check_finish(s: &mut State, now: Instant, fx: &mut Vec<Effect>) {
    let Some(reason) = s.ending else { return };
    if s.finished || s.main.is_some() || s.main_code.is_some() {
        return;
    }
    let tree_empty = s
        .last_sample
        .as_ref()
        .is_some_and(|x| x.members.iter().all(|m| m.state == 'Z'));
    let settled = s
        .killed_at
        .is_some_and(|t| now.saturating_duration_since(t) >= KILL_SETTLE);
    if s.no_children || tree_empty || settled {
        finish(s, reason, fx);
    }
}

fn finish(s: &mut State, reason: EndReason, fx: &mut Vec<Effect>) {
    if s.ending.is_none() {
        s.ending = Some(reason);
    }
    s.finished = true;
    fx.push(Effect::Finish(reason));
}

pub fn exit_of(s: &State) -> Exit {
    exit::run_exit(
        &s.results,
        s.ending.unwrap_or(EndReason::Completed),
        s.spec.keep_going,
    )
}

/// The final summary (`ERRORS`, `HEAD`/`TAIL`, `DONE`), produced after the final drain.
pub fn summary(s: &State, now: Instant) -> Vec<Effect> {
    let mut fx = vec![];
    let e = &s.detectors.errors;
    if e.count > 0 {
        fx.push(Effect::Emit(Event::Errors {
            count: e.count,
            items: e.items.clone(),
        }));
    }
    let total = s.out.log_line_no as usize;
    let (h, t) = (s.spec.output.head, s.spec.output.tail);
    if h > 0 && !s.out.head.is_empty() {
        fx.push(Effect::Emit(Event::Head {
            n: h,
            lines: s.out.head.items(),
        }));
    }
    if t > 0 {
        let mut lines = s.out.tail.items();
        if h > 0 {
            let rest = total.saturating_sub(h);
            if rest > lines.len() {
                fx.push(Effect::Emit(Event::Skipped {
                    lines: (rest - lines.len()) as u64,
                }));
            }
            let keep = rest.min(lines.len());
            lines.drain(..lines.len() - keep);
        }
        if !lines.is_empty() {
            fx.push(Effect::Emit(Event::Tail { n: t, lines }));
        }
    }
    let end = s.ending.unwrap_or(EndReason::Completed);
    fx.push(Effect::Emit(Event::Done {
        exit: exit::recorded_exit(&exit_of(s)),
        reason: exit::done_reason(&s.results, end),
        dur: now.saturating_duration_since(s.started).as_secs_f64(),
    }));
    fx.push(Effect::WriteStatus);
    fx
}

#[cfg(test)]
#[path = "step_tests.rs"]
mod tests;
