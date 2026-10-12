#!/usr/bin/env bash
set -euo pipefail
export DISPLAY=:0
# stdout contains JPEG only; errors remain on stderr.
xwd -root -silent | convert xwd:- -resize '640x360>' -quality 75 jpeg:-

