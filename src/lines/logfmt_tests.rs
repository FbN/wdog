use super::*;

#[test]
fn round_trip() {
    let l = format("2", Stream::Out, "hello [x] y");
    assert_eq!(l, "[2] hello [x] y");
    assert_eq!(parse(&l), Some(("2", Stream::Out, "hello [x] y")));
    let l = format("3", Stream::Err, "oops");
    assert_eq!(l, "[3!] oops");
    assert_eq!(parse(&l), Some(("3", Stream::Err, "oops")));
}

#[test]
fn retry_label() {
    assert_eq!(label(2, 0), "2");
    assert_eq!(label(2, 1), "2.1");
    let l = format(&label(2, 1), Stream::Out, "x");
    assert_eq!(parse(&l), Some(("2.1", Stream::Out, "x")));
}

#[test]
fn empty_text_and_no_prefix() {
    assert_eq!(parse("[1] "), Some(("1", Stream::Out, "")));
    assert_eq!(parse("[1]"), Some(("1", Stream::Out, "")));
    assert_eq!(parse("no prefix"), None);
    assert_eq!(parse("[abc] x"), None);
    assert_eq!(raw("[1!] boom"), "boom");
    assert_eq!(raw("plain"), "plain");
}
