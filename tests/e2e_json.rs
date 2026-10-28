//! Phase 7.2: `--json` audit. Every stdout line of every command is a `"v":1` JSON object,
//! except live child output, which stays plain text.
#![allow(clippy::disallowed_methods)]

mod common;
use common::*;

/// Every stdout line parses as a v1 JSON object; returns them in order.
fn all_json(o: &Out) -> Vec<serde_json::Value> {
    assert!(!o.stdout.is_empty(), "no output; stderr: {}", o.stderr);
    o.stdout
        .lines()
        .map(|l| {
            let v: serde_json::Value =
                serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l:?}"));
            assert!(v.is_object(), "{l}");
            v
        })
        .collect()
}

fn event_line(o: &Out) -> serde_json::Value {
    let v = all_json(o);
    assert_eq!(v.len(), 1, "{}", o.stdout);
    let v = v.into_iter().next().unwrap();
    assert_eq!(v["v"], 1, "{v}");
    assert!(v["ts"].as_f64().is_some(), "{v}");
    v
}

#[test]
fn start_status_wait_sleep() {
    let e = Env::new();
    let o = e.run(&["start", "--json", "--", "sleep 0.3; echo ready; sleep 6501"]);
    assert_eq!(o.code, Some(0));
    let v = event_line(&o);
    assert_eq!(v["event"], "START");
    let id = v["id"].as_str().unwrap().to_string();
    assert!(v["log"].as_str().unwrap().ends_with("out.log"), "{v}");

    let o = e.run(&["wait", "--json", &id, "--until", "ready", "--for", "5s"]);
    assert_eq!(o.code, Some(0));
    let v = event_line(&o);
    assert_eq!(v["event"], "READY");

    let o = e.run(&["wait", "--json", &id, "--for", "300ms"]);
    assert_eq!(o.code, Some(124));
    assert_eq!(event_line(&o)["event"], "TIMEOUT");

    let o = e.run(&["sleep", "--json", "300ms", &id]);
    assert_eq!(o.code, Some(0));
    // a quiet sleep prints nothing or one JSON line, never text
    if !o.stdout.is_empty() {
        event_line(&o);
    }

    let o = e.run(&["status", "--json", &id]);
    let v = all_json(&o);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0]["id"], id.as_str());
    assert_eq!(v[0]["state"], "running");

    e.run(&["start", "--", "true"]);
    let o = e.run(&["status", "--json"]);
    assert_eq!(all_json(&o).len(), 2, "{}", o.stdout);

    let o = e.run(&["stop", "--json", &id]);
    assert_eq!(event_line(&o)["event"], "STOPPED");
}

#[test]
fn wait_and_sleep_alerts_as_json() {
    let e = Env::new();
    let id = e.start(&["--interval", "200ms", "--stall", "1", "--", "sleep 6502"]);
    let o = e.run(&["wait", "--json", &id, "--for", "10s"]);
    assert_eq!(o.code, Some(1));
    assert_eq!(event_line(&o)["event"], "STALL");
    e.run(&["stop", &id]);
    let o = e.run(&["wait", "--json", &id]);
    let v = event_line(&o);
    assert!(v["event"] == "DONE" || v["event"] == "STOPPED", "{v}");
}

#[test]
fn run_json_quiet_is_all_json() {
    let e = Env::new();
    let o = e.run(&["run", "--json", "--", "echo hi", "exit 2"]);
    assert_eq!(o.code, Some(2));
    let v = all_json(&o);
    assert_eq!(v.first().unwrap()["event"], "START");
    assert_eq!(v.last().unwrap()["event"], "DONE");
    assert!(v.iter().all(|x| x["v"] == 1));
}

#[test]
fn run_json_live_keeps_child_output_plain() {
    let e = Env::new();
    let o = e.run(&["run", "--json", "--live", "--", "echo plain-child-line"]);
    assert_eq!(o.code, Some(0));
    let (json, text): (Vec<&str>, Vec<&str>) = o.stdout.lines().partition(|l| l.starts_with('{'));
    for l in &json {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        assert_eq!(v["v"], 1);
    }
    assert!(
        text.iter().any(|l| l.contains("plain-child-line")),
        "{}",
        o.stdout
    );
    assert!(text.iter().all(|l| !l.starts_with("[wdog]")), "{}", o.stdout);
}

#[test]
fn wdog_errors_as_json() {
    let e = Env::new();
    for args in [
        vec!["wait", "--json", "nosuch"],
        vec!["status", "--json", "nosuch"],
        vec!["run", "--json", "--max", "3q", "--", "true"],
        vec!["start", "--json", "--bogus", "--", "true"],
        vec!["stop", "--json", "nosuch"],
    ] {
        let o = e.run(&args);
        assert_eq!(o.code, Some(125), "{args:?}");
        let v = event_line(&o);
        assert_eq!(v["event"], "ERROR", "{args:?}");
        assert!(!v["msg"].as_str().unwrap().is_empty());
    }
    // `--json` inside a step command is not wdog's flag
    let o = e.run(&["run", "--max", "3q", "--", "echo --json"]);
    assert!(o.stdout.starts_with("[wdog] ERROR"), "{}", o.stdout);
}
