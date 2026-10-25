#!/usr/bin/env bash
set -euo pipefail
test -f /run/desktop/ready
runuser -u desktop -- xdpyinfo -display :0 >/dev/null
if [[ ${DESKTOP_SESSION:-xfce} == gnome ]]; then
    pgrep -u desktop -x gnome-shell >/dev/null
    if pgrep -u desktop -f '^/usr/libexec/gnome-session-failed' >/dev/null; then
        echo 'GNOME session failure screen detected' >&2
        exit 1
    fi
fi
