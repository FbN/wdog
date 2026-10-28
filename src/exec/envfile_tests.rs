use super::*;

fn kv(k: &str, v: &str) -> (OsString, OsString) {
    (k.into(), v.into())
}

#[test]
fn parse_values_with_eq_newline_and_bytes() {
    let b = b"A=1\0B=x=y\0C=line1\nline2\0D=\xff\0bad\0=nokey\0";
    let v = parse_env0(b);
    assert_eq!(v.len(), 4);
    assert_eq!(v[0], kv("A", "1"));
    assert_eq!(v[1], kv("B", "x=y"));
    assert_eq!(v[2], kv("C", "line1\nline2"));
    assert_eq!(v[3].1.as_bytes(), b"\xff");
}

#[test]
fn diff_cases() {
    let before = vec![kv("SAME", "1"), kv("CHG", "a"), kv("GONE", "x"), kv("PWD", "/a")];
    let after = vec![
        kv("SAME", "1"),
        kv("CHG", "b"),
        kv("NEW", "n"),
        kv("PWD", "/b"),
        kv("SHLVL", "2"),
    ];
    let d = diff_env(&before, &after);
    assert_eq!(
        d,
        vec![
            EnvChange::Set("CHG".into(), "b".into()),
            EnvChange::Set("NEW".into(), "n".into()),
            EnvChange::Unset("GONE".into())
        ]
    );
}

#[test]
fn merge_precedence() {
    let ch = vec![
        EnvChange::Set("A".into(), "1".into()),
        EnvChange::Set("A".into(), "2".into()),
        EnvChange::Unset("B".into()),
    ];
    let m = merge(ch.clone(), &[]);
    assert!(m.contains(&EnvChange::Set("A".into(), "2".into())));
    assert!(m.contains(&EnvChange::Unset("B".into())));
    let m = merge(ch, &[("A".into(), "9".into()), ("B".into(), "b".into())]);
    assert!(m.contains(&EnvChange::Set("A".into(), "9".into())));
    assert!(
        m.contains(&EnvChange::Set("B".into(), "b".into())),
        "--env wins over an unset"
    );
}

#[test]
fn apply_and_keys() {
    let base = vec![kv("A", "1"), kv("B", "2")];
    let out = apply(
        &base,
        &[
            EnvChange::Unset("A".into()),
            EnvChange::Set("C".into(), "3".into()),
        ],
    );
    assert_eq!(out, vec![kv("B", "2"), kv("C", "3")]);
    assert_eq!(key_names(&[EnvChange::Unset("A".into())]), vec!["A"]);
}

#[test]
fn error_message_has_no_values() {
    let e = EnvError {
        path: "/x/.env".into(),
        what: "exit=2".into(),
    };
    assert_eq!(
        e.to_string(),
        "env-file /x/.env: exit=2 (check it with: sh -c 'set -a; . /x/.env')"
    );
}
