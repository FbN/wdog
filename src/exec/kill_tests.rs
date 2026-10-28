use super::*;
use crate::proc::TtyRead;

fn e(pid: i32, pgid: i32, start: u64) -> ProcEntry {
    ProcEntry {
        pid,
        ppid: 1,
        pgid,
        state: 'S',
        cpu_ticks: 0,
        start_ticks: start,
        io: None,
        tty_read: TtyRead::No,
        cmdline: String::new(),
    }
}

#[test]
fn verified_groups_only() {
    let es = vec![e(10, 10, 100), e(11, 10, 110), e(20, 20, 999)];
    // group 10 has its recorded leader; group 20 reused the number of a group we recorded
    let plan = plan_kill(&es, &[(10, 10, 100), (20, 20, 200)], &[]);
    assert_eq!(plan.groups, vec![10]);
    assert_eq!(plan.skipped, 1);
    assert_eq!(plan.members, vec![10, 11]);
}

#[test]
fn leader_gone_but_member_known() {
    let es = vec![e(11, 10, 110)];
    let plan = plan_kill(&es, &[(10, 10, 100)], &[(11, 110)]);
    assert_eq!(plan.groups, vec![10]);
    assert_eq!(plan.pids, vec![11]);
    assert_eq!(plan.members, vec![11]);
}

#[test]
fn dead_groups_are_not_skipped() {
    let plan = plan_kill(&[], &[(10, 10, 100)], &[(11, 110)]);
    assert_eq!(plan, Plan::default());
}
