---
name: wdog
description: Run long or risky shell commands (builds, test suites, installs, migrations, dev servers, watchers, docker) under the wdog watchdog, so hangs, prompts, leftover processes and live errors are reported instead of silently eating time. Use whenever a command may take more than ~30 seconds, may wait for input, starts a server, or previously hung; and instead of `cmd > log 2>&1 &`, `sleep N; tail log`, `kill -0` loops or polling with `ps`.
---

# wdog — watchdog runner for commands

`wdog` runs your command, watches the whole process tree, and prints short `[wdog]` lines when something needs your attention. It works for one-off commands (`wdog run`) and background jobs (`wdog start` + `wdog wait`).

## When to use it

Use `wdog` for:

- builds, test suites, package installs, migrations, `docker build`, anything that may take > 30 s;
- servers and watchers (`npm run dev`, `python -m http.server`, `cargo watch`);
- commands that might ask a question (`apt`, `npx`, `git` with an editor, `ssh`, `gpg`);
- long work on another machine over ssh (use `wdog on HOST`, recipe below);
- any command that hung or timed out before.

Plain shell is fine for quick commands (`ls`, `git status`, `cat`, a single fast `grep`).

If `wdog` is not installed (`command not found`), don't install it yourself: tell the user and
suggest `cargo binstall wdog` (prebuilt static binary) or `cargo install wdog --features https`.
It runs on Linux only. Until then, run the command without wdog.

## Golden rules

1. **Wrap, don't background by hand.** Never write `cmd > log 2>&1 &`, `nohup`, `sleep 60; tail log`, `while kill -0 ...`. Use `wdog start` and `wdog wait`.
2. **Read the last `[wdog]` line, not just the exit code.** A step's own exit code can collide with wdog's codes (124–127). The last line says what happened: `DONE exit=<n> reason=<r>`, `ERROR <msg>`, or an alert.
3. **An alert means: stop waiting and look.** `STALL`, `PROMPT`, `ORPHANS`, `ERROR_LIVE`, `BUSY`, `SILENCE`, `LOST` end `wdog wait` early. Inspect with `wdog tail <id> -n 50` before doing anything else.
4. **Never answer a `PROMPT`.** Rerun non-interactively instead (`-y`, `--yes`, `--non-interactive`, `CI=1`, `GIT_EDITOR=true`, `DEBIAN_FRONTEND=noninteractive`).
5. **Stay under your tool timeout.** If your shell tool has a time limit (often 2–10 minutes), don't `wdog run` something that may outlast it. Use `wdog start`, then `wdog wait <id> --for <less than the limit>`, and repeat the wait.
6. **Clean up.** `wdog stop <id>` for every background job you started and no longer need. `wdog status` lists them.
7. **Quote each step as one argument** after `--`: `wdog run -- 'npm ci' 'npm test'`. Each step runs in `/bin/sh -c`; steps stop at the first failure. If steps need quotes, `$` or several lines, use `--steps-file -` with a quoted heredoc instead (recipe below).

## Recipes

### Build and test (foreground)

```bash
wdog run -- 'npm ci' 'npm run build' 'npm test'
```

Output for agents is quiet by default: only `[wdog]` lines, an error summary and the last 20 lines. Need more? `wdog tail <id> -n 200` (the id is on the first line).

### Many or complex steps: a script, no quoting

```bash
wdog run --steps-file - <<'EOF'
cd services/api
# -STEP- install
npm ci
# -STEP- test
npm test -- --grep 'api "v2"'
EOF
```

`# -STEP- <label>` lines split the script; the label is what `STEP` lines and `status --steps` show. Text before the first marker runs at the start of every step. Each step has `set -e`, so any failing line fails it. Works with `wdog start` too.

### Something that may outlast your tool timeout

```bash
wdog start --name build --max 45m -- 'cargo build --release' 'cargo test'
wdog wait build --for 8m      # repeat while it prints TIMEOUT (exit 124)
```

**Name your jobs** with `--name`: every command that takes an id also takes the name, so you don't
need to read the id first and can chain `start` and `wait` in one command. Starting again with the
same name replaces the finished job (it is removed); while the old one still runs, `start` refuses
(`wdog stop <name>` first). A name used from another directory is refused too: pick another one.
Names: letters, digits, `.` `_` `-`, not only hex digits (`cafe` looks like an id).

