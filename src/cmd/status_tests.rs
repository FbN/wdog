use super::*;

#[test]
fn line_format() {
    let mut st = Status::initial("3fa91c", std::path::Path::new("/x/out.log"), 3, "npm test");
    st.state = "running".into();
    st.started_at = 1000.0;
    st.last_activity_at = 1239.0;
    st.step = 2;
    st.last_lines = vec!["Running 214 tests".into()];
    let l = status_line(&st, 1242.0);
    assert_eq!(
        l,
        "3fa91c running   4m02s idle=3s    step=2/3 last=\"Running 214 tests\""
    );
    st.state = "failed".into();
    st.ended_at = Some(1100.0);
    st.exit = Some(1);
    let l = status_line(&st, 5000.0);
    assert!(l.contains("failed"));
    assert!(l.contains("1m40s"));
    assert!(l.contains("idle=-"));
    assert!(l.contains("exit=1"));
}
