use super::*;
use crate::proc::TtyRead;

fn e(pid: i32, ppid: i32, pgid: i32) -> ProcEntry {
    ProcEntry {
        pid,
        ppid,
        pgid,
        state: 'S',
        cpu_ticks: 0,
        start_ticks: pid as u64 * 10,
        io: None,
        tty_read: TtyRead::No,
        cmdline: String::new(),
    }
}

fn sorted(mut v: Vec<i32>) -> Vec<i32> {
    v.sort();
    v
}

#[test]
fn chain_and_fanout() {
    let es = vec![
        e(1, 0, 1),
        e(10, 1, 10),
        e(11, 10, 11),
        e(12, 11, 11),
        e(13, 11, 11),
        e(99, 1, 99),
    ];
    assert_eq!(sorted(descendants(&es, &[10], &[])), vec![11, 12, 13]);
}

#[test]
fn reparented_orphan_and_pgid_member() {
    // 20 re-parented to the watcher 10; 30 left the tree but kept pgid 11
    let es = vec![e(10, 1, 10), e(11, 10, 11), e(20, 10, 11), e(30, 1, 11)];
    assert_eq!(sorted(descendants(&es, &[10], &[11])), vec![11, 20, 30]);
}

#[test]
fn zombies_excluded_and_cycles_safe() {
    let mut z = e(12, 10, 10);
    z.state = 'Z';
    let es = vec![e(10, 11, 10), e(11, 10, 10), z];
    assert_eq!(descendants(&es, &[10], &[]), vec![11]);
    assert!(descendants(&[], &[10], &[]).is_empty());
}

#[test]
fn group_check() {
    let es = vec![e(50, 1, 50), e(51, 50, 50)];
    assert!(group_is_ours(&es, 50, &[(51, 510)]));
    assert!(
        !group_is_ours(&es, 50, &[(51, 999)]),
        "same pid, other start time"
    );
    assert!(!group_is_ours(&es, 50, &[(77, 770)]), "only unknown members");
    assert!(alive(&es, 50, 500));
    assert!(!alive(&es, 50, 1));
}
