# wdog — Watchdog Runner for Coding Agents: Functional Specification

Oct 28, 2026 · @Fabiano Taioli

## 1. Goal

`wdog` is a Linux command-line wrapper that a coding agent puts in front of any long-running shell command. It does two things:

1. **Raises a flag when a run looks anomalous**: no progress, waiting for keyboard input, leftover processes, or error output while the process stays alive. The flag tells the agent to stop waiting and look at what is happening. `wdog` does not decide what to do about it.
2. **Removes shell boilerplate** that agents write again and again: redirecting to a log, `tee`, sleep-and-check loops, command sequences with exit-code tracking, and cleanup of process trees.

Success means:

- An agent never waits more than the configured stall threshold on a stuck command without being told.
- A typical long-running call fits on one line.
- What the agent reads back is a short summary, not the full output.

## 2. Problem

Coding agents run builds, test suites, installs, migrations and dev servers, then wait for them to exit before reading the result. An agent cannot see the terminal, so it doesn't notice what a person would see in seconds. Four situations are common:

| Situation | What the agent experiences | Typical cause |
| --- | --- | --- |
| Silent stall | The command never returns, with no output | Deadlock, waiting on a lock or an unreachable network peer |
| Waiting for input | The command never returns; the last line is a question | `(y/N)` prompt, pager, password prompt, editor opened by `git` |
| Leftover children | The main process exited, but the call still doesn't return | A daemon or watcher keeps the output pipe open |
| Live failure | Error lines appear, but the process keeps running | Dev server crashed internally, test runner hanging after a failure |

The cost is wall-clock time, tokens spent on repeated status checks, and user interruptions. Existing tools only cover part of this:

- `timeout(1)` and similar enforce a total runtime, not progress.
- CI "no output" watchdogs (CircleCI, Jenkins, project scripts) look only at output and live inside one CI platform.
- Agents' built-in background tasks and output monitors react to lines a command prints. They cannot report silence, tell a busy-but-quiet process from a stuck one, or detect a prompt.

On top of this, agents write fragile shell plumbing every time: `> log 2>&1 &`, `echo $!`, `sleep 30; tail`, `kill -0`, grep for errors. That costs tokens on every call and introduces mistakes.

## 3. Scope, users and assumptions

**Primary user:** a coding agent that runs shell commands through a Bash-like tool. **Secondary user:** the human supervising the agent, who wants a quick look at what is running.

**Assumptions**

- Linux only. `wdog` reads `/proc` for process trees, CPU time, I/O counters and wait states.
- Node.js is available on the machine. `wdog` ships as a single file with no npm dependencies.
- Alerts are advisory. A false positive costs the agent one quick check, which is acceptable. A missed stall costs far more.
- `wdog` does not kill anything on its own initiative, except when the caller explicitly asks for it (`--max`, `--kill-on-stall`, `wdog stop`).
- Commands are run through `/bin/sh -c`, so the agent passes each step as a normal shell string.

## 4. Functional requirements

Priorities: **Must** for the first version, **Should** soon after, **Could** if cheap.

### 4.1 Execution and sequences

| ID | Requirement | Priority |
| --- | --- | --- |
| EX-1 | Run one command or an ordered sequence of commands given as separate arguments after `--`. | Must |
| EX-2 | In a sequence, stop at the first failing step by default; `--keep-going` runs all steps. | Must |
| EX-3 | Report each step's exit code and duration. | Must |
| EX-4 | `--cwd DIR` and `--env K=V` (repeatable) apply to all steps. | Must |
| EX-5 | Each run starts its own process group so the whole tree can be tracked and stopped. | Must |
| EX-6 | stdin is closed (`/dev/null`) by default, so a command that asks for input fails fast instead of hanging; `--stdin-tty` turns this off. | Must |
| EX-7 | `--max DURATION` (e.g. `30m`) stops the whole tree after a total time limit. | Must |
| EX-8 | `--retry N` re-runs a failed step up to N times. | Could |
| EX-9 | `--env-file FILE` (repeatable) loads a `.env` file into the environment of every step, as `set -a && . FILE && set +a` would. The file is sourced once per run by `/bin/sh`; later files override earlier ones and `--env K=V` overrides both. The variables exist only in the steps' environment: the caller's shell is not changed, and values are never written to the console, the log or the job files (`.env` files often hold secrets). A file that is missing or fails to load stops the run before the first step (exit 125). | Should |
| EX-10 | `--steps-file FILE` (`-` = stdin, e.g. a quoted heredoc `<<'EOF'`) reads the steps from a shell script instead of `--` arguments, so they need no quoting. Lines that are exactly `# -STEP-` (optionally followed by a label) split the script into steps; without such lines the whole script is one step. Text before the first marker is a prelude copied to the start of every step (each step is still its own `/bin/sh`, so `cd` and variables do not carry over otherwise). Every step starts with `set -e`, so any failing command fails the step; `set +e` in the prelude turns this off. Output shows the label, or the step's first command followed by `…`. With `-` the steps' stdin is `/dev/null`, and `--stdin-tty` is refused. An empty script or an empty step stops the run before the first step (exit 125). | Should |

