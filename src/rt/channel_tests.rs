use super::*;
use std::time::Duration;

#[test]
fn ordering_and_close() {
    let (tx, rx) = channel();
    tx.send(1);
    tx.send(2);
    drop(tx);
    let got = async_io::block_on(async { (rx.recv().await, rx.recv().await, rx.recv().await) });
    assert_eq!(got, (Some(1), Some(2), None));
}

#[test]
fn wake_from_other_task() {
    let ex = async_executor::LocalExecutor::new();
    let out = async_io::block_on(ex.run(async {
        let (tx, rx) = channel();
        ex.spawn(async move {
            async_io::Timer::after(Duration::from_millis(20)).await;
            tx.send(7);
        })
        .detach();
        rx.recv().await
    }));
    assert_eq!(out, Some(7));
}

#[test]
fn send_before_first_poll() {
    let (tx, rx) = channel();
    let tx2 = tx.clone();
    tx2.send("a");
    drop(tx2);
    assert_eq!(async_io::block_on(rx.recv()), Some("a"));
    drop(tx);
    assert_eq!(async_io::block_on(rx.recv()), None);
}
