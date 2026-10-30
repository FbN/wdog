---
name: wdog
description: Run long or risky shell commands (builds, test suites, installs, migrations, dev servers, watchers, docker, long work on another host over ssh) under the wdog watchdog, so hangs, prompts, leftover processes and live errors are reported instead of silently eating time. Use whenever a command may take more than ~30 seconds, may wait for input, starts a server, or previously hung; and instead of `cmd > log 2>&1 &`, `sleep N; tail log`, `kill -0` loops or polling with `ps`.
---

# wdog — watchdog runner for commands

`wdog` runs your command, watches the whole process tree, and prints short `[wdog]` lines when something needs your attention. It works for one-off commands (`wdog run`) and background jobs (`wdog start` + `wdog wait`).

For every `[wdog]` line, exit code and option, read `references/reference.md` (next to this file).

## When to use it

- builds, test suites, package installs, migrations, `docker build`, anything that may take > 30 s;
- servers and watchers (`npm run dev`, `python -m http.server`, `cargo watch`);
- commands that might ask a question (`apt`, `npx`, `git` with an editor, `ssh`, `gpg`);
- long work on another machine over ssh (`wdog on HOST`, below);
- any command that hung or timed out before.

Plain shell is fine for quick commands (`ls`, `git status`, `cat`, a single fast `grep`).

If `wdog` is not installed (`command not found`), don't install it yourself: tell the user and
suggest `cargo binstall wdog` (prebuilt static binary) or `cargo install wdog --features https`.
It runs on Linux only. Until then, run the command without wdog.

## Golden rules

1. **Wrap, don't background by hand.** Never write `cmd > log 2>&1 &`, `nohup`, `sleep 60; tail log`, `while kill -0 ...`. Use `wdog start` and `wdog wait`.
2. **Read the last `[wdog]` line, not just the exit code.** A step's own exit code can collide with wdog's codes (124–127). The last line says what happened: `DONE exit=<n> reason=<r>` (`ok`, `step_failed`, `max`, `kill_on_stall`, `stopped`), `ERROR <msg>` (wdog itself failed, exit 125), or an alert.
3. **An alert means: stop waiting and look.** Alerts end `wdog wait` early. Inspect with `wdog tail <id> -n 50` before doing anything else.
4. **Never answer a `PROMPT`.** Rerun non-interactively instead (`-y`, `--yes`, `--non-interactive`, `CI=1`, `GIT_EDITOR=true`, `DEBIAN_FRONTEND=noninteractive`). stdin is closed, so a command that asks fails fast instead of hanging.
5. **Stay under your tool timeout.** If your shell tool has a time limit (often 2–10 minutes), don't `wdog run` something that may outlast it. Use `wdog start`, then `wdog wait <id> --for <less than the limit>`, and repeat the wait.
6. **Clean up.** `wdog stop <id>` for every background job you started and no longer need. `wdog status` lists them.
7. **Quote each step as one argument** after `--`: `wdog run -- 'npm ci' 'npm test'`. Each step runs in `/bin/sh -c`; steps stop at the first failure. If steps need quotes, `$` or several lines, use `--steps-file -` with a quoted heredoc instead.

## Alerts

| Line | Meaning | What to do |
| --- | --- | --- |
| `STALL 120s cpu=0.0s io=0B pids=3 last="..."` | no output, CPU or I/O for that long | likely hung (network, lock, waiting on a service): check `last=` and `wdog tail`, then stop and fix. Quiet but legit step? Raise `--stall` |
| `PROMPT line="Continue? [y/N]" via=text` | waiting for input | stop it, rerun non-interactively (rule 4) |
| `ORPHANS pids=123,124 "sleep 600" \| ...` | main process exited, children still run | a daemon or stray server: `wdog stop <id>` unless you wanted it |
| `ERROR_LIVE log:<n> "..."` | error line while the process still runs | inspect now; it may never exit on its own |
| `SILENCE` / `BUSY` | only with `--silence` / `--busy` | `wdog tail <id>`; details in the reference |
| `LOST killed=<n>` | the watcher was killed (e.g. by your tool timeout); its tree was cleaned up | rerun with `wdog start` |

## Recipes

### Build and test (foreground)

```bash
wdog run -- 'npm ci' 'npm run build' 'npm test'
```

Output for agents is quiet by default: only `[wdog]` lines, an error summary (`ERRORS`) and the last 20 lines (`TAIL`). Need more? `wdog tail <id> -n 200` (the id is on the first line).

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

`# -STEP- <label>` lines split the script; the label names the step in output. Text before the first marker runs at the start of every step; otherwise each step is its own shell (a `cd` in one step does not reach the next). Each step has `set -e`. Works with `wdog start` too.

### Something that may outlast your tool timeout

```bash
wdog start --name build --max 45m -- 'cargo build --release' 'cargo test'
wdog wait build --for 8m      # repeat while it prints TIMEOUT (exit 124)
```

**Name your jobs** with `--name`: every command that takes an id also takes the name, so you can chain `start` and `wait` in one command. Starting again with the same name replaces the finished job; while the old one still runs, `start` refuses (`wdog stop <name>` first). `wait` returns as soon as the job finishes or raises an alert, so `--for` only caps one wait.

### Dev server: start, wait until ready, use it, stop it

```bash
wdog start --name dev -- 'npm run dev'
wdog wait dev --port 3000 --for 2m        # or --url http://localhost:3000/health, or --until 'ready in'
# ... run your checks against the server ...
wdog stop dev
```

If `wait` exits 1 instead of printing `READY`, the server crashed or hit an alert: `wdog tail dev -n 50`.

### Long work on another host (ssh)

Don't wrap `ssh host long-op` in a local `wdog run`: the work happens remotely, so wdog sees no activity and reports a false `STALL`. Run wdog on the host:

```bash
wdog on db01 start --name mig -- './migrate.sh --all'
wdog on db01 wait mig --for 8m        # repeat while TIMEOUT, as locally
wdog on db01 tail mig -n 50
```

Any wdog command works after `wdog on HOST`. The first call installs wdog there by itself (`ON host=..` line); never install it yourself. Paths in the command are on the host; `--steps-file -` sends a local heredoc. Key login only: if ssh says `Permission denied`, tell the user. Use `start` + `wait` for long work: if ssh drops, the job keeps running.

### Other useful commands

```bash
wdog run --env-file .env.dev -- 'npm test'  # load .env for the steps; values never printed
wdog sleep 5m <id>        # sleep, but wake early (exit 1) if the job raises an alert
wdog status               # one line per job: id, state, runtime, idle, step, last line
wdog status <id> --steps  # every step: ok/failed with exit and time, running, pending
wdog errors <id>          # error-pattern matches with log line numbers
```

Handy `run`/`start` options: `--max 30m` (hard limit, exit 124), `--stall 300` (default 120 s), `--kill-on-stall`, `--keep-going`, `--cwd DIR`, `--env K=V`, `--json`. Full list in the reference.

## Things to know

- **Background processes die with `wdog run`** once it returns. Use `wdog start` for anything that must keep running.
- **`wait --port` false positive:** an old server holding the port also counts as ready; check `wdog status` and stop old jobs first.
- `wdog monitor` is for humans (needs a terminal): don't run it. Use `status`, `wait` and `tail`.
