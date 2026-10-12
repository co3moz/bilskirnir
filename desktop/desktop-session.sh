#!/usr/bin/env bash
set -euo pipefail
export HOME=/home/desktop USER=desktop XDG_RUNTIME_DIR=/run/desktop
export DISPLAY=:0 PULSE_SERVER=unix:/run/desktop/pulse/native
cleanup() {
    rm -f /run/desktop/ready
    local pids
    pids=$(jobs -pr)
    if [[ -n "$pids" ]]; then kill $pids 2>/dev/null || true; fi
    wait || true
}
trap cleanup EXIT
trap 'exit 0' TERM INT
mkdir -p "$HOME/.config" "$HOME"/{Desktop,Documents,Downloads,Music,Pictures,Videos}
if command -v xdg-user-dirs-update >/dev/null; then xdg-user-dirs-update; fi
# Stale X locks live in tmpfs, not in snapshots.
Xtigervnc :0 -geometry "${DESKTOP_RESOLUTION:-1920x1080}" -depth 24 \
    -rfbport 5900 -localhost no -SecurityTypes None -AlwaysShared -AcceptSetDesktopSize=1 &
vnc_pid=$!
for _ in {1..100}; do
    if xdpyinfo -display :0 >/dev/null 2>&1; then break; fi
    kill -0 "$vnc_pid" || exit 1
    sleep .1
done
xdpyinfo -display :0 >/dev/null
if [[ ${DESKTOP_AUDIO:-false} == true ]]; then
    mkdir -p /run/desktop/pulse
    pulseaudio --daemonize=no --exit-idle-time=-1 --disallow-exit --use-pid-file=no \
        --load="module-native-protocol-unix socket=/run/desktop/pulse/native auth-anonymous=1" &
    pulse_pid=$!
    for _ in {1..100}; do
        if pactl info >/dev/null 2>&1; then break; fi
        kill -0 "$pulse_pid" || exit 1
        sleep .1
    done
    pactl load-module module-null-sink sink_name=desktop-output rate=48000 channels=2 >/dev/null
    pactl set-default-sink desktop-output
    if [[ ${DESKTOP_MICROPHONE:-false} == true ]]; then
        pactl load-module module-null-sink sink_name=browser-mic-sink rate=48000 channels=1 >/dev/null
        pactl load-module module-remap-source master=browser-mic-sink.monitor source_name=browser-microphone channels=1 >/dev/null
        pactl set-default-source browser-microphone
    fi
fi
case ${DESKTOP_SESSION:-xfce} in
    xfce) session_command=(startxfce4) ;;
    gnome) session_command=(/usr/local/bin/desktop-gnome) ;;
    *) echo "Unknown DESKTOP_SESSION: ${DESKTOP_SESSION}" >&2; exit 1 ;;
esac
if [[ ${DESKTOP_GPU:-false} == true ]]; then
    gpu-run glxinfo -B > /run/desktop/gpu-info 2>&1
    cat /run/desktop/gpu-info
    if grep -Eqi 'OpenGL renderer string:.*(llvmpipe|softpipe|Software Rasterizer)' /run/desktop/gpu-info; then
        echo 'GPU requested, but renderer is software. Check /dev/dxg, WSL libraries and GPU_ADAPTER.' >&2
        exit 1
    fi
    # Applications launched from the selected session inherit VirtualGL.
    gpu-run dbus-run-session -- "${session_command[@]}" &
else
    dbus-run-session -- "${session_command[@]}" &
fi
desktop_pid=$!
for _ in {1..150}; do
    kill -0 "$desktop_pid" || exit 1
    # An X server alone is not a ready desktop. Wait for the window manager.
    if xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q 'window id # 0x'; then break; fi
    sleep .2
done
xprop -root _NET_SUPPORTING_WM_CHECK | grep -q 'window id # 0x'
touch /run/desktop/ready
echo "Desktop ready: session=${DESKTOP_SESSION:-xfce}, ${DESKTOP_RESOLUTION:-1920x1080}, GPU=${DESKTOP_GPU:-false}, audio=${DESKTOP_AUDIO:-false}"
if [[ ${DESKTOP_AUDIO:-false} == true ]]; then
    wait -n "$vnc_pid" "$desktop_pid" "$pulse_pid"
else
    wait -n "$vnc_pid" "$desktop_pid"
fi

