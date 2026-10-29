# wdog

A Linux watchdog runner for long shell commands, built for coding agents. `wdog` runs your
command, logs everything, watches the whole process tree and prints short `[wdog]` lines when a
run looks wrong: no progress (`STALL`), waiting for input (`PROMPT`), leftover processes
(`ORPHANS`), or error output while it keeps running (`ERROR_LIVE`). It also replaces the usual
`> log 2>&1 &`, `sleep; tail`, `kill -0` plumbing with `start`, `wait`, `status` and `stop`.

Spec: [`doc/functional-spec.md`](doc/functional-spec.md). Plan: [`doc/plan/`](doc/plan/00-overview.md).
Agent skill: [`doc/skill/wdog/SKILL.md`](doc/skill/wdog/SKILL.md).

## Install

Linux only (reads `/proc`). No root needed. Needs Rust 1.85 or newer.

```bash
cargo install --git https://github.com/FbN/wdog wdog
```

`wait --url` speaks plain `http://` by default. For `https://`, add the `https` feature (rustls
with bundled root certificates, no OpenSSL needed, about 1.3 MB larger):

```bash
cargo install --git https://github.com/FbN/wdog wdog --features https
```

From a clone: `cargo install --path .` (same `--features https` option).

### Arch Linux

The PKGBUILD builds the latest `main` from GitHub, with https included:

```bash
mkdir wdog-pkg && cd wdog-pkg
curl -LO https://raw.githubusercontent.com/FbN/wdog/main/packaging/arch/PKGBUILD
makepkg -si                   # add --check to run the test suite during the build
```

