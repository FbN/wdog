use super::*;

#[test]
fn old_and_dead_only() {
    let now = SystemTime::now();
    let old = now - Duration::from_secs(25 * 3600);
    let jobs = vec![
        ("a".to_string(), false, old),
        ("b".to_string(), true, old),
        ("c".to_string(), false, now - Duration::from_secs(3600)),
    ];
    assert_eq!(to_remove(&jobs, now), vec!["a".to_string()]);
}
