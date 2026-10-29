//! `--steps-file`: a shell script split into steps at `# -STEP- [label]` lines.
//!
//! Text before the first marker is a prelude, copied to the start of every step. Each step
//! runs with `set -e`, so a failing command fails the step (`set +e` in the prelude undoes it).
//! Without markers the whole text is one step.

use anyhow::{Result, bail};

pub const MARKER: &str = "# -STEP-";

/// One step: the script given to `sh -c` and the short name shown in output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStep {
    pub script: String,
    pub name: String,
}

/// `Some(label)` for a marker line (label may be empty), `None` otherwise.
fn marker(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix(MARKER)?;
    if rest.is_empty() || rest.starts_with([' ', '\t']) {
        Some(rest.trim())
    } else {
        None
    }
}

/// Lines that run something: not blank, not a comment.
fn code_lines(body: &str) -> impl Iterator<Item = &str> {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

fn auto_name(body: &str) -> String {
    let mut it = code_lines(body);
    let first = it.next().unwrap_or_default();
    if it.next().is_some() {
        format!("{first} …")
    } else {
        first.to_string()
    }
}

pub fn parse(text: &str) -> Result<Vec<FileStep>> {
    let mut prelude = String::new();
    let mut parts: Vec<(String, String)> = vec![]; // (label, body)
    for line in text.lines() {
        if let Some(label) = marker(line) {
            parts.push((label.to_string(), String::new()));
            continue;
        }
        let buf = match parts.last_mut() {
            Some((_, body)) => body,
            None => &mut prelude,
        };
        buf.push_str(line);
        buf.push('\n');
    }
    if parts.is_empty() {
        if code_lines(&prelude).next().is_none() {
            bail!("--steps-file: no commands found");
        }
        parts.push((String::new(), std::mem::take(&mut prelude)));
    }
    let mut out = vec![];
    for (i, (label, body)) in parts.into_iter().enumerate() {
        if code_lines(&body).next().is_none() {
            bail!("--steps-file: step {} is empty", i + 1);
        }
        let name = if label.is_empty() { auto_name(&body) } else { label };
        out.push(FileStep {
            script: format!("set -e\n{prelude}{body}"),
            name,
        });
    }
    Ok(out)
}

#[cfg(test)]
#[path = "stepfile_tests.rs"]
mod tests;
