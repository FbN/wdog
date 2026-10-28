use super::*;

#[test]
fn fires_once_and_rearms() {
    let t0 = Instant::now();
    let th = Duration::from_secs(3);
    let (b, f) = on_sample(Busy::default(), true, 100, th, t0);
    assert!(f.is_none());
    let (b, f) = on_sample(b, true, 400, th, t0 + th);
    assert_eq!(f, Some((3.0, 300)));
    let (b, f) = on_sample(b, true, 500, th, t0 + th * 2);
    assert!(f.is_none());
    let b = on_output(b);
    assert_eq!(b, Busy::default());
}

#[test]
fn idle_resets() {
    let t0 = Instant::now();
    let th = Duration::from_secs(3);
    let (b, _) = on_sample(Busy::default(), true, 1, th, t0);
    let (b, _) = on_sample(b, false, 1, th, t0 + Duration::from_secs(2));
    let (_, f) = on_sample(b, true, 2, th, t0 + Duration::from_secs(4));
    assert!(f.is_none());
}
