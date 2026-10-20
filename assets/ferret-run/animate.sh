#!/usr/bin/env bash
# Play the ferret run cycle in the terminal with the xpressive CLI.
#
# usage: ./animate.sh [profile] [frame-delay-seconds]
#   profile: any XPR display profile id (default XPR-STD, 20px)
#   delay:   per-frame pause (default 0.10)
#
# Ctrl-C to stop.

set -euo pipefail

dir="$(cd "$(dirname "$0")" && pwd)"
xprc="${XPRC:-$HOME/xpressive/target/debug/xprc}"
profile="${1:-XPR-STD}"
delay="${2:-0.10}"

command -v "$xprc" >/dev/null 2>&1 || {
    echo "xprc not found at $xprc — build ~/xpressive or set XPRC" >&2
    exit 1
}

# Pre-render each frame's ANSI block once, then flip between them.
frames=()
for f in "$dir"/ferret-run-[1-6].xpr; do
    frames+=("$("$xprc" preview "$f" --profile "$profile")")
done

rows="$(printf '%s' "${frames[0]}" | wc -l)"
cleanup() { printf '\033[?25h\n'; }
trap cleanup EXIT INT TERM
printf '\033[?25l' # hide cursor

i=0
while true; do
    printf '\033[%dA' "$rows"   # rewind to the top of the previous frame
    printf '%s\n' "${frames[i]}"
    i=$(( (i + 1) % ${#frames[@]} ))
    sleep "$delay"
done
