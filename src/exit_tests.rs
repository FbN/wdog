use super::*;

#[test]
fn table_rows() {
    assert_eq!(run_exit(&[0, 0], EndReason::Completed, false), Exit::Code(0));
    assert_eq!(run_exit(&[0, 1], EndReason::Completed, false), Exit::Code(1));
    assert_eq!(run_exit(&[127], EndReason::Completed, false), Exit::Code(127));
    assert_eq!(run_exit(&[0], EndReason::Max, false), Exit::Code(124));
    assert_eq!(run_exit(&[0], EndReason::KilledOnStall, false), Exit::Code(126));
    assert_eq!(
        run_exit(&[], EndReason::Stopped(Signal::SIGINT), false),
        Exit::Signal(Signal::SIGINT)
    );
}

#[test]
fn keep_going_uses_last_failure() {
    assert_eq!(run_exit(&[2, 0, 3, 0], EndReason::Completed, true), Exit::Code(3));
    assert_eq!(
        run_exit(&[2, 0, 3, 0], EndReason::Completed, false),
        Exit::Code(2)
    );
}

#[test]
fn recorded() {
    assert_eq!(recorded_exit(&Exit::Signal(Signal::SIGINT)), 130);
    assert_eq!(recorded_exit(&Exit::Signal(Signal::SIGHUP)), 129);
    assert_eq!(recorded_exit(&Exit::Signal(Signal::SIGTERM)), 143);
    assert_eq!(recorded_exit(&Exit::Code(3)), 3);
}

#[test]
fn signalled_step_is_a_code() {
    use nix::sys::wait::WaitStatus;
    use nix::unistd::Pid;
    let st = WaitStatus::Signaled(Pid::from_raw(1), Signal::SIGKILL, false);
    assert_eq!(status_code(st), 137);
    assert_eq!(run_exit(&[137], EndReason::Completed, false), Exit::Code(137));
}

#[test]
fn reasons() {
    assert_eq!(done_reason(&[0], EndReason::Completed), DoneReason::Ok);
    assert_eq!(done_reason(&[126], EndReason::Completed), DoneReason::StepFailed);
    assert_eq!(
        done_reason(&[0], EndReason::KilledOnStall),
        DoneReason::KillOnStall
    );
    assert_eq!(done_reason(&[0], EndReason::Max), DoneReason::Max);
    assert_eq!(
        done_reason(&[], EndReason::Stopped(Signal::SIGTERM)),
        DoneReason::Stopped
    );
}