### 4.2 Logging and output capture

| ID | Requirement | Priority |
| --- | --- | --- |
| LG-1 | stdout and stderr of every step always go to a log file; default `$WDOG_HOME/<id>/out.log` (see BG-7), overridable with `--log FILE`. | Must |
| LG-2 | Each log line is prefixed with the step number; stderr lines are marked. | Should |
| LG-3 | `--tail N` and `--head N` choose what is printed to the console at the end (default: last 20 lines). | Must |
| LG-4 | `--quiet` prints only `wdog` event lines and the final summary (`ERRORS`, the `TAIL`/`HEAD` block, `DONE`), not the live output. `--quiet` is the default when stdout is not a terminal, which is how agents run commands, so a long build does not fill the agent's context. `--live` forces live output; on a terminal live output stays the default. | Must |
| LG-5 | A status file `$WDOG_HOME/<id>/status.json` holds state, PIDs, start time, last activity, current step and last lines. | Must |

### 4.3 Anomaly detection

| ID | Requirement | Priority |
| --- | --- | --- |
| AN-1 | **Stall:** alert when, for `--stall SECONDS` (default 120), there has been no new output and no change in CPU time or I/O counters across the whole process tree. | Must |
| AN-2 | CPU time is the sum of `utime + stime` from `/proc/<pid>/stat` over all descendants; I/O is the sum of `rchar + wchar` (covers files and sockets) from `/proc/<pid>/io`. | Must |
| AN-3 | Process trees are rebuilt on every sample by following parent PIDs, so new grandchildren are included. | Must |
| AN-4 | **Waiting for input:** alert when the last output line has no trailing newline and looks like a question, or a tree member is blocked reading a terminal. Fires after a short grace period (default 5 s), not after the stall threshold. | Must |
| AN-5 | **Leftover children:** alert when the main process has exited but descendants are still alive after a grace period (default 5 s); list their PIDs and command lines. | Must |
| AN-6 | **Error pattern while alive:** alert when a line matches an error pattern and the process is still running. Built-in patterns are conservative (`FATAL`, `Traceback`, `panic:`, `Segmentation fault`, `ECONNREFUSED`, `EADDRINUSE`, `out of memory`); `--err REGEX` adds more; `--no-default-err` drops the built-ins. | Must |
| AN-7 | Each alert fires once per episode and re-arms after activity resumes. A stall that continues re-alerts at doubling intervals. | Should |
| AN-8 | Alerts never stop the process unless `--kill-on-stall` is given. Leftover children (AN-5) are not killed either: `wdog` keeps reading their output into the log until the run ends. When `wdog run` returns, its output pipes close, so a leftover child that writes output after that gets `SIGPIPE` and usually dies. Processes that must keep running belong in `wdog start`. | Must |
| AN-9 | **High CPU with no output:** optional alert after `--busy SECONDS` of CPU use with no output, to catch infinite loops. Off by default. | Could |
| AN-10 | **No output:** optional alert after `--silence DURATION` with no output, whatever CPU and I/O do (a process that keeps reading or writing but prints nothing never stalls). It reports the CPU time and I/O used during the quiet stretch, the number of processes and the last line, fires once per quiet stretch, and re-arms on output or at the next step. It wakes `wait` and `sleep` like the other alerts and never stops anything (`--kill-on-stall` ignores it). Off by default: many steps are legitimately quiet for long. | Could |

### 4.4 Waiting

| ID | Requirement | Priority |
| --- | --- | --- |
| WT-1 | `wdog wait <id>` blocks until the job exits, raises an alert, or hits `--for DURATION`, whichever comes first, and says which. | Must |
| WT-2 | `--until REGEX` returns as soon as a matching line appears in the log. | Must |
| WT-3 | `--port N` returns when a TCP port accepts connections; `--url URL` when an HTTP request returns a status below 500; `--file PATH` when a file exists. | Must |
| WT-4 | `wdog sleep DURATION [<id>]` is a plain sleep that also wakes early on any alert from the named job, or from any job when no id is given. | Should |

### 4.5 Background jobs

