# wdog plugin

This plugin teaches Claude to run long or risky shell commands (builds, test suites, installs,
migrations, dev servers, docker) under [wdog](https://github.com/FbN/wdog), a Linux watchdog
runner. Instead of waiting silently on a hung command, or improvising `cmd > log 2>&1 &` and
`sleep; tail` loops, Claude gets one short `[wdog]` line when a run stalls, waits for input,
leaves processes behind or prints errors while it keeps going, plus a compact summary at the end.
Background jobs get names, so Claude can start a server, wait for its port, read its output and
stop it with plain commands.

## Install

In Claude Code:

```
/plugin install wdog --marketplace FbN/wdog
```

Then install the `wdog` command itself, as described under Requirements.

## What it contains

One skill, `wdog`: instructions for when to use wdog and how to read its output. There are no
hooks, MCP servers or scripts. The plugin itself runs nothing and sends nothing anywhere.

## Requirements

The plugin needs the `wdog` command on your `PATH` (Linux only, no root needed):

```bash
cargo binstall wdog                  # prebuilt static binary
cargo install wdog --features https  # or build from source
```

If `wdog` is missing, Claude tells you and runs commands without it. Claude never installs it
on its own.

## What wdog does on your machine

wdog runs only the commands Claude asks it to run, with your user's permissions. It reads
`/proc` to watch the process tree and keeps each job's output and status in
`$XDG_RUNTIME_DIR/wdog/` (jobs older than 24 hours are removed). It makes no network
connections, except `wdog wait --url`, which checks an address the command line names.

## License

MIT, see [LICENSE](https://github.com/FbN/wdog/blob/main/LICENSE).
