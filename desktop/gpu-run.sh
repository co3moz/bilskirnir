#!/usr/bin/env bash
set -euo pipefail
if [[ $# == 0 ]]; then echo 'Usage: gpu-run Unity [args...] | gpu-run glxinfo -B'; exit 2; fi
if [[ ${DESKTOP_GPU:-false} != true ]]; then echo 'GPU is disabled for this environment.' >&2; exit 1; fi
export LD_LIBRARY_PATH="/usr/lib/wsl/lib:${LD_LIBRARY_PATH:-}"
export MESA_D3D12_DEFAULT_ADAPTER_NAME="${GPU_ADAPTER:-}"
if [[ -e /dev/dxg ]]; then export GALLIUM_DRIVER=d3d12; fi
unset LIBGL_ALWAYS_SOFTWARE
# Chromium-based browsers pick their X visual with raw GLX requests that bypass VirtualGL and
# then expect a framebuffer configuration attached to that visual. VGL_CHROMEHACK makes
# VirtualGL emulate that choice by attaching the browser's 32-bit visual to every framebuffer
# configuration with an 8-bit alpha channel. That changes the window visual of any other
# application that asks for an alpha channel, so it is enabled only for Chromium-family command
# names; an explicit VGL_CHROMEHACK in the environment always wins.
if [[ -z ${VGL_CHROMEHACK:-} ]]; then
    case "$(basename -- "$1")" in
        google-chrome*|chrome|chromium*|brave*|microsoft-edge*|vivaldi*|opera*|electron*)
            export VGL_CHROMEHACK=1 ;;
    esac
fi
# Route OpenGL rendering to EGL, present pixels through the independent VNC X server.
exec /opt/VirtualGL/bin/vglrun -d "${VGL_DISPLAY:-egl}" "$@"

