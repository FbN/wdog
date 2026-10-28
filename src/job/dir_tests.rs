use super::*;

#[test]
fn home_choice() {
    assert_eq!(home_path(Some("/x"), Some("/run/user/1"), 5), PathBuf::from("/x"));
    assert_eq!(
        home_path(None, Some("/run/user/1"), 5),
        PathBuf::from("/run/user/1/wdog")
    );
    assert_eq!(home_path(Some(""), None, 5), PathBuf::from("/tmp/wdog-5"));
}

#[test]
fn safety_rule() {
    assert!(check_home(false, true, 5, 0o40700, 5).is_ok());
    assert!(check_home(true, true, 5, 0o700, 5).is_err());
    assert!(check_home(false, true, 6, 0o700, 5).is_err());
    assert!(check_home(false, true, 5, 0o770, 5).is_err());
    assert!(check_home(false, true, 5, 0o702, 5).is_err());
    assert!(check_home(false, false, 5, 0o700, 5).is_err());
}

#[test]
fn ids() {
    let id = new_id();
    assert_eq!(id.len(), 6);
    assert!(valid_id(&id));
    assert!(!valid_id("../x"));
    assert!(!valid_id(""));
}

#[test]
fn exclusive_job_dirs() {
    let t = tempfile::tempdir().unwrap();
    let a = new_job(t.path()).unwrap();
    let b = new_job(t.path()).unwrap();
    assert_ne!(a.id, b.id);
    assert!(job(t.path(), &a.id).is_ok());
    assert!(job(t.path(), "abcdef").is_err());
    assert_eq!(list_jobs(t.path()).len(), 2);
}
