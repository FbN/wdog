use super::*;

#[test]
fn defaults_merge() {
    let p = merge_patterns(&["oops".into()], false);
    assert_eq!(p.len(), 8);
    assert_eq!(p[0], "FATAL");
    assert_eq!(p[2], "panic:");
    assert_eq!(p[7], "oops");
    let p = merge_patterns(&["oops".into()], true);
    assert_eq!(p, vec!["oops".to_string()]);
}

#[test]
fn defaults_are_literal() {
    let p = merge_patterns(&[], false);
    let set = regex::RegexSet::new(&p).unwrap();
    assert!(set.is_match("Segmentation fault (core dumped)"));
    assert!(!set.is_match("fatal error"));
}

#[test]
fn serde_round_trip() {
    let mut s = RunSpec::new(vec!["true".into(), "echo hi".into()]);
    s.max = Some(Duration::from_millis(1500));
    s.env = vec![("A".into(), "1".into())];
    let j = serde_json::to_string(&s).unwrap();
    assert!(j.contains("\"v\":1"));
    let back: RunSpec = serde_json::from_str(&j).unwrap();
    assert_eq!(back, s);
}

#[test]
fn step_names_fall_back_to_commands() {
    let mut s = RunSpec::new(vec!["true".into(), "echo hi".into()]);
    assert_eq!(s.step_name(1), "echo hi");
    assert!(!serde_json::to_string(&s).unwrap().contains("names"));
    s.names = vec!["t".into(), "greet".into()];
    assert_eq!(s.step_name(1), "greet");
    assert_eq!(s.step_name(5), "");
}
