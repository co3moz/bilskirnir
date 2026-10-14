#!/usr/bin/env bash
# WSL 2 runs every distribution in one network namespace, so other distributions can remove the
# firewall rules this Docker Engine programmed. An enabled nftables service flushes the whole
# ruleset when it starts and when it stops, which removes all of them: desktops lose internet
# access and new internal networks cannot be created. A Docker Engine starting elsewhere rebuilds
# Docker's shared chains with only its own networks: NAT survives, isolation between networks
# does not. Docker notices neither. This loop checks that every bridge network of this engine
# still has its rule in DOCKER-FORWARD and restarts Docker when one is missing twice in a row;
# live-restore keeps the containers running across that restart.
#
# Usage: firewall-watch.sh          watch and repair (the bilskirnir-firewall service)
#        firewall-watch.sh --check  print the bridge networks without rules; exit 1 if there are any
set -uo pipefail
INTERVAL=10
COOLDOWN=60
MAX_COOLDOWN=3600

# Prints "name bridge" for each bridge network without rules; returns 2 when it cannot tell.
missing() {
    local ids info chain name id bridge
    # The chain layout below is the iptables backend's; other backends are not checked.
    [[ $(docker info --format '{{.FirewallBackend.Driver}}' 2>/dev/null) == iptables ]] || return 2
    nft list tables >/dev/null 2>&1 || return 2
    ids=$(docker network ls --quiet --no-trunc --filter driver=bridge) || return 2
    [[ -n $ids ]] || return 0
    # shellcheck disable=SC2086 # one argument per network ID
    info=$(docker network inspect --format '{{.Name}} {{.Id}} {{with index .Options "com.docker.network.bridge.name"}}{{.}}{{end}}' $ids) || return 2
    # Docker accepts each bridge network's outgoing traffic here; a flushed ruleset has no chain.
    chain=$(nft list chain ip filter DOCKER-FORWARD 2>/dev/null) || chain=""
    while read -r name id bridge; do
        bridge=${bridge:-br-${id:0:12}}
        if ! grep -qF "iifname \"$bridge\"" <<<"$chain"; then
            echo "$name $bridge"
        fi
    done <<<"$info"
}

if [[ ${1:-} == --check ]]; then
    found=$(missing 2>/dev/null)
    case $? in
        0) [[ -z $found ]] || { echo "$found"; exit 1; } ;;
        *) echo "Docker with the iptables firewall backend, or root for nft, is not available." >&2; exit 2 ;;
    esac
    exit 0
fi

strikes=0
last_restart=-$MAX_COOLDOWN
cooldown=$COOLDOWN
while sleep "$INTERVAL"; do
    found=$(missing 2>/dev/null) || continue
    if [[ -z $found ]]; then
        strikes=0
        cooldown=$COOLDOWN
        continue
    fi
    # A network that is being created may not have its rules yet, so act on the second sighting.
    strikes=$((strikes + 1))
    if (( strikes < 2 || SECONDS - last_restart < cooldown )); then
        continue
    fi
    names=$(awk '{print $1}' <<<"$found" | paste -sd, -)
    last_restart=$SECONDS
    if [[ $(docker info --format '{{.LiveRestoreEnabled}}' 2>/dev/null) != true ]]; then
        echo "Firewall rules are missing for $names. live-restore is off, so a restart would stop the desktops; run 'sudo systemctl restart docker'."
        continue
    fi
    echo "Firewall rules are missing for $names; another WSL distribution probably removed them. Restarting Docker, containers keep running."
    systemctl restart docker
    strikes=0
    # A restart that does not bring the rules back is not retried every minute.
    cooldown=$(( cooldown * 2 > MAX_COOLDOWN ? MAX_COOLDOWN : cooldown * 2 ))
done
