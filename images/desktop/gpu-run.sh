#!/usr/bin/env bash
set -euo pipefail
if [[ $# == 0 ]]; then echo 'Usage: gpu-run Unity [args...] | gpu-run glxinfo -B'; exit 2; fi
if [[ ${DESKTOP_GPU:-false} != true ]]; then echo 'GPU is disabled for this container.' >&2; exit 1; fi
if [[ -e /dev/dxg ]]; then
    # WSL: Mesa's D3D12 driver renders on the WSL GPU with the driver libraries Windows provides.
    export LD_LIBRARY_PATH="/usr/lib/wsl/lib:${LD_LIBRARY_PATH:-}"
    export MESA_D3D12_DEFAULT_ADAPTER_NAME="${GPU_ADAPTER:-}"
    export GALLIUM_DRIVER=d3d12
fi
unset LIBGL_ALWAYS_SOFTWARE
# Elsewhere EGL lists NVIDIA's device first, then the cards Mesa's own drivers run, and its
# software renderer last. GPU_ADAPTER=AMD or Intel picks that vendor's card under /dev/dri.
card() {
    local vendor node
    shopt -s nocasematch
    case ${GPU_ADAPTER:-} in
        amd) vendor=0x1002 ;;
        intel) vendor=0x8086 ;;
        *) return 0 ;;
    esac
    for node in /dev/dri/card*; do
        if [[ $(cat "/sys/class/drm/${node##*/}/device/vendor" 2>/dev/null) == "$vendor" ]]; then
            echo "$node"
            return 0
        fi
    done
    return 0
}
display=${VGL_DISPLAY:-}
if [[ -z $display && ! -e /dev/dxg ]]; then display=$(card); fi
# Chromium-family processes turn on VirtualGL's Chrome workaround for themselves. The session
# runs under gpu-run, so this also reaches a browser started from the panel, the menu or a link.
chromium_gpu=/usr/local/lib/bilskirnir/chromium-gpu.so
if [[ :${LD_PRELOAD:-}: != *:"$chromium_gpu":* ]]; then
    export LD_PRELOAD="$chromium_gpu${LD_PRELOAD:+:$LD_PRELOAD}"
fi
# Route OpenGL rendering to EGL, present pixels through the independent VNC X server.
exec /opt/VirtualGL/bin/vglrun -d "${display:-egl}" "$@"
