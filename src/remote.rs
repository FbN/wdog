//! `wdog on HOST`: pure parts. Shell quoting, the two remote scripts, the agent file name and
//! the static-binary check (RM-1..RM-8).

/// Exit code of the run script when the agent is missing or the host is unsupported. It only
/// counts together with a `WDOG_ON ...` stderr line, so a command that exits 197 is never rerun.
pub const MISSING_CODE: i32 = 197;
pub const MARKER: &str = "WDOG_ON ";

/// Default remote base directory: the agent and jobs live in `<base>/wdog-<uid>`.
pub const DEFAULT_BASE: &str = "/tmp";

/// POSIX single-quoting: `it's` -> `'it'\''s'`. Safe for sh, bash, dash, zsh, ksh.
pub fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./=:,@%+".contains(&b))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Checks the agent and runs it with the user's arguments, or reports `WDOG_ON missing` /
/// `WDOG_ON unsupported <uname>` and exits 197. Args: `$1` base dir, `$2` agent file name, rest
/// the wdog command. One line, no single quotes, so the login shell sees one quoted string.
pub const RUN_SCRIPT: &str = concat!(
    r#"b=$1; f=$2; shift 2; "#,
    r#"s=$(uname -sm); if [ "$s" != "Linux x86_64" ]; then echo "WDOG_ON unsupported $s" >&2; exit 197; fi; "#,
    r#"h=$b/wdog-$(id -u); a=$h/bin/$f; "#,
    r#"if [ -d "$h" ] && [ ! -L "$h" ] && [ -O "$h" ] && [ ! -L "$h/bin" ] && [ -O "$h/bin" ] && [ -f "$a" ] && [ -O "$a" ] && [ -x "$a" ]; then "#,
    r#"WDOG_HOME=$h; export WDOG_HOME; exec "$a" "$@"; fi; "#,
    r#"echo "WDOG_ON missing" >&2; exit 197"#,
);

/// Reads the agent from stdin into `<base>/wdog-<uid>/bin/<name>` (temp file, size check,
/// rename), checks it runs, writes the `wdog` wrapper and removes other agent versions.
/// Args: `$1` base dir, `$2` agent file name, `$3` size in bytes. Reports
/// `WDOG_ON unsafe <dir>`, `upload_failed`, `noexec <dir>` or `installed <dir>` on stderr.
pub const INSTALL_SCRIPT: &str = concat!(
    r#"b=$1; f=$2; n=$3; h=$b/wdog-$(id -u); umask 077; "#,
    r#"mkdir -p "$h/bin" 2>/dev/null; "#,
    r#"for d in "$h" "$h/bin"; do if [ -L "$d" ] || [ ! -d "$d" ] || [ ! -O "$d" ]; then echo "WDOG_ON unsafe $d" >&2; exit 1; fi; done; "#,
    r#"chmod 700 "$h" "$h/bin" || exit 1; "#,
    r#"t=$h/bin/.up.$$; "#,
    r#"if ! cat > "$t" || [ "$(wc -c < "$t")" -ne "$n" ]; then rm -f "$t"; echo "WDOG_ON upload_failed" >&2; exit 1; fi; "#,
    r#"chmod 700 "$t" && mv -f "$t" "$h/bin/$f" || exit 1; "#,
    r#"if ! "$h/bin/$f" --version >/dev/null 2>&1; then echo "WDOG_ON noexec $h" >&2; exit 1; fi; "#,
    r#"w=$h/.wrap.$$; "#,
    r##"{ echo "#!/bin/sh"; echo "WDOG_HOME=\"$h\"; export WDOG_HOME; exec \"$h/bin/$f\" \"\$@\""; } > "$w" && chmod 700 "$w" && mv -f "$w" "$h/wdog" || exit 1; "##,
    r#"for o in "$h"/bin/wdog-*; do [ "$o" = "$h/bin/$f" ] || rm -f "$o"; done; "#,
    r#"echo "WDOG_ON installed $h" >&2"#,
);

/// The command string ssh sends: `exec sh -c '<script>' wdog-on <args...>`, each arg quoted.
pub fn remote_command(script: &str, args: &[String]) -> String {
    let mut s = format!("exec sh -c {} wdog-on", sh_quote(script));
    for a in args {
        s.push(' ');
        s.push_str(&sh_quote(a));
    }
    s
}

