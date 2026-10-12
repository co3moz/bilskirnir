#!/usr/bin/env bash
set -euo pipefail
install -d -m 0700 -o desktop -g desktop /run/desktop
if [[ ${DESKTOP_SESSION:-xfce} == gnome ]]; then
    install -d /run/dbus
    dbus-daemon --system --fork --nopidfile
fi
if [[ ${DESKTOP_SUDO:-true} == true ]]; then
    printf 'desktop ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/desktop
    chmod 0440 /etc/sudoers.d/desktop
else
    rm -f /etc/sudoers.d/desktop
fi
if [[ ${DESKTOP_GPU:-false} == true ]]; then
    export LD_LIBRARY_PATH="/usr/lib/wsl/lib:${LD_LIBRARY_PATH:-}"
    export MESA_D3D12_DEFAULT_ADAPTER_NAME="${GPU_ADAPTER:-}"
    unset LIBGL_ALWAYS_SOFTWARE
else
    export LIBGL_ALWAYS_SOFTWARE=1
fi
# Refresh runtime settings on every start, including starts from committed images.
printf 'export DISPLAY=:0\nexport PULSE_SERVER=unix:/run/desktop/pulse/native\n' > /etc/profile.d/desktop.sh
if [[ ${DESKTOP_GPU:-false} == true ]]; then
    printf 'export LD_LIBRARY_PATH=/usr/lib/wsl/lib:${LD_LIBRARY_PATH:-}\n' >> /etc/profile.d/desktop.sh
fi
exec runuser -u desktop -- /usr/local/bin/desktop-session

