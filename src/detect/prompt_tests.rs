use super::*;

#[test]
fn prompts() {
    let yes = [
        "Continue? (y/N) ",
        "Do you want to continue? [Y/n] ",
        "Overwrite? [y/N] ",
        "Password:",
        "[sudo] password for fbn: ",
        "Enter passphrase for key '/home/u/.ssh/id_ed25519': ",
        "Username for 'https://github.com': ",
        "Password for 'https://u@github.com': ",
        "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
        "Ok to proceed? (y) ",
        "Proceed (Y/n)? ",
        "rm: remove regular file 'x'? ",
        "Press any key to continue:",
        "Is this ok [y/N]: ",
        ">>> ",
        "Shall I go on?",
        "Login: ",
        "Please confirm (y):",
    ];
    for s in yes {
        assert!(looks_like_prompt(s), "should be a prompt: {s:?}");
    }
}

#[test]
fn not_prompts() {
    let no = [
        "Compiling foo v1.0 (path)",
        "45% [=====>",
        "Error: x",
        "Error:",
        "Running tests:",
        "Downloading (12.3 MB)",
        "",
        "   ",
        "Building...",
        "Step 1/7 : FROM node",
        "progress [####] ETA 3s",
        "status: [ok]",
    ];
    for s in no {
        assert!(!looks_like_prompt(s), "should not be a prompt: {s:?}");
    }
    assert!(!looks_like_prompt(&format!("{}?", "x".repeat(250))));
}

#[test]
fn text_grace_and_rearm() {
    let t0 = Instant::now();
    let g = Duration::from_secs(5);
    let p = Prompt::default();
    let (p, f) = check_text(p, Some(("Continue? [y/N] ", t0)), g, t0 + Duration::from_secs(4));
    assert!(!f);
    let (p, f) = check_text(p, Some(("Continue? [y/N] ", t0)), g, t0 + g);
    assert!(f);
    let (p, f) = check_text(p, Some(("Continue? [y/N] ", t0)), g, t0 + g * 2);
    assert!(!f, "once per episode");
    let p = rearm(p);
    let (_, f) = check_text(p, Some(("Continue? [y/N] ", t0)), g, t0 + g * 3);
    assert!(f);
    let (_, f) = check_text(Prompt::default(), None, g, t0 + g);
    assert!(!f);
}

#[test]
fn tty_path() {
    let t0 = Instant::now();
    let g = Duration::from_secs(5);
    let (p, f) = check_tty(Prompt::default(), true, t0, g, t0 + Duration::from_secs(1));
    assert!(!f);
    let (p, f) = check_tty(p, true, t0, g, t0 + Duration::from_secs(6));
    assert!(f);
    let (p, _) = check_tty(p, false, t0, g, t0 + Duration::from_secs(7));
    assert_eq!(p.tty_since, None);
    // output during the blocked window delays it
    let (p, _) = check_tty(Prompt::default(), true, t0, g, t0);
    let (_, f) = check_tty(
        p,
        true,
        t0 + Duration::from_secs(4),
        g,
        t0 + Duration::from_secs(6),
    );
    assert!(!f);
}