/// 64-bit FNV-1a: names the agent file so two builds of one version never mix.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// `wdog-0.5.0-1a2b3c4d5e6f`
pub fn agent_name(version: &str, bytes: &[u8]) -> String {
    format!("wdog-{version}-{:012x}", fingerprint(bytes) & 0xffff_ffff_ffff)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Elf {
    Static,
    /// Has a `PT_INTERP`: needs the local libc on the remote.
    Dynamic,
    NotX86_64,
    Unknown,
}

/// Is this a static x86_64 ELF (no program interpreter)?
pub fn check_elf(b: &[u8]) -> Elf {
    let u16_at = |o: usize| b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let u32_at = |o: usize| {
        b.get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    let u64_at = |o: usize| {
        b.get(o..o + 8).map(|s| {
            let mut a = [0u8; 8];
            a.copy_from_slice(s);
            u64::from_le_bytes(a)
        })
    };
    if b.len() < 64 || &b[0..4] != b"\x7fELF" {
        return Elf::Unknown;
    }
    if b[4] != 2 || b[5] != 1 || u16_at(18) != Some(62) {
        return Elf::NotX86_64;
    }
    let (Some(phoff), Some(phentsize), Some(phnum)) = (u64_at(32), u16_at(54), u16_at(56)) else {
        return Elf::Unknown;
    };
    for i in 0..usize::from(phnum) {
        let off = phoff as usize + i * usize::from(phentsize);
        match u32_at(off) {
            Some(3) => return Elf::Dynamic,
            Some(_) => {}
            None => return Elf::Unknown,
        }
    }
    Elf::Static
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StdinMode {
    /// `/dev/null`: the remote command does not read stdin, and ssh must not eat ours.
    Null,
    /// Read all of stdin first and send it on each attempt (`--steps-file -`).
    Buffer,
    /// Pass the terminal through, with `ssh -t` (`monitor`, `--stdin-tty`).
    Tty,
}

/// The wdog arguments before the `--` that starts the step commands.
fn opts(args: &[String]) -> impl Iterator<Item = &String> {
    args.iter().take_while(|a| a.as_str() != "--")
}

pub fn stdin_mode(args: &[String], stdin_is_tty: bool) -> StdinMode {
    let o: Vec<&String> = opts(args).collect();
    let steps_stdin = o.windows(2).any(|w| w[0] == "--steps-file" && w[1] == "-")
        || o.iter().any(|a| a.as_str() == "--steps-file=-");
    if steps_stdin && !stdin_is_tty {
        return StdinMode::Buffer;
    }
    let wants_tty =
        args.first().is_some_and(|a| a == "monitor") || o.iter().any(|a| a.as_str() == "--stdin-tty");
    if wants_tty && stdin_is_tty {
        StdinMode::Tty
    } else {
        StdinMode::Null
    }
}

pub fn wants_json(args: &[String]) -> bool {
    opts(args).any(|a| a == "--json")
}

/// `WDOG_ON <word> [rest]` lines from the remote's stderr.
#[derive(Debug, PartialEq, Eq)]
pub enum Marker {
    Missing,
    Unsupported(String),
    Unsafe(String),
    UploadFailed,
    Noexec(String),
    Installed(String),
}

pub fn parse_marker(line: &str) -> Option<Marker> {
    let rest = line.strip_prefix(MARKER)?;
    let (w, arg) = rest.split_once(' ').unwrap_or((rest, ""));
    let arg = arg.to_string();
    Some(match w {
        "missing" => Marker::Missing,
        "unsupported" => Marker::Unsupported(arg),
        "unsafe" => Marker::Unsafe(arg),
        "upload_failed" => Marker::UploadFailed,
        "noexec" => Marker::Noexec(arg),
        "installed" => Marker::Installed(arg),
        _ => return None,
    })
}

/// Host names for ssh: no leading `-` (would be an option), no spaces or control characters.
pub fn check_host(h: &str) -> Result<(), &'static str> {
    if h.is_empty() || h.starts_with('-') {
        return Err("must not be empty or start with '-'");
    }
    if h.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("must not contain spaces or control characters");
    }
    Ok(())
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
