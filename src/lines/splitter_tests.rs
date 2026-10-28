use super::*;

#[test]
fn split_across_chunks() {
    let mut s = LineSplitter::new();
    assert_eq!(s.push(b"hel"), Vec::<String>::new());
    assert_eq!(s.partial(), "hel");
    assert_eq!(s.push(b"lo\nwor"), vec!["hello"]);
    assert_eq!(s.push(b"ld\n"), vec!["world"]);
    assert_eq!(s.partial(), "");
}

#[test]
fn crlf_even_split() {
    let mut s = LineSplitter::new();
    assert_eq!(s.push(b"a\r\nb\r"), vec!["a"]);
    assert_eq!(s.push(b"\nc"), vec!["b"]);
    assert_eq!(s.partial(), "c");
}

#[test]
fn progress_bar_replaces_partial() {
    let mut s = LineSplitter::new();
    assert!(s.push(b"10%\r20%\r30%").is_empty());
    assert_eq!(s.partial(), "30%");
    assert_eq!(s.push(b"\rdone\n"), vec!["done"]);
}

#[test]
fn invalid_utf8_is_lossy() {
    let mut s = LineSplitter::new();
    let v = s.push(b"a\xffb\n");
    assert_eq!(v, vec!["a\u{fffd}b"]);
}

#[test]
fn huge_line_is_split() {
    let mut s = LineSplitter::new();
    let big = vec![b'x'; MAX_LINE + 10];
    let v = s.push(&big);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].len(), MAX_LINE);
    assert_eq!(s.partial().len(), 10);
}

#[test]
fn empty_push_and_finish() {
    let mut s = LineSplitter::new();
    assert!(s.push(b"").is_empty());
    assert_eq!(s.finish(), None);
    s.push(b"LAST");
    assert_eq!(s.finish(), Some("LAST".to_string()));
    assert_eq!(s.finish(), None);
}

#[test]
fn empty_lines_kept() {
    let mut s = LineSplitter::new();
    assert_eq!(s.push(b"\n\n"), vec!["", ""]);
}
