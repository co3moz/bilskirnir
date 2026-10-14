#!/usr/bin/env bash
# First start of the Bilskirnir WSL distribution: create the account, write the installation
# settings and optionally start the manager. WSL runs this as root the first time the distribution opens.
set -euo pipefail
INSTALL=/opt/bilskirnir
ENV_FILE=$INSTALL/.env

if getent passwd 1000 >/dev/null; then
    exit 0
fi
# A piped first start cannot be asked again, so an invalid answer ends it instead of looping.
fail_unless_interactive() {
    if [[ ! -t 0 ]]; then exit 1; fi
}
# Values go through the environment, so characters such as & | \ in a password stay literal.
# The file holds the password and the agent token, so it is only ever written by root alone.
set_setting() {
    (umask 077 && KEY=$1 VALUE=$2 awk 'BEGIN { k = ENVIRON["KEY"]; v = ENVIRON["VALUE"] }
        index($0, k "=") == 1 { print k "=" v; found = 1; next } { print }
        END { if (!found) print k "=" v }' "$ENV_FILE" >"$ENV_FILE.new")
    mv "$ENV_FILE.new" "$ENV_FILE"
}

echo "Bilskirnir: first start"
echo
echo "Choose one account. It is your Linux user and your dashboard sign-in."
username=""
while [[ -z $username ]]; do
    read -erp "Username [bilskirnir]: " username || true
    username=${username:-bilskirnir}
    if ! [[ $username =~ ^[a-z_][a-z0-9_-]{0,31}$ ]]; then
        echo "Use lowercase letters, digits, dashes and underscores (at most 32 characters)."
        username=""
        fail_unless_interactive
    fi
done
# The manager requires at least eight characters for a sign-in password; an empty one turns sign-in off.
while true; do
    password="" repeat=""
    read -rsp "Password (at least 8 characters, or empty for no password): " password || true
    echo
    if [[ -z $password ]]; then
        break
    fi
    if [[ ${#password} -lt 8 ]]; then
        echo "The password needs at least 8 characters."
    elif [[ $password == *"'"* ]]; then
        echo "The password must not contain single quotes."
    else
        read -rsp "Repeat the password: " repeat || true
        echo
        if [[ $password == "$repeat" ]]; then
            break
        fi
        echo "The passwords do not match."
    fi
    fail_unless_interactive
done

useradd --create-home --shell /bin/bash --uid 1000 --groups sudo,docker "$username"
if [[ -n $password ]]; then
    printf '%s:%s\n' "$username" "$password" | chpasswd
fi
if ! grep -q '^\[user\]' /etc/wsl.conf; then
    printf '\n[user]\ndefault=%s\n' "$username" >> /etc/wsl.conf
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
(umask 077 && cp "$INSTALL/.env.example" "$ENV_FILE")
if [[ -n $password ]]; then
    set_setting AUTH_MODE password
    set_setting ADMIN_PASSWORD "'$password'"
else
    set_setting AUTH_MODE none
fi
set_setting ADMIN_USERNAME "$username"
# Agent access is on by default in the packaged distribution, with its own random token.
set_setting MCP_ENABLED true
set_setting MCP_TOKEN "$(openssl rand -hex 32)"
set_setting WSL_GPU "$gpu"
set_setting NVIDIA_COMPUTE "$cuda"
set_setting GPU_ADAPTER "$adapter"
chown root:docker "$ENV_FILE"
chmod 0640 "$ENV_FILE"

echo
if [[ -n $password ]]; then
    echo "Created $username with your password. It signs in to the dashboard too; sudo needs no password."
else
    echo "Created $username without a password. The dashboard opens without signing in, so anyone"
    echo "who can reach it can use it; set a password before allowing LAN access."
fi
echo "Settings written to $ENV_FILE (WSL GPU: $gpu, CUDA: $cuda)."
if [[ $gpu == false ]]; then
    echo "No WSL GPU device was found; desktops will run without GPU acceleration."
fi
echo

# Only an interactive first start offers to download the manager right away.
start=n
if [[ -t 0 ]]; then
    echo "Desktop images download when you first create a desktop from them."
    read -erp "Start Bilskirnir now? This downloads the manager image. [Y/n] " start || true
    start=${start:-y}
fi
if [[ $start =~ ^[Yy]$ ]]; then
    runuser -u "$username" -- /usr/local/bin/bilskirnir start ||
        echo "Bilskirnir did not start. Run 'bilskirnir doctor', then 'bilskirnir start'."
else
    echo "Run 'bilskirnir start' to download and start the manager."
fi
echo
# The agent settings are printed once here, last, so they are not scrolled away by the download.
/usr/local/bin/bilskirnir mcp
echo
echo "'bilskirnir mcp' shows this again, and 'bilskirnir doctor' checks Docker, the GPU and the network."
