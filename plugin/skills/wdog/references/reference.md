# wdog reference

Everything `SKILL.md` leaves out: every `[wdog]` line, exit codes, options and details. Read the
part you need.

## Every `[wdog]` line

| Line | Meaning | What to do |
| --- | --- | --- |
| `id=<id> [name=<n>] [replaced=<old>] log=<path>` | job started (`replaced`: the finished job with that name was removed) | keep the id, or use the name |
| `ENV loaded=<n> file=<f>` | `.env` file loaded | nothing |
| `STEP 2/3 OK 88s <cmd or label>` / `STEP 2/3 FAIL exit=1 ...` | a step finished | on `FAIL`, read `ERRORS` and `TAIL` |
| `STALL 120s cpu=0.0s io=0B pids=3 last="..."` | no output, no CPU, no I/O for that long | likely hung: check `last=`, `wdog tail`; stop and fix (network, lock, waiting on a service) |
| `SILENCE 10m00s cpu=3.2s io=48MiB pids=4 last="..."` | only with `--silence`: no output for that long, but CPU/I/O may still move | `io`/`cpu` rising: probably working (download, copy); both near 0: likely stuck. Check `wdog tail <id>` |
| `BUSY 5m00s cpu=290.1s` | only with `--busy`: CPU in use with no output for that long | possible infinite loop: check `wdog tail <id>` |
| `RESUMED after 151s` | activity came back after a stall | keep waiting |
| `PROMPT line="Continue? [y/N]" via=text` | the command waits for input (`via=tty`: blocked reading the terminal) | stop it, rerun non-interactively |
| `ORPHANS pids=123,124 "sleep 600" \| ...` | main process exited but children still run | usually a daemon or a stray server: stop with `wdog stop <id>` unless you wanted it |
| `ERROR_LIVE log:<n> "..."` | an error line while the process still runs (`FATAL`, `Traceback`, `panic:`, `EADDRINUSE`, ...) | inspect now; it may never exit on its own |
| `ERRORS <n>: log:<line> "..." \| ...` | summary of error matches at the end | start debugging from these lines |
| `TAIL 20` / `HEAD 20` | the block that follows is the end / start of the output | read it |
| `SKIPPED <n> lines` | between `HEAD` and `TAIL` (both given): lines not shown | `wdog tail <id> -n <more>` if you need them |
| `RETRY 1/2 exit=1` | with `--retry`: a failed step is rerun | nothing |
| `TIMEOUT after=...` | `--max` (run) or `--for` (wait) reached | run: the job was killed; wait: the job still runs, wait again or stop it |
| `READY port=3000` | the `wait` condition is met | go on |
| `STOPPED sig=SIGTERM` | the job was stopped (`wdog stop`, Ctrl-C, a signal) | nothing, unless unexpected |
| `LOST killed=<n>` | the watcher itself died (killed, crashed); its tree was cleaned up (the step's main process dies with the watcher, `killed` counts only what was left after it, so 0 is normal) | rerun the job |
| `LOG_FULL at=64MiB` | the log hit its size cap; later lines are not on disk | the final `TAIL` is still correct; reduce output if you need the middle |
| `ON host=<h> installed=<v> path=<p>` | `wdog on` installed wdog on that host | nothing; `p` also works with plain `ssh <h> <p> ...` |
| `DONE exit=<n> reason=<r> <dur>` | the run ended | see exit codes below |
| `ERROR <msg>` | wdog itself failed (bad options, cannot start, bad `.env`, ssh or install failure with `wdog on`) | fix the command line |

## Exit codes

| Code | `wdog run` | `wdog wait` |
| --- | --- | --- |
| 0 | all steps succeeded (`reason=ok`) | condition met (`READY`) or job exited 0 |
| 1 | — | job failed, was stopped, was lost, or an alert woke it (see last line) |
| n | the failing step's own code (`reason=step_failed`) | — |
| 124 | `--max` reached (`reason=max`) | `--for` reached (`TIMEOUT`): the job keeps running |
| 125 | wdog itself failed (`ERROR` line) | wdog itself failed |
| 126 | `--kill-on-stall` fired (`reason=kill_on_stall`), **or** a step exited 126 (`reason=step_failed`) | — |
| 127 | command not found (from the shell) | — |
| 128+n | wdog was stopped by signal n (130 Ctrl-C, 143 SIGTERM) | — |

Codes 124–127 can also come from a step itself. The `reason=` on the `DONE` line settles it.
`wdog sleep` exits 0 after the time, 1 early on an alert. `wdog on` exits with the remote
command's code.

## Options for `run` and `start`

| Option | Use |
| --- | --- |
| `--name NAME` | use NAME in place of the id in `wait`, `tail`, `stop`, ...; reusing it replaces the finished job |
| `--steps-file FILE` / `-` | steps from a script (file or stdin heredoc) split at `# -STEP- [label]` lines; replaces the `-- 'cmd'` arguments |
| `--max 30m` | hard time limit for the whole run; kills the tree, exit 124 |
| `--stall 300` | seconds of total inactivity before `STALL` (default 120). Raise it for steps that legitimately go quiet (big `docker build` layers, long compiles without output) |
| `--silence 10m` | alert after that long with no output even if CPU/I/O move (off by default). For steps that normally print regularly; a quiet network wait or download never `STALL`s |
| `--busy 5m` | alert after that long of CPU use with no output (off by default), to catch infinite loops |
| `--kill-on-stall` | stop the tree on the first `STALL` (exit 126) instead of only alerting |
| `--keep-going` | run all steps even if one fails |
| `--retry N` | rerun a failed step up to N times (flaky steps only) |
| `--cwd DIR`, `--env K=V`, `--env-file FILE` | working dir and environment for every step |
| `--err REGEX` | extra error pattern (repeatable); `--no-default-err` drops the built-ins |
| `--tail N` / `--head N` | how many output lines to show at the end (default: last 20) |
| `--live` | stream live output (default off when your stdout is not a terminal) |
| `--json` | events as JSON lines (`{"v":1,"ts":...,"event":"DONE",...}`); also on `wait`, `sleep`, `status`, `errors`, `stop`, `gc`, and for wdog's own `ERROR`. With `--live`, child output stays plain text: parse only lines starting with `{` |
| `--stdin-tty` | let the command read the terminal (humans only; agents have no terminal) |

`wait` options: `--for D`, `--until REGEX`, `--port N`, `--url URL` (ready when status < 500;
`https://` only in a wdog built with the `https` feature, otherwise exit 125), `--file PATH`.
Durations: `45`, `45s`, `30m`, `2h`.

## Details

- **`--env-file`** works like `set -a && . ./FILE && set +a` before each step, but your own shell
  stays unchanged. Values are never printed or logged; `--env K=V` overrides the file.
- **Names:** letters, digits, `.` `_` `-`, not only hex digits (`cafe` looks like an id). A
  name used from another directory is refused: pick another one.
- **`--steps-file` steps are separate shells.** Only the prelude (text before the first
  `# -STEP-`) is shared, copied into each step; a `cd` or `export` inside one step does not reach
  the next. A marker line inside a heredoc body within a step still splits it.
- **Port already in use:** `wait --port` only checks that something accepts connections. If an
  old server holds the port, `READY` can be a false positive; check `wdog status` and stop old
  jobs first.
- **Jobs are per user** and kept 24 h (`wdog gc` cleans up now). Logs live under
  `$WDOG_HOME/<id>/out.log`.
- **`wdog on`:** jobs and the agent live in `/tmp/wdog-<uid>/` on the host, gone after a reboot.
  The remote must be Linux x86_64. If `/tmp` there is `noexec`, the `ERROR` says so: retry with
  `wdog on --dir DIR HOST ...` (an absolute directory on the host). `wdog on` refuses to install
  a non-static wdog: tell the user to install the release build (`cargo binstall wdog`).
- `wdog monitor` is a full-screen live view for humans; it needs a terminal, so don't run it.