`wait` returns as soon as the job finishes or raises an alert, so the `--for` limit only caps one wait; it doesn't slow you down.

### Dev server: start, wait until ready, use it, stop it

```bash
wdog start --name dev -- 'npm run dev'
wdog wait dev --port 3000 --for 2m        # or --url http://localhost:3000/health, or --until 'ready in'
# ... run your checks against the server ...
wdog stop dev
```

If `wait` exits 1 instead of printing `READY`, the server crashed or hit an alert: `wdog tail dev -n 50`.

### Load a .env file for the commands

```bash
wdog run --env-file .env.development -- 'npm run migrate' 'npm test'
```

Same as `set -a && . ./.env.development && set +a` before each step, but your own shell stays unchanged. Values are never printed or logged; `--env K=V` overrides the file.

### Long work on another host (ssh)

Don't wrap `ssh host long-op` in a local `wdog run`: the work happens remotely, so wdog sees no
CPU, I/O or output and reports a false `STALL`. Run wdog on the host instead:

```bash
wdog on db01 start --name mig -- './migrate.sh --all'
wdog on db01 wait mig --for 8m        # repeat while TIMEOUT, as locally
wdog on db01 tail mig -n 50
wdog on db01 stop mig
```

Any wdog command works after `wdog on HOST`. If wdog is missing there, the first call installs it in
`/tmp/wdog-<uid>/` (printing `ON host=.. installed=.. path=..`) and runs the command; you never
install it yourself. Paths in the command are on the remote host; `--steps-file -` with a heredoc
sends the script from here. The exit code and `[wdog]` lines are the remote's. It uses key login
only (`BatchMode=yes`): if ssh fails with `Permission denied`, tell the user. Prefer `start` +
`wait` over `run` for long work: if ssh drops, the job keeps running and `wait` can be repeated.

### Pause while a job runs, but wake on trouble

```bash
wdog sleep 5m <id>     # exit 0 after 5 min, exit 1 early if the job raises an alert
```

### Check on everything

```bash
wdog status            # one line per job: id, state, runtime, idle seconds, step, last line
wdog status <id> --steps  # every step: ok/failed with exit and time, running, pending, not_run
wdog errors <id>       # error-pattern matches with log line numbers
wdog tail <id> -n 100  # last lines of the log
```

## Reading `[wdog]` lines

