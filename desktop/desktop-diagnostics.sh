#!/usr/bin/env bash
set -u
export DISPLAY=:0
export XDG_RUNTIME_DIR=/run/desktop
printf '=== Desktop / OpenGL ===\n'
glxinfo -B 2>&1 || true
printf '\n=== GPU / VirtualGL EGL ===\n'
if [[ ${DESKTOP_GPU:-false} == true ]]; then gpu-run glxinfo -B 2>&1 || true; else echo 'GPU disabled'; fi
printf '\n=== Vulkan ===\n'
timeout 10 env -u DISPLAY vulkaninfo --summary 2>&1 || true
printf '\n=== CUDA ===\n'
if [[ -x /usr/lib/wsl/lib/nvidia-smi ]]; then /usr/lib/wsl/lib/nvidia-smi 2>&1; else echo 'NVIDIA runtime not present'; fi
printf '\n=== Audio ===\n'
pactl info 2>&1 || true

