#!/bin/sh
# NF-2: CPU used by wdog while watching 50 sleeping processes (target < 1% of one core).
set -eu
W=${WDOG:-./target/release/wdog}
SECS=${SECS:-60}
$W run --interval 2s --max "${SECS}s" -- 'for i in $(seq 50); do sleep 600 & done; wait' >/dev/null &
sleep 1
pid=$(pgrep -n -f "wdog run --interval 2s --max ${SECS}s")
t0=$(awk '{print $14+$15}' /proc/$pid/stat)
sleep $((SECS - 3))
t1=$(awk '{print $14+$15}' /proc/$pid/stat)
hz=$(getconf CLK_TCK)
wait || true
awk -v a="$t0" -v b="$t1" -v hz="$hz" -v s="$((SECS - 3))" 'BEGIN { printf "wdog cpu: %.3f%% of one core\n", (b - a) / hz / s * 100 }'
