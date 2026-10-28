use super::*;
use crate::spec::merge_patterns;

#[test]
fn default_patterns() {
    let m = ErrorMatcher::new(&merge_patterns(&[], false)).unwrap();
    for s in [
        "FATAL boom",
        "Traceback (most recent call last):",
        "thread 'main' panicked at x: panic: y",
        "Segmentation fault",
        "connect ECONNREFUSED 127.0.0.1:5432",
        "Error: listen EADDRINUSE: address already in use",
        "fatal: out of memory",
    ] {
        assert!(m.is_match(s), "{s}");
    }
    assert!(!m.is_match("all good"));
    assert!(!m.is_match("fatal error"), "case-sensitive");
}

#[test]
fn no_default_and_user_regex() {
    let m = ErrorMatcher::new(&merge_patterns(&["^E\\d+".into()], true)).unwrap();
    assert!(m.is_match("E42 broken"));
    assert!(!m.is_match("FATAL"));
    let none = ErrorMatcher::new(&[]).unwrap();
    assert!(!none.is_match("FATAL"));
}

#[test]
fn rate_limit_and_summary() {
    let t0 = Instant::now();
    let (e, f) = on_match(Errors::default(), 1, "a", true, t0);
    assert!(f);
    let (e, f) = on_match(e, 2, "b", true, t0 + Duration::from_secs(3));
    assert!(!f);
    let (e, f) = on_match(e, 3, "c", true, t0 + Duration::from_secs(11));
    assert!(f);
    let (e, f) = on_match(e, 4, "d", false, t0 + Duration::from_secs(30));
    assert!(!f, "not live after main exit");
    assert_eq!(e.count, 4);
    let mut e = e;
    for i in 0..10 {
        e = on_match(e, 10 + i, "x", false, t0).0;
    }
    assert_eq!(e.items.len(), SUMMARY_ITEMS);
    assert_eq!(e.count, 14);
}

#[test]
fn scan_log() {
    let m = ErrorMatcher::new(&merge_patterns(&[], false)).unwrap();
    let log = "[1] ok\n[1!] FATAL x\n[2] fine\n[2] panic: y\n";
    let got = scan(log.lines(), &m);
    assert_eq!(got, vec![(2, "FATAL x".to_string()), (4, "panic: y".to_string())]);
}
