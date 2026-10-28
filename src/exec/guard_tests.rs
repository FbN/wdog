use super::*;

#[test]
fn final_state_kills_nothing() {
    let mut st = Status::initial("abc123", std::path::Path::new("/x"), 1, "true");
    for s in ["done", "failed", "timeout", "killed", "stopped", "lost"] {
        st.state = s.into();
        assert_eq!(guard_action(&st), GuardAction::Nothing, "{s}");
    }
}

#[test]
fn running_kills_recorded() {
    let mut st = Status::initial("abc123", std::path::Path::new("/x"), 1, "true");
    st.state = "running".into();
    st.pgids = vec![(10, 10, 100)];
    st.pids = vec![(11, 110)];
    assert_eq!(
        guard_action(&st),
        GuardAction::Kill {
            pgids: vec![(10, 10, 100)],
            pids: vec![(11, 110)]
        }
    );
    st.state = "starting".into();
    assert!(matches!(guard_action(&st), GuardAction::Kill { .. }));
}
