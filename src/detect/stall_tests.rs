use super::*;

const T: Duration = Duration::from_secs(120);

#[test]
fn under_and_at_threshold() {
    let t0 = Instant::now();
    let (s, f) = check(Stall::default(), t0, T, t0 + Duration::from_secs(119));
    assert!(f.is_none() && !s.active());
    let (s, f) = check(s, t0, T, t0 + T);
    assert_eq!(f, Some(120.0));
    assert!(s.active());
}

#[test]
fn doubling_schedule() {
    let t0 = Instant::now();
    let at = |secs| t0 + Duration::from_secs(secs);
    let (s, f) = check(Stall::default(), t0, T, at(120));
    assert!(f.is_some());
    let (s, f) = check(s, t0, T, at(200));
    assert!(f.is_none());
    let (s, f) = check(s, t0, T, at(240));
    assert!(f.is_some());
    let (s, f) = check(s, t0, T, at(400));
    assert!(f.is_none());
    let (s, f) = check(s, t0, T, at(480));
    assert!(f.is_some());
    let (_, f) = check(s, t0, T, at(960));
    assert!(f.is_some());
}

#[test]
fn resume_and_rearm() {
    let t0 = Instant::now();
    let (s, _) = check(Stall::default(), t0, T, t0 + T);
    let (s, r) = on_activity(s, t0 + Duration::from_secs(151));
    assert_eq!(r, Some(151.0));
    assert!(!s.active());
    let (_, r) = on_activity(s, t0 + Duration::from_secs(152));
    assert!(r.is_none());
    // new activity time resets the timer
    let t1 = t0 + Duration::from_secs(151);
    let (_, f) = check(s, t1, T, t1 + Duration::from_secs(100));
    assert!(f.is_none());
}
