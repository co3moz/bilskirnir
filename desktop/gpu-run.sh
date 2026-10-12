#!/usr/bin/env bash
set -euo pipefail
if [[ $# == 0 ]]; then echo 'Usage: gpu-run Unity [args...] | gpu-run glxinfo -B'; exit 2; fi
if [[ ${DESKTOP_GPU:-false} != true ]]; then echo 'GPU is disabled for this environment.' >&2; exit 1; fi
export LD_LIBRARY_PATH="/usr/lib/wsl/lib:${LD_LIBRARY_PATH:-}"
export MESA_D3D12_DEFAULT_ADAPTER_NAME="${GPU_ADAPTER:-}"
if [[ -e /dev/dxg ]]; then export GALLIUM_DRIVER=d3d12; fi
unset LIBGL_ALWAYS_SOFTWARE
# Route OpenGL rendering to EGL, present pixels through the independent VNC X server.
exec /opt/VirtualGL/bin/vglrun -d "${VGL_DISPLAY:-egl}" "$@"

