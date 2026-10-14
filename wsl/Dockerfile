# Root filesystem for the Bilskirnir WSL distribution: Ubuntu 24.04 with Docker Engine, Compose,
# Buildx, the NVIDIA Container Toolkit and the Bilskirnir installation files. The image is
# exported with `scripts/build-wsl.sh` into a `.wsl` package that `wsl --install --from-file` accepts.
FROM ubuntu:24.04
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl gnupg sudo systemd systemd-sysv dbus iproute2 iptables \
    openssl less nano locales tzdata dbus-user-session libpam-systemd \
    && locale-gen en_US.UTF-8 && rm -rf /var/lib/apt/lists/*
RUN install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc \
    && chmod a+r /etc/apt/keyrings/docker.asc \
    && echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu noble stable" > /etc/apt/sources.list.d/docker.list \
    && curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg \
    && curl -fsSL https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list \
       | sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' \
       > /etc/apt/sources.list.d/nvidia-container-toolkit.list \
    && apt-get update && apt-get install -y --no-install-recommends \
       docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin nvidia-container-toolkit \
    && rm -rf /var/lib/apt/lists/*
# Docker starts with systemd; the NVIDIA runtime is registered now so the first boot needs no network.
RUN nvidia-ctk runtime configure --runtime=docker \
    && systemctl enable docker containerd \
    && systemctl mask systemd-firstboot.service kmod-static-nodes.service \
    && userdel -r ubuntu \
    && printf '%%sudo ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/bilskirnir \
    && chmod 0440 /etc/sudoers.d/bilskirnir \
    && install -d -m 0755 /opt/bilskirnir /opt/bilskirnir/deploy /usr/lib/wsl /usr/share/doc/bilskirnir
# Every COPY names its mode, since a checkout on a Windows drive makes each file 0777. --chmod also
# applies to a directory the COPY creates, so each destination directory exists beforehand.
COPY --chmod=0644 compose.ghcr.yml /opt/bilskirnir/compose.yml
COPY --chmod=0644 .env.example /opt/bilskirnir/.env.example
COPY --chmod=0644 deploy/Caddyfile /opt/bilskirnir/deploy/Caddyfile
COPY --chmod=0644 wsl/wsl.conf /etc/wsl.conf
COPY --chmod=0644 wsl/wsl-distribution.conf /etc/wsl-distribution.conf
COPY --chmod=0755 wsl/oobe.sh /usr/lib/wsl/oobe.sh
COPY --chmod=0755 wsl/keepalive.sh /usr/lib/wsl/keepalive.sh
COPY --chmod=0755 wsl/bilskirnir /usr/local/bin/bilskirnir
COPY --chmod=0644 LICENSE /usr/share/doc/bilskirnir/LICENSE
RUN rm -f /etc/machine-id /var/lib/dbus/machine-id
