use super::*;

#[test]
fn combinations() {
    let pts = Path::new("/dev/pts/3");
    assert_eq!(classify('S', Some("n_tty_read"), Some(pts)), TtyRead::Yes);
    assert_eq!(classify('S', Some("wait_woken"), Some(pts)), TtyRead::Yes);
    assert_eq!(classify('S', Some("do_select"), Some(pts)), TtyRead::No);
    assert_eq!(classify('S', Some("0"), Some(pts)), TtyRead::No);
    assert_eq!(classify('T', Some("do_signal_stop"), Some(pts)), TtyRead::Yes);
    assert_eq!(
        classify('S', Some("n_tty_read"), Some(Path::new("/dev/null"))),
        TtyRead::No
    );
    assert_eq!(classify('S', Some("n_tty_read"), None), TtyRead::Unknown);
    assert_eq!(
        classify('S', None, Some(Path::new("/dev/tty1"))),
        TtyRead::Unknown
    );
}
