use super::*;
use std::io::Write;

#[test]
fn split() {
    let mut b = b"a\nb\nc".to_vec();
    assert_eq!(split_complete(&mut b), vec!["a", "b"]);
    assert_eq!(b, b"c");
    let mut b = b"x".to_vec();
    assert!(split_complete(&mut b).is_empty());
}

#[test]
fn incremental_and_truncation() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("f");
    std::fs::write(&p, "1\n2\npart").unwrap();
    let mut t = Tailer::from_start(&p);
    assert_eq!(t.read_lines(), vec!["1", "2"]);
    let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
    f.write_all(b"ial\n3\n").unwrap();
    assert_eq!(t.read_lines(), vec!["partial", "3"]);
    assert!(t.read_lines().is_empty());
    std::fs::write(&p, "new\n").unwrap();
    assert_eq!(t.read_lines(), vec!["new"]);
    let mut e = Tailer::from_end(&p);
    assert!(e.read_lines().is_empty());
}

#[test]
fn last_n() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("f");
    let content: String = (1..=5000).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&p, content).unwrap();
    assert_eq!(
        last_lines(&p, 3).unwrap(),
        vec!["line 4998", "line 4999", "line 5000"]
    );
    assert_eq!(last_lines(&p, 10000).unwrap().len(), 5000);
    std::fs::write(&p, "").unwrap();
    assert!(last_lines(&p, 3).unwrap().is_empty());
    std::fs::write(&p, "a\nb").unwrap();
    assert_eq!(last_lines(&p, 1).unwrap(), vec!["b"]);
}

#[test]
fn lines_before_a_fixed_end() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("f");
    std::fs::write(&p, "a\nb\nc\nd\n").unwrap();
    // the first 6 bytes are "a\nb\nc\n"
    assert_eq!(
        lines_before(&p, 6, 2).unwrap(),
        (vec!["b".into(), "c".into()], false)
    );
    assert_eq!(
        lines_before(&p, 6, 10).unwrap(),
        (vec!["a".into(), "b".into(), "c".into()], true)
    );
    assert_eq!(lines_before(&p, 0, 5).unwrap(), (vec![], true));
    assert_eq!(lines_before(&p, 99, 1).unwrap(), (vec!["d".into()], false));
}
