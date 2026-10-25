#!/usr/bin/env bash
set -euo pipefail
# Usage: desktop-screenshot [MAX_WIDTH] [X,Y,WIDTH,HEIGHT] [WINDOW_ID]
# stdout contains JPEG only; errors remain on stderr.
export DISPLAY=:0
max_width=${1:-0}
region=${2:-}
window=${3:-}
filters=()
if [[ -n $region ]]; then
    IFS=, read -r x y width height <<<"$region"
    filters+=(-crop "${width}x${height}+${x}+${y}" +repage)
fi
if [[ $max_width -gt 0 ]]; then
    filters+=(-resize "${max_width}x>")
fi
if [[ -n $window ]]; then
    xwd -id "$window" -silent
else
    xwd -root -silent
fi | convert xwd:- "${filters[@]}" -quality 80 jpeg:-