| Line | Meaning | What to do |
| --- | --- | --- |
| `id=<id> [name=<n>] [replaced=<old>] log=<path>` | job started (`replaced`: the finished job with that name was removed) | keep the id, or use the name |
| `ENV loaded=<n> file=<f>` | `.env` file loaded | nothing |
| `STEP 2/3 OK 88s <cmd or label>` / `STEP 2/3 FAIL exit=1 ...` | a step finished | on `FAIL`, read `ERRORS` and `TAIL` |
| `STALL 120s cpu=0.0s io=0B pids=3 last="..."` | no output, no CPU, no I/O for that long | likely hung: check `last=`, `wdog tail`; stop and fix (network, lock, waiting on a service) |
| `SILENCE 10m00s cpu=3.2s io=48MiB pids=4 last="..."` | only with `--silence`: no output for that long, but CPU/I/O may still move | `io`/`cpu` rising: probably working (download, copy); both near 0: likely stuck. Check `wdog tail <id>` |
| `RESUMED after 151s` | activity came back after a stall | keep waiting |
| `PROMPT line="Continue? [y/N]" via=text` | the command waits for input (`via=tty`: blocked reading the terminal) | stop it, rerun non-interactively (rule 4) |
| `ORPHANS pids=123,124 "sleep 600" \| ...` | main process exited but children still run | usually a daemon or a stray server: stop with `wdog stop <id>` unless you wanted it |
| `ERROR_LIVE log:<n> "..."` | an error line while the process still runs (`FATAL`, `Traceback`, `panic:`, `EADDRINUSE`, ...) | inspect now; it may never exit on its own |
| `ERRORS <n>: log:<line> "..." \| ...` | summary of error matches at the end | start debugging from these lines |
| `TAIL 20` / `HEAD 20` | the block that follows is the end / start of the output | read it |
| `SKIPPED <n> lines` | between `HEAD` and `TAIL` (both given): lines not shown | `wdog tail <id> -n <more>` if you need them |
| `TIMEOUT after=...` | `--max` (run) or `--for` (wait) reached | run: the job was killed; wait: the job still runs, wait again or stop it |
| `READY port=3000` | the `wait` condition is met | go on |
| `STOPPED sig=SIGTERM` | the job was stopped (`wdog stop`, Ctrl-C, a signal) | nothing, unless unexpected |
| `LOST killed=<n>` | the watcher itself died (killed, crashed); its tree was cleaned up (the step's main process dies with the watcher, `killed` counts only what was left after it, so 0 is normal) | rerun the job |
| `LOG_FULL at=64MiB` | the log hit its size cap; later lines are not on disk | the final `TAIL` is still correct; reduce output if you need the middle |
| `DONE exit=<n> reason=<r> <dur>` | the run ended | see exit codes below |
| `ON host=<h> installed=<v> path=<p>` | `wdog on` installed wdog on that host | nothing; `p` also works with plain `ssh <h> <p> ...` |
| `ERROR <msg>` | wdog itself failed (bad options, cannot start, bad `.env`, ssh/install failure with `wdog on`) | fix the command line |

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

## Useful options (`run` and `start`)

| Option | Use |
| --- | --- |
| `--max 30m` | hard time limit for the whole run; kills the tree, exit 124 |
| `--stall 300` | seconds of total inactivity before `STALL` (default 120). Raise it for steps that legitimately go quiet (big `docker build` layers, long compiles without output) |
| `--silence 10m` | alert after that long with no output even if CPU/I/O move (off by default). For steps that normally print regularly; a quiet network wait or download never `STALL`s |
| `--kill-on-stall` | stop the tree on the first `STALL` (exit 126) instead of only alerting |
| `--keep-going` | run all steps even if one fails |
| `--steps-file FILE` / `-` | steps from a script (file or stdin heredoc) split at `# -STEP- [label]` lines; replaces the `-- 'cmd'` arguments |
| `--name NAME` | use NAME in place of the id in `wait`, `tail`, `stop`, ...; reusing it replaces the finished job |
| `--cwd DIR`, `--env K=V`, `--env-file FILE` | working dir and environment for every step |
| `--err REGEX` | extra error pattern (repeatable); `--no-default-err` drops the built-ins |
| `--tail N` / `--head N` | how many output lines to show at the end (default: last 20) |
| `--live` | stream live output (default off when your stdout is not a terminal) |
| `--json` | events as JSON lines (`{"v":1,"ts":...,"event":"DONE",...}`); also on `wait`, `sleep`, `status`, `errors`, `stop`, `gc`, and for wdog's own `ERROR`. With `--live`, child output stays plain text: parse only lines starting with `{` |
| `--stdin-tty` | let the command read the terminal (humans only; agents have no terminal) |
| `--retry N` | rerun a failed step up to N times (flaky steps only) |

`wait` options: `--for D`, `--until REGEX`, `--port N`, `--url URL` (ready when status < 500; `https://` only in a wdog built with the `https` feature, otherwise exit 125), `--file PATH`. Durations: `45`, `45s`, `30m`, `2h`.

## Things to know

- **stdin is closed** by default: a command that asks for input fails fast instead of hanging. Pass flags like `--yes` up front.
- **Background processes die with `wdog run`.** Something a step starts in the background keeps running during the run, but usually dies once `wdog run` returns (its output pipe closes). Use `wdog start` for anything that must keep running.
- **`--steps-file` steps are separate shells.** Only the prelude (text before the first `# -STEP-`) is shared, copied into each step; a `cd` or `export` inside one step does not reach the next. A marker line inside a heredoc body within a step still splits it.
- **Port already in use:** `wait --port` only checks that something accepts connections. If an old server holds the port, `READY` can be a false positive; check `wdog status` and stop old jobs first.
- **Jobs are per user** and kept 24 h (`wdog gc` cleans up now). Logs live under `$WDOG_HOME/<id>/out.log`.
- `wdog monitor` is a full-screen live view for humans; it needs a terminal, so don't run it. Use `status`, `wait` and `tail`.
- If the watcher is killed (for example by your tool's timeout), its guard cleans up the tree and the job shows `lost`. Rerun with `wdog start`.
