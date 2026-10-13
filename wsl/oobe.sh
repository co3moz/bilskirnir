#!/usr/bin/env bash
# First start of the Bilskirnir WSL distribution: create the Linux user, write the installation
# settings and optionally start the manager. WSL runs this as root the first time the distribution opens.
set -euo pipefail
INSTALL=/opt/bilskirnir
ENV_FILE=$INSTALL/.env

if getent passwd 1000 >/dev/null; then
    exit 0
fi
echo "Bilskirnir: first start"
echo

username=""
while [[ -z $username ]]; do
    read -rp "Linux username [bilskirnir]: " username || true
    username=${username:-bilskirnir}
    if ! [[ $username =~ ^[a-z_][a-z0-9_-]{0,31}$ ]]; then
        echo "Use lowercase letters, digits, dashes and underscores (at most 32 characters)."
        username=""
    fi
done
useradd --create-home --shell /bin/bash --uid 1000 --groups sudo,docker "$username"
if ! grep -q '^\[user\]' /etc/wsl.conf; then
    printf '\n[user]\ndefault=%s\n' "$username" >> /etc/wsl.conf
fi
echo "Created $username (sudo without a password; member of the docker group)."

password=""
while [[ ${#password} -lt 8 ]]; do
    read -rsp "Bilskirnir admin password for the web interface (at least 8 characters): " password || true
    echo
    if [[ ${#password} -lt 8 ]]; then
        echo "The password needs at least 8 characters."
        if [[ ! -t 0 ]]; then exit 1; fi
    fi
done
if [[ $password == *$'\n'* || $password == *"'"* ]]; then
    echo "The password must not contain line breaks or single quotes."
    exit 1
fi

gpu=false
cuda=false
adapter=""
if [[ -e /dev/dxg && -d /usr/lib/wsl/lib ]]; then
    gpu=true
    if compgen -G "/usr/lib/wsl/lib/libcuda.so*" >/dev/null; then
        cuda=true
        adapter=NVIDIA
    fi
fi
# Agent access is on by default in the packaged distribution, with its own random token.
mcp_token=$(openssl rand -hex 32)
cp "$INSTALL/.env.example" "$ENV_FILE"
sed -i \
    -e "s|^ADMIN_PASSWORD=.*|ADMIN_PASSWORD='$password'|" \
    -e "s|^MCP_ENABLED=.*|MCP_ENABLED=true|" \
    -e "s|^MCP_TOKEN=.*|MCP_TOKEN=$mcp_token|" \
    -e "s|^WSL_GPU=.*|WSL_GPU=$gpu|" \
    -e "s|^NVIDIA_COMPUTE=.*|NVIDIA_COMPUTE=$cuda|" \
    -e "s|^GPU_ADAPTER=.*|GPU_ADAPTER=$adapter|" \
    "$ENV_FILE"
chown root:docker "$ENV_FILE"
chmod 0640 "$ENV_FILE"
echo
echo "Settings written to $ENV_FILE (WSL GPU: $gpu, CUDA: $cuda)."
if [[ $gpu == false ]]; then
    echo "No WSL GPU device was found; desktops will run without GPU acceleration."
fi
echo
echo "Agent access (MCP) is enabled with a random token; 'bilskirnir mcp' shows it with client settings."
echo
echo "Next steps:"
echo "  bilskirnir start    download the images and start the manager"
echo "  bilskirnir doctor   check Docker, GPU and network"
echo "  bilskirnir mcp      show the agent endpoint, token and client configuration"
echo
# Only an interactive first start offers to download the images right away.
start=n
if [[ -t 0 ]]; then
    read -rp "Start Bilskirnir now? This downloads about 3 GB of images. [Y/n] " start || true
    start=${start:-y}
fi
if [[ $start =~ ^[Yy]$ ]]; then
    runuser -u "$username" -- /usr/local/bin/bilskirnir start
fi
