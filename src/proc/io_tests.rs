use super::*;

const REAL: &str = "rchar: 323934931\nwchar: 323929600\nsyscr: 632687\nsyscw: 632675\nread_bytes: 0\nwrite_bytes: 323932160\ncancelled_write_bytes: 0\n";

#[test]
fn real_sample() {
    assert_eq!(
        parse_io(REAL).unwrap(),
        IoCounters {
            rchar: 323934931,
            wchar: 323929600
        }
    );
}

#[test]
fn reordered_and_extra() {
    let s = "foo: 1\nwchar: 2\nrchar: 3\n";
    assert_eq!(parse_io(s).unwrap(), IoCounters { rchar: 3, wchar: 2 });
}

#[test]
fn missing_keys() {
    assert!(parse_io("rchar: 1\n").is_err());
    assert!(parse_io("").is_err());
}
