use super::*;

const NORMAL: &str = "4242 (sleep) S 4200 4242 4100 34816 4242 4194304 103 0 0 0 7 3 0 0 20 0 1 0 987654 8445952 196 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0";

#[test]
fn normal_line() {
    let f = parse_stat(NORMAL).unwrap();
    assert_eq!(f.pid, 4242);
    assert_eq!(f.comm, "sleep");
    assert_eq!(f.state, 'S');
    assert_eq!(f.ppid, 4200);
    assert_eq!(f.pgrp, 4242);
    assert_eq!(f.utime, 7);
    assert_eq!(f.stime, 3);
    assert_eq!(f.starttime, 987654);
}

#[test]
fn comm_with_spaces_and_paren() {
    let s = NORMAL.replace("(sleep)", "(a ) b)");
    let f = parse_stat(&s).unwrap();
    assert_eq!(f.comm, "a ) b");
    assert_eq!(f.ppid, 4200);
    let s = NORMAL.replace("(sleep)", "(sd-pam)");
    assert_eq!(parse_stat(&s).unwrap().comm, "sd-pam");
}

#[test]
fn kernel_thread_and_zombie() {
    let k = "2 (kthreadd) S 0 0 0 0 -1 2129984 0 0 0 0 0 5 0 0 20 0 1 0 2 0 0 18446744073709551615 0 0 0 0 0 0 0 2147483647 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
    let f = parse_stat(k).unwrap();
    assert_eq!(f.ppid, 0);
    let z = NORMAL.replace(" S 4200", " Z 4200");
    assert_eq!(parse_stat(&z).unwrap().state, 'Z');
}

#[test]
fn bad_input() {
    assert!(parse_stat("4242 (sleep) S 1 2").is_err());
    assert!(parse_stat("").is_err());
    assert!(parse_stat(&NORMAL.replace(" 4200 ", " x ")).is_err());
}
