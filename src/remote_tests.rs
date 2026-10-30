use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn quote_plain_and_special() {
    assert_eq!(sh_quote("status"), "status");
    assert_eq!(sh_quote("--for=2m"), "--for=2m");
    assert_eq!(sh_quote(""), "''");
    assert_eq!(sh_quote("a b"), "'a b'");
    assert_eq!(sh_quote("it's"), r"'it'\''s'");
    assert_eq!(sh_quote("$HOME"), "'$HOME'");
}

#[test]
fn scripts_have_no_single_quotes_or_newlines() {
    for sc in [RUN_SCRIPT, INSTALL_SCRIPT] {
        assert!(!sc.contains('\''), "{sc}");
        assert!(!sc.contains('\n'), "{sc}");
    }
}

#[test]
fn remote_command_quotes_each_arg() {
    let c = remote_command("echo hi", &s(&["/tmp", "run", "--", "echo 'x' $y"]));
    assert_eq!(c, r"exec sh -c 'echo hi' wdog-on /tmp run -- 'echo '\''x'\'' $y'");
}

#[test]
fn agent_name_depends_on_bytes() {
    let a = agent_name("0.5.0", b"one");
    assert!(a.starts_with("wdog-0.5.0-"));
    assert_eq!(a.len(), "wdog-0.5.0-".len() + 12);
    assert_eq!(a, agent_name("0.5.0", b"one"));
    assert_ne!(a, agent_name("0.5.0", b"two"));
}

/// A minimal ELF64 header with the given program header types.
fn elf(machine: u16, types: &[u32]) -> Vec<u8> {
    let mut b = vec![0u8; 64 + 56 * types.len()];
    b[0..4].copy_from_slice(b"\x7fELF");
    b[4] = 2;
    b[5] = 1;
    b[18..20].copy_from_slice(&machine.to_le_bytes());
    b[32..40].copy_from_slice(&64u64.to_le_bytes());
    b[54..56].copy_from_slice(&56u16.to_le_bytes());
    b[56..58].copy_from_slice(&(types.len() as u16).to_le_bytes());
    for (i, t) in types.iter().enumerate() {
        let o = 64 + 56 * i;
        b[o..o + 4].copy_from_slice(&t.to_le_bytes());
    }
    b
}

#[test]
fn elf_kinds() {
    assert_eq!(check_elf(&elf(62, &[6, 1, 1])), Elf::Static);
    assert_eq!(check_elf(&elf(62, &[6, 3, 1])), Elf::Dynamic);
    assert_eq!(check_elf(&elf(183, &[1])), Elf::NotX86_64);
    assert_eq!(check_elf(b"#!/bin/sh\necho hi\n"), Elf::Unknown);
    let mut cut = elf(62, &[1, 1]);
    cut.truncate(70);
    assert_eq!(check_elf(&cut), Elf::Unknown);
}

#[test]
fn stdin_modes() {
    assert_eq!(stdin_mode(&s(&["status"]), false), StdinMode::Null);
    assert_eq!(
        stdin_mode(&s(&["run", "--steps-file", "-"]), false),
        StdinMode::Buffer
    );
    assert_eq!(
        stdin_mode(&s(&["run", "--steps-file=-"]), false),
        StdinMode::Buffer
    );
    // after `--` it is a step command, not an option
    assert_eq!(
        stdin_mode(&s(&["run", "--", "--steps-file", "-"]), false),
        StdinMode::Null
    );
    assert_eq!(stdin_mode(&s(&["monitor"]), true), StdinMode::Tty);
    assert_eq!(stdin_mode(&s(&["monitor"]), false), StdinMode::Null);
    assert_eq!(
        stdin_mode(&s(&["run", "--stdin-tty", "--", "x"]), true),
        StdinMode::Tty
    );
}

#[test]
fn json_only_before_dashdash() {
    assert!(wants_json(&s(&["status", "--json"])));
    assert!(!wants_json(&s(&["run", "--", "x --json"])));
}

#[test]
fn markers() {
    assert_eq!(parse_marker("WDOG_ON missing"), Some(Marker::Missing));
    assert_eq!(
        parse_marker("WDOG_ON unsupported Linux aarch64"),
        Some(Marker::Unsupported("Linux aarch64".into()))
    );
    assert_eq!(
        parse_marker("WDOG_ON installed /tmp/wdog-1000"),
        Some(Marker::Installed("/tmp/wdog-1000".into()))
    );
    assert_eq!(parse_marker("WDOG_ON what"), None);
    assert_eq!(parse_marker("some output"), None);
}

#[test]
fn hosts() {
    assert!(check_host("db01").is_ok());
    assert!(check_host("me@db01.example.com").is_ok());
    assert!(check_host("-oProxyCommand=x").is_err());
    assert!(check_host("a b").is_err());
    assert!(check_host("").is_err());
}