This installs `/usr/bin/wdog`, the docs in `/usr/share/doc/wdog/` and the agent skill in
`/usr/share/wdog/skill/wdog/` (copy that folder into your agent's skills folder). Set
`_https=0` in the PKGBUILD for a build without https. Update by running `makepkg -si` again;
remove with `sudo pacman -R wdog-git`.

### Static binaries

musl builds that run on any x86_64 Linux:

```bash
rustup target add x86_64-unknown-linux-musl
./scripts/release.sh          # dist/wdog and dist/wdog-https; checks they are static
./scripts/release.sh --test   # also runs the whole test suite against the musl build
```

## Commands

```bash
wdog run   [opts] -- 'cmd1' ['cmd2' ...]   # foreground, returns when done
wdog start [opts] -- 'cmd1' ['cmd2' ...]   # background, prints [wdog] id=<id> log=<path>
wdog run|start [opts] --steps-file FILE|-  # steps from a script, see below
wdog wait  <id> [--for 2m] [--until RE] [--port N] [--url URL] [--file PATH] [--json]
wdog sleep 30s [<id>] [--json]
wdog status [<id>] [--steps] [--json]   # --steps: one line per step
wdog tail  <id> [-n 50] [-f] [--raw]
wdog errors <id> [--json]
wdog stop  <id> [--grace 5s] [--json]
wdog gc [--json]
```

Options for `run` and `start`: `--steps-file`, `--cwd`, `--env K=V`, `--env-file FILE`, `--log`, `--tail N`,
`--head N`, `--quiet`, `--live`, `--stall`, `--err REGEX`, `--no-default-err`, `--max`,
`--kill-on-stall`, `--keep-going`, `--json`, `--stdin-tty`, `--interval`, `--busy`, `--retry N`.
Durations: `45`, `45s`, `30m`, `2h`, `200ms`.

### Steps from a script

`--steps-file FILE` (or `-` for stdin) takes a normal shell script, split into steps at
`# -STEP- [label]` lines. Nothing needs quoting, and loops or multi-line commands work:

```bash
wdog run --stall 2m --steps-file - <<'EOF'
cd services/api
# -STEP- install
npm ci
# -STEP- db
docker compose up -d db
for i in $(seq 30); do pg_isready -h localhost && break; sleep 1; done
# -STEP- test
npm test -- --grep 'api "v2"'
EOF
```

- Text before the first marker (`cd services/api` above) is copied to the start of every step.
  Each step is still its own `/bin/sh`.
- Every step runs with `set -e`: any failing command fails the step. Put `set +e` in the prelude
  to turn that off.
- Output shows the label (`STEP 2/3 OK 4s  db`), or the step's first command plus `…`.
- No marker: the whole script is one step. With `-`, the steps' stdin is `/dev/null`.

## Output

```
[wdog] id=a7f3c1 log=/run/user/1000/wdog/a7f3c1/out.log
[wdog] STEP 1/3 OK   41s  npm ci
[wdog] STEP 2/3 OK   1m28s  npm run build
[wdog] STALL 2m00s cpu=0.0s io=0B pids=3 last="Running 214 tests"
[wdog] RESUMED after 2m31s
[wdog] STEP 3/3 FAIL exit=1 1m52s  npm test
[wdog] ERRORS 2: log:842 "FAIL src/api.test.js" | log:851 "Expected 200, received 500"
[wdog] TAIL 20
...
[wdog] DONE exit=1 reason=step_failed 4m01s
```

Live output is shown only on a terminal (or with `--live`); when stdout is a pipe, as for
agents, only `[wdog]` lines and the final summary are printed. `--json` prints events as JSON
lines (`{"v":1,"ts":...,"event":"DONE",...}`), for every command, including wdog's own `ERROR`
(exit 125). Live child output is never wrapped: with `--json --live` it stays plain text
between the JSON lines, so parse only lines that start with `{`. Every job keeps `out.log`, `events.jsonl` and `status.json` in `$WDOG_HOME/<id>/`.

## Exit codes

| Code | Meaning |
| --- | --- |
| failing step's code | a step failed (0 when all succeeded) |
| 124 | `--max` reached (`run`), `--for` reached (`wait`) |
| 125 | wdog itself failed (`[wdog] ERROR ...`) |
| 126 | `--kill-on-stall` fired |
| 127 | command not found (from the shell) |
| 128+n | wdog was stopped by signal n (it re-raises the signal) |
| `wait`: 0 / 1 | 0 = condition met or job exited 0; 1 = job failed, stopped, lost, or an alert woke it |

A step can exit 124–127 itself, so read the last `[wdog]` line: `DONE exit=<n> reason=<ok|step_failed|max|kill_on_stall|stopped>`.

## Agent snippet

See [`doc/agent-snippet.md`](doc/agent-snippet.md) for a 10-line block to paste into your agent's instructions file, or install
the skill by copying `doc/skill/wdog/` into your agent's skills folder.

## Limits

- **Orphans of `wdog run`:** a background process started by a step keeps running and is
  logged during the run, but `wdog run` closes its output pipes when it returns, so an orphan
  that prints after that gets `SIGPIPE` and usually dies. Use `wdog start` for anything that must
  keep running. `wdog stop <id>` can still clean up the orphans `wdog run` recorded.
- **Guard:** each run has a small guard process in its own session. If `wdog` is killed with
  `SIGKILL` (for example by an agent's tool timeout), the guard kills the job's tree and marks
  the job `lost`. It cannot help if the whole session or cgroup is killed; `wdog stop <id>` then
  cleans up through the recorded process groups.
- **Job folder:** `WDOG_HOME`, default `$XDG_RUNTIME_DIR/wdog`, else `/tmp/wdog-$UID`, mode 0700.
  wdog refuses a symlink, another owner, or group/world write. Jobs older than 24 h are removed.
- **Log cap:** `out.log` stops growing at `WDOG_LOG_MAX` bytes (default 64 MiB) and `LOG_FULL` is
  printed once; the final `TAIL` block still shows the real end of the output.
- **Throughput:** around 190 MB/s for typical log lines; floods of very short lines (`yes`) go
  through at about 5 M lines/s.
- **`--env-file`** is sourced by `/bin/sh` once per run; values never reach the console, the log
  or the job files, and the loader's own errors are hidden because they can quote the file.

## Development

```bash
./scripts/check.sh            # fmt, clippy -D warnings, tests (cargo-nextest if installed)
SECS=60 ./scripts/bench-tree.sh   # NF-2: CPU while watching 50 processes
```

Unit tests sit next to each module (`foo.rs` + `foo_tests.rs`); end-to-end tests are in
`tests/`. The `spike/` package holds the throwaway checks of the Linux process behaviour
(results in `doc/plan/spike-results.md`).

## Author and license

Author: Fabiano Taioli <ftaioli@gmail.com>. MIT license, see [`LICENSE`](LICENSE).
