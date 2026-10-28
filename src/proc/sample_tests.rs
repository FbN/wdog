use super::*;
use crate::proc::IoCounters;

fn e(pid: i32, cpu: u64, io: Option<u64>) -> ProcEntry {
    ProcEntry {
        pid,
        ppid: 1,
        pgid: 1,
        state: 'S',
        cpu_ticks: cpu,
        start_ticks: 100 + pid as u64,
        io: io.map(|b| IoCounters { rchar: b, wchar: 0 }),
        tty_read: TtyRead::No,
        cmdline: format!("cmd{pid}"),
    }
}

fn s(es: &[ProcEntry]) -> Sample {
    let pids: Vec<i32> = es.iter().map(|e| e.pid).collect();
    aggregate(es, &pids, Instant::now())
}

#[test]
fn aggregates() {
    let a = s(&[e(1, 5, Some(10)), e(2, 7, None)]);
    assert_eq!(a.cpu_ticks, 12);
    assert_eq!(a.io_bytes, 10);
    assert_eq!(a.io_unknown, 1);
    assert_eq!(a.pids(), vec![1, 2]);
}

#[test]
fn changes() {
    let base = s(&[e(1, 5, Some(10))]);
    assert_eq!(activity(&base, &s(&[e(1, 5, Some(10))])), Activity::None);
    assert_eq!(activity(&base, &s(&[e(1, 6, Some(10))])), Activity::Cpu(1));
    assert_eq!(activity(&base, &s(&[e(1, 5, Some(15))])), Activity::Io(5));
}

#[test]
fn member_exits_while_other_progresses() {
    let prev = s(&[e(1, 500, Some(10)), e(2, 5, Some(10))]);
    let cur = s(&[e(2, 6, Some(10))]);
    assert_eq!(activity(&prev, &cur), Activity::Cpu(1));
}

#[test]
fn new_member_and_unreadable() {
    let prev = s(&[e(1, 5, Some(10))]);
    assert_eq!(
        activity(&prev, &s(&[e(1, 5, Some(10)), e(3, 0, Some(0))])),
        Activity::Spawn
    );
    let prev = s(&[e(1, 5, None)]);
    assert_eq!(activity(&prev, &s(&[e(1, 5, None)])), Activity::Unknown);
    let prev = s(&[e(1, 5, None), e(2, 1, Some(1))]);
    assert_eq!(
        activity(&prev, &s(&[e(1, 5, None), e(2, 1, Some(1))])),
        Activity::None
    );
}
