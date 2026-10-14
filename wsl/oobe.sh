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
# Asks with a menu moved by the up and down arrows and confirmed with Enter; the first option is
# the default. Sets choice to the picked option's first word in lowercase, which is also what a
# piped first start answers with (or its first letter, or an empty line for the default).
choose() {
    local question=$1 selected=0 key rest answer i
    shift
    local options=("$@") words=()
    local count=${#options[@]}
    for i in "${!options[@]}"; do
        words[i]=${options[i]%%[ ,]*}
        words[i]=${words[i],,}
    done
    if [[ ! -t 0 ]]; then
        answer=""
        read -r answer || true
        answer=${answer,,}
        selected=-1
        for i in "${!words[@]}"; do
            if [[ -z $answer || $answer == "${words[i]}" || $answer == "${words[i]:0:1}" ]]; then
                selected=$i
                break
            fi
        done
        if [[ $selected == -1 ]]; then
            echo "$question Unknown answer: $answer"
            exit 1
        fi
        echo "$question ${options[selected]%%,*}"
        choice=${words[selected]}
        return
    fi
    trap 'printf "\e[?25h"' EXIT
    printf '%s \e[2m(arrow keys, then Enter)\e[0m\n\e[?25l' "$question"
    while true; do
        for i in "${!options[@]}"; do
            if [[ $i == "$selected" ]]; then
                printf '\e[2K  \e[1m> %s\e[0m\n' "${options[i]}"
            else
                printf '\e[2K    %s\n' "${options[i]}"
            fi
        done
        IFS= read -rsn1 key || break
        if [[ $key == $'\e' ]]; then
            rest=""
            IFS= read -rsn2 -t 0.05 rest || true
            key+=$rest
        fi
        case $key in
            $'\e[A' | $'\eOA') selected=$(((selected + count - 1) % count)) ;;
            $'\e[B' | $'\eOB') selected=$(((selected + 1) % count)) ;;
            '') break ;;
        esac
        printf '\e[%dA' "$count"
    done
    # The menu collapses into one line with the question and the answer.
    printf '\e[%dA\e[J\e[?25h%s %s\n' "$((count + 1))" "$question" "${options[selected]%%,*}"
    trap - EXIT
    choice=${words[selected]}
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

echo
echo "Agents such as Claude Code use the desktops through the MCP endpoint."
choose "Enable agent access (MCP)?" "Yes" "No"
mcp=$choice
mcp_auth=""
mcp_token=""
if [[ $mcp == yes ]]; then
    choose "How do agents authenticate?" "Random token, generated now" "Custom token, entered next" \
        "None, no token required"
    mcp_auth=$choice
fi
# Bearer token characters (RFC 6750) need no quoting in .env, an HTTP header or a pasted command.
token_pattern='^[A-Za-z0-9._~+/-]+=*$'
if [[ $mcp_auth == custom ]]; then
    while true; do
        read -erp "Token (32 to 256 characters): " mcp_token || true
        if [[ ${#mcp_token} -ge 32 && ${#mcp_token} -le 256 && $mcp_token =~ $token_pattern ]]; then
            break
        fi
        echo "Use 32 to 256 letters, digits and - . _ ~ + / characters, with = only at the end."
        fail_unless_interactive
    done
fi

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
# Agent access defaults to on with a random token; the answers above can choose a custom one or none.
if [[ $mcp == yes ]]; then
    set_setting MCP_ENABLED true
    if [[ $mcp_auth == none ]]; then
        set_setting MCP_AUTH_MODE none
    else
        set_setting MCP_AUTH_MODE token
        set_setting MCP_TOKEN "${mcp_token:-$(openssl rand -hex 32)}"
    fi
else
    set_setting MCP_ENABLED false
fi
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
start=no
if [[ -t 0 ]]; then
    echo "Starting downloads the manager image; desktop images download when you first create a desktop."
    choose "Start Bilskirnir now?" "Yes" "No"
    start=$choice
fi
if [[ $start == yes ]]; then
    runuser -u "$username" -- /usr/local/bin/bilskirnir start ||
        echo "Bilskirnir did not start. Run 'bilskirnir doctor', then 'bilskirnir start'."
else
    echo "Run 'bilskirnir start' to download and start the manager."
fi
echo
if [[ $mcp == yes ]]; then
    # The agent settings are printed once here, last, so they are not scrolled away by the download.
    /usr/local/bin/bilskirnir mcp
    echo
    echo "'bilskirnir mcp' shows this again, and 'bilskirnir doctor' checks Docker, the GPU and the network."
else
    echo "Agent access (MCP) is off; 'bilskirnir mcp' explains how to turn it on."
    echo "'bilskirnir doctor' checks Docker, the GPU and the network."
fi
