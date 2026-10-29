use super::*;

#[test]
fn fires_once_with_cpu_and_io_used() {
    let t0 = Instant::now();
    let th = Duration::from_secs(10);
    let (s, f) = on_sample(
        Silence::default(),
        t0,
        100,
        5_000,
        th,
        t0 + Duration::from_secs(2),
    );
    assert!(f.is_none());
    let (s, f) = on_sample(s, t0, 400, 9_000, th, t0 + th);
    assert_eq!(
        f,
        Some(Fire {
            after: 10.0,
            cpu_ticks: 300,
            io_bytes: 4_000
        })
    );
    let (s, f) = on_sample(s, t0, 500, 9_500, th, t0 + th * 3);
    assert!(f.is_none(), "once per quiet stretch");
    assert_eq!(on_output(s), Silence::default());
}

#[test]
fn fires_even_while_cpu_and_io_move() {
    let t0 = Instant::now();
    let th = Duration::from_secs(4);
    let mut s = Silence::default();
    let mut fired = 0;
    for i in 0..=6u64 {
        let (ns, f) = on_sample(s, t0, i * 100, i * 1_000, th, t0 + Duration::from_secs(i));
        s = ns;
        fired += f.is_some() as u32;
    }
    assert_eq!(fired, 1);
}

#[test]
fn output_rearms() {
    let t0 = Instant::now();
    let th = Duration::from_secs(4);
    let (s, f) = on_sample(Silence::default(), t0, 0, 0, th, t0 + th);
    assert!(f.is_some());
    let s = on_output(s);
    let later = t0 + th * 2;
    let (s, f) = on_sample(s, later, 0, 0, th, later + Duration::from_secs(1));
    assert!(f.is_none());
    let (_, f) = on_sample(s, later, 0, 0, th, later + th);
    assert!(f.is_some());
}
