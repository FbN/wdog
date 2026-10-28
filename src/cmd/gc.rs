use crate::console;
use crate::exit::Exit;
use crate::job::dir::wdog_home;
use anyhow::Result;

pub fn run(json: bool) -> Result<Exit> {
    let home = wdog_home()?;
    let n = crate::job::gc::run(&home);
    let _ = std::fs::write(home.join(".gc"), b"");
    if json {
        let j = serde_json::json!({ "removed": n });
        console::out(&crate::event::json_line("GC", j, std::time::SystemTime::now()));
    } else {
        console::out(&format!("[wdog] GC removed={n}"));
    }
    Ok(Exit::Code(0))
}
