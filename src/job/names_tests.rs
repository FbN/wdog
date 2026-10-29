use super::*;

#[test]
fn name_rules() {
    for ok in ["test", "build-1", "api.v2", "my_job", "Deploy", "a1g", "x"] {
        assert!(valid_name(ok), "{ok}");
    }
    for bad in [
        "",
        "-x",
        ".x",
        "a b",
        "a/b",
        "cafe",
        "123",
        "abc123",
        &"n".repeat(33),
    ] {
        assert!(!valid_name(bad), "{bad}");
    }
    assert!(check_name("cafe").unwrap_err().contains("job id"));
}

#[test]
fn decide_rules() {
    let here = Path::new("/p");
    let l = Link {
        id: "abc123".into(),
        dir: here.into(),
    };
    assert_eq!(decide(None, OldJob::Missing, here), Claim::Free);
    assert_eq!(decide(Some(&l), OldJob::Missing, here), Claim::Free);
    assert_eq!(
        decide(Some(&l), OldJob::Ended, here),
        Claim::Replace("abc123".into())
    );
    assert_eq!(
        decide(Some(&l), OldJob::Live, here),
        Claim::Running("abc123".into())
    );
    let other = Path::new("/q");
    assert_eq!(
        decide(Some(&l), OldJob::Ended, other),
        Claim::Elsewhere("abc123".into(), here.into())
    );
}

fn ended_job(home: &Path, id: &str) {
    let p = JobPaths::new(home, id);
    fs::create_dir(&p.dir).unwrap();
    let mut st = status::Status::initial(id, &p.log, 1, "true");
    st.state = "done".into();
    status::write_atomic(&p.status, &st).unwrap();
}

#[test]
fn claim_replace_and_prune() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    let here = h.canonicalize().unwrap();
    ended_job(h, "aaa111");
    assert_eq!(claim(h, "t", &here, "aaa111").unwrap(), None);
    assert_eq!(resolve(h, "t").as_deref(), Some("aaa111"));
    ended_job(h, "bbb222");
    assert_eq!(claim(h, "t", &here, "bbb222").unwrap().as_deref(), Some("aaa111"));
    assert!(!h.join("aaa111").exists());
    assert_eq!(resolve(h, "t").as_deref(), Some("bbb222"));
    let other = h.join("sub");
    fs::create_dir(&other).unwrap();
    assert!(claim(h, "t", &other, "ccc333").is_err());
    fs::remove_dir_all(h.join("bbb222")).unwrap();
    prune(h);
    assert_eq!(resolve(h, "t"), None);
}
