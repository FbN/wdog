use super::*;

fn m(pid: i32, state: char) -> Member {
    Member {
        pid,
        start_ticks: pid as u64,
        cpu_ticks: 0,
        io: None,
        state,
        cmdline: format!("c{pid}"),
    }
}

#[test]
fn clean_exit() {
    let t0 = Instant::now();
    let o = Orphans {
        waiting_since: Some(t0),
        reported: vec![],
    };
    let g = Duration::from_secs(5);
    assert_eq!(decide(&o, true, &[], g, t0), Decision::Finish);
    let ms = [m(5, 'S')];
    let left = leftovers(&ms, None, &[]);
    assert_eq!(
        decide(&o, true, &left, g, t0),
        Decision::Finish,
        "NoChildren wins"
    );
    assert_eq!(
        decide(&o, false, &[], g, t0),
        Decision::Finish,
        "sample shows no members"
    );
}

#[test]
fn orphan_at_grace() {
    let t0 = Instant::now();
    let g = Duration::from_secs(5);
    let o = Orphans {
        waiting_since: Some(t0),
        reported: vec![],
    };
    let ms = [m(5, 'S'), m(6, 'Z'), m(7, 'S')];
    let left = leftovers(&ms, Some((7, 7)), &[]);
    assert_eq!(left.len(), 1, "zombie and main ignored");
    assert_eq!(
        decide(&o, false, &left, g, t0 + Duration::from_secs(4)),
        Decision::Wait
    );
    assert_eq!(
        decide(&o, false, &left, g, t0 + g),
        Decision::Alert(vec![(5, "c5".into())])
    );
}

#[test]
fn reported_ignored_and_not_waiting() {
    let ms = [m(5, 'S')];
    assert!(leftovers(&ms, None, &[(5, 5)]).is_empty());
    let o = Orphans::default();
    assert_eq!(
        decide(&o, true, &[], Duration::ZERO, Instant::now()),
        Decision::Wait
    );
}
