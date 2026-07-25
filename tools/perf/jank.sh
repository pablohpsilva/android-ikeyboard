#!/usr/bin/env bash
# On-device jank measurement for the FeatherKey keyboard (com.featherkey).
# Usage: tools/perf/jank.sh <serial> [budget_pct]
# Precondition: a text field is focused and the FeatherKey keyboard is visible.
# The keyboard occupies the bottom band; taps/swipes below drive
# keystroke -> decode -> suggestion -> redraw cycles. Coordinates target the
# reference device (SM-A166B, 1080x2340); adjust for other screens.
set -uo pipefail
PKG=com.featherkey
SERIAL="${1:?usage: jank.sh <serial> [budget_pct]}"
BUDGET="${2:-5}"
adb() { command adb -s "$SERIAL" "$@"; }

# Ensure a focused text field (Settings search) so the keyboard is up.
adb shell am start -a android.settings.SETTINGS >/dev/null 2>&1; sleep 2
adb shell input keyevent 84 >/dev/null 2>&1; sleep 1   # SEARCH -> focuses a field on most Samsung builds

adb shell dumpsys gfxinfo "$PKG" reset >/dev/null 2>&1

# Fixed input sequence: 40 letter taps across the key band + 4 swipes (gesture typing).
KEYS_Y1=1900; KEYS_Y2=1980; KEYS_Y3=2060
for rep in $(seq 1 4); do
  for x in 90 210 330 450 570 690 810 930; do
    adb shell input tap "$x" "$KEYS_Y1" >/dev/null 2>&1
  done
  for x in 150 390 630 870; do
    adb shell input tap "$x" "$KEYS_Y2" >/dev/null 2>&1
  done
  adb shell input swipe 90 "$KEYS_Y2" 930 "$KEYS_Y2" 300 >/dev/null 2>&1  # swipe-to-type
done

OUT="$(adb shell dumpsys gfxinfo "$PKG" 2>/dev/null)"
total=$(echo "$OUT" | sed -n 's/.*Total frames rendered: \([0-9]*\).*/\1/p' | head -1)
janky=$(echo "$OUT" | sed -n 's/.*Janky frames: [0-9]* (\([0-9.]*\)%).*/\1/p' | head -1)
p95=$(echo "$OUT" | sed -n 's/.*95th percentile: \([0-9]*\)ms.*/\1/p' | head -1)
p99=$(echo "$OUT" | sed -n 's/.*99th percentile: \([0-9]*\)ms.*/\1/p' | head -1)
slowui=$(echo "$OUT" | sed -n 's/.*Number Slow UI thread: \([0-9]*\).*/\1/p' | head -1)

echo "total_frames=$total janky_pct=$janky p95_ms=$p95 p99_ms=$p99 slow_ui=$slowui budget_pct=$BUDGET"
awk -v j="${janky:-100}" -v b="$BUDGET" 'BEGIN{ exit !(j+0 <= b+0) }'
rc=$?
[ "$rc" -eq 0 ] && echo "JANK OK (<= ${BUDGET}%)" || echo "JANK OVER BUDGET (> ${BUDGET}%)"
exit "$rc"
