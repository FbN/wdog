use super::*;

#[test]
fn no_marker_is_one_step() {
    let s = parse("cd x\nmake\n").unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].script, "set -e\ncd x\nmake\n");
    assert_eq!(s[0].name, "cd x …");
}

#[test]
fn prelude_labels_and_auto_names() {
    let text = "export A=1\n# -STEP- build\nmake\n  # -STEP-  \n# just a comment\nmake test\n";
    let s = parse(text).unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].name, "build");
    assert_eq!(s[0].script, "set -e\nexport A=1\nmake\n");
    assert_eq!(s[1].name, "make test");
    assert_eq!(s[1].script, "set -e\nexport A=1\n# just a comment\nmake test\n");
}

#[test]
fn multi_line_bodies_keep_shell_syntax() {
    let text = "# -STEP- wait db\nfor i in 1 2 3; do\n  true && break\ndone\n";
    let s = parse(text).unwrap();
    assert_eq!(s[0].script, "set -e\nfor i in 1 2 3; do\n  true && break\ndone\n");
}

#[test]
fn similar_comments_are_not_markers() {
    for line in ["# -STEP-x", "#-STEP-", "echo '# -STEP-'", "# -step-"] {
        let s = parse(&format!("{line}\ntrue\n")).unwrap();
        assert_eq!(s.len(), 1, "{line}");
    }
}

#[test]
fn empty_input_and_empty_steps() {
    for t in ["", "\n\n", "# only a comment\n"] {
        let e = parse(t).unwrap_err().to_string();
        assert!(e.contains("no commands"), "{t:?}: {e}");
    }
    let e = parse("# -STEP- a\ntrue\n# -STEP- b\n# nothing\n").unwrap_err();
    assert!(e.to_string().contains("step 2 is empty"), "{e}");
    let e = parse("set -x\n# -STEP-\n").unwrap_err();
    assert!(e.to_string().contains("step 1 is empty"), "{e}");
}