| ID | Requirement | Priority |
| --- | --- | --- |
| BG-1 | `wdog start` launches a run detached from the calling shell and prints its id immediately. All `run` options apply. | Must |
| BG-2 | `wdog status [<id>]` prints one line per job: id, state, runtime, seconds since last activity, current step, last output line. With `--steps`, it also prints one line per step: `ok`/`failed` (exit code, duration), `running`, `pending`, `not_run` (the job ended first) or `interrupted` (the watcher died during it), plus the attempt count with `--retry`. | Must |
| BG-3 | `wdog tail <id> [-n N] [-f]` shows the log; `-f` follows it (for humans). | Must |
| BG-4 | `wdog stop <id>` sends SIGTERM to the whole tree, then SIGKILL after `--grace` (default 5 s). | Must |
| BG-5 | `wdog errors <id>` lists error-pattern matches with log line numbers. | Should |
| BG-6 | Job directories older than 24 hours are cleaned up on the next `wdog` call; `wdog gc` does it on demand. | Should |
| BG-7 | Job directories live in `$WDOG_HOME/<id>/`. `WDOG_HOME` defaults to `$XDG_RUNTIME_DIR/wdog`, or `/tmp/wdog-$UID` when `XDG_RUNTIME_DIR` is unset, so users never share it. It is created with mode `0700`; `wdog` refuses to use it (exit 125) if it is a symlink, owned by another user, or writable by group or others. | Must |
| BG-8 | `wdog monitor` is a live full-screen view for humans, like `top`. On the left, one row per job (newest start first, so rows don't move while jobs run): id, name, state with any alerts of a live job, idle time, current step, run time; alerts are coloured. On the right, the selected job's output, following new lines (`PgUp`/`PgDn` scroll and pause following, `Home` goes to the top, `End` follows again, `←`/`→` pan long lines); `o` switches it to the job's steps (state, exit code, duration and script of each). `↑`/`↓` pick a job, `q`, `Esc` or Ctrl-C quit. It refreshes twice a second, only reads job files, and never changes a job. Escape sequences and control characters in the output are removed before display. Without a terminal it exits 125 with an `ERROR` line. | Could |
| BG-9 | `--name NAME` on `run` and `start` names the job; every command that takes a job id also takes its name. Names use letters, digits, `.`, `_`, `-` (1–32, not starting with `.` or `-`) and cannot be only hex digits. A name belongs to the directory the job runs in (`--cwd`, or the current one). Starting a job whose name points to a finished (or lost) job started in the same directory removes that job and takes the name; the start line shows `name=<n> replaced=<old id>`. If the job still runs, or was started in another directory, `start` and `run` exit 125 with an `ERROR` line. `status` shows `name=`, and `status.json` has `"name"`. | Should |

## 5. Interface, output and exit codes

### 5.1 Commands

```bash
wdog run   [opts] -- 'cmd1' ['cmd2' ...]   # foreground, returns when done
wdog start [opts] -- 'cmd1' ['cmd2' ...]   # background, prints <id>
wdog start --name NAME ...                 # then use NAME in place of <id>
wdog run|start [opts] --steps-file FILE|-  # steps from a script split at '# -STEP-' lines
wdog wait  <id> [--for 2m] [--until RE] [--port N] [--url URL] [--file PATH]
wdog sleep 30s [<id>]
wdog status [<id>] [--steps]
wdog tail  <id> [-n 50] [-f]
wdog errors <id>
wdog stop  <id> [--grace 5s]
wdog gc
wdog monitor                             # live view of all jobs (humans)
```

Shared options: `--name`, `--cwd`, `--env`, `--env-file`, `--log`, `--tail`, `--head`, `--quiet`, `--live`, `--stall`, `--err`, `--no-default-err`, `--max`, `--kill-on-stall`, `--keep-going`, `--json`. Durations accept `s`, `m`, `h`; a bare number means seconds.

### 5.2 Output format

Every line `wdog` writes itself starts with ` [wdog]  ` followed by an upper-case event word, so an agent or the Monitor tool can filter them with one pattern. Example run:

```
[wdog] id=a7f3 log=/run/user/1000/wdog/a7f3/out.log
[wdog] STEP 1/3 OK   41s  npm ci
[wdog] STEP 2/3 OK   88s  npm run build
[wdog] STALL 120s cpu=0.0s io=0B pids=3 last="Running 214 tests"
[wdog] RESUMED after 151s
[wdog] STEP 3/3 FAIL exit=1 112s  npm test
[wdog] ERRORS 2: log:842 "FAIL src/api.test.js" | log:851 "Expected 200, received 500"
[wdog] TAIL 20
...
[wdog] DONE exit=1 reason=step_failed 241s
```

Other event words: `ENV` (one per `--env-file`, e.g. `ENV loaded=12 file=.env.development`; key names only with `--json`), `PROMPT`, `ORPHANS`, `ERROR_LIVE`, `TIMEOUT`, `READY` (from `wait`), `STOPPED`, `LOST` (the watcher died and its guard cleaned up, see NF-4). With `--json`, each event is one JSON object per line instead.

### 5.3 Exit codes

| Code | Meaning |
| --- | --- |
| Same as the failing step | A step failed (or 0 when all steps succeeded) |
| 124 | `--max` or `wait --for` limit reached |
| 125 | `wdog` itself failed (bad arguments, cannot spawn) |
| 126 | Stopped by `--kill-on-stall` |
| 127 | Command not found (passed through from the shell) |
| 0 / 1 for `wait` | 0 = condition met or job exited 0; 1 = job failed or an alert woke it (the reason is in the last `[wdog]` line) |

A step's own exit code can be 124, 125, 126 or 127 too (126 is common: "permission denied" from the shell), so the code alone is ambiguous. The last `[wdog]` line always says what happened: `DONE exit=<n> reason=<r>` with `reason` one of `ok`, `step_failed`, `max`, `kill_on_stall`, `stopped`; or `ERROR <msg>` when `wdog` itself failed (125). **Agents should read the last `[wdog]` line, not just the exit code.** The agent snippet (NF-8) says so.

## 6. Non-functional requirements

| ID | Requirement |
| --- | --- |
| NF-1 | Linux kernel 4.x or later; Node.js 18 or later; no npm dependencies; one executable file. |
| NF-2 | Sampling every 2 s by default (`--interval`); overhead below 1% of one CPU core for a tree of 50 processes. |
| NF-3 | Live output passes through with no added buffering beyond one line. |
| NF-4 | `wdog` survives the target's crash and never leaves its own watcher process behind. If `wdog` itself dies without cleaning up (killed with `SIGKILL`, e.g. by an agent harness timeout, or crashed), a small guard process started with each run, in its own session, kills the job's process tree, marks the job `lost` and writes a `LOST` event. The guard cannot help if the whole session or cgroup is killed; `wdog stop <id>` then cleans up using the recorded process groups. |
| NF-5 | Works without root. Processes the user cannot read in `/proc` are counted as "alive, activity unknown" rather than as stalled. |
| NF-6 | Code style: functional JavaScript with JSDoc type annotations. Pure functions for parsing `/proc`, building trees, detecting prompts and deciding alerts; side effects (spawning, timers, file writes) kept in a thin outer layer. |
| NF-7 | Each pure function has unit tests; each anomaly type has an end-to-end test with a deliberately broken command. |
| NF-8 | A ready-made 10-line snippet for the agent's instructions file (or an agent skill) that tells the agent when and how to use `wdog`. |

## 7. Out of scope, open questions and acceptance tests

**Out of scope for the first version:** macOS and Windows; diagnosing why a process is stuck (stack dumps, `strace`); bugs inside the agent harness itself, where no child process is ever started; remote hosts and containers other than the current one.

**Open questions**

- [ ] Is 120 s the right default stall threshold, or should it scale with the command (e.g. longer for `docker build`)?
- [ ] Decided 2026-10-28: stdin defaults to /dev/null (EX-6); --stdin-tty keeps the terminal attached when needed.
- [ ] Should alerts also go to a desktop or phone notification for the human, or stay agent-only?
- [x] Decided 2026-10-28: the tool is named `wdog`. `wd` clashed with the `node-wikibase-cli` command on Debian/Ubuntu and with oh-my-zsh's `wd` plugin.

**Acceptance tests** (each must produce the stated event within the stated time)

| Test command | Expected event |
| --- | --- |
| `sleep 600` with `--stall 10` | `STALL` within 12 s |
| `read -p 'Continue? (y/N) ' x` with `--stdin-tty` | `PROMPT` within 7 s |
| `sh -c 'sleep 600 &'` | `ORPHANS` within 7 s of the main process exiting |
| `sh -c 'echo FATAL boom; sleep 600'` | `ERROR_LIVE` within 3 s |
| A busy loop printing nothing, `--stall 10` | No `STALL` (CPU is changing) |
| `wdog start -- 'python3 -m http.server 8123'` then `wdog wait <id> --port 8123` | `READY` within 5 s |
| `wdog run --env-file .env -- 'test "$FOO" = bar'` with `FOO=bar` in `.env` | Exit 0 and an `ENV loaded=1` line; `FOO` still unset in the caller's shell; `bar` appears in no `[wdog]` line or job file |
| `wdog run -- 'true' 'false' 'echo never'` | Step 3 not run; exit code 1 |
| `wdog stop <id>` on a tree of 3 processes | All 3 gone within the grace period |
| `kill -9` the `wdog run` process of `sh -c 'sleep 600 & sleep 600'` | Both `sleep` gone within 3 s; `wdog status` shows `lost`; `LOST` in the job's events |
| `wdog run -- 'sh -c "sleep 600 &"'` | `ORPHANS`; the orphan is still alive after `wdog run` returns (not killed, AN-8) |
