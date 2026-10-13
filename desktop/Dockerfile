FROM ubuntu:24.04 AS common
ARG DEBIAN_FRONTEND=noninteractive
ARG VIRTUALGL_VERSION=3.1.4
RUN apt-get update && apt-get install -y --no-install-recommends \
    dbus-x11 tigervnc-standalone-server tigervnc-common \
    x11-utils x11-xserver-utils x11-apps xdotool wmctrl imagemagick sudo tini curl ca-certificates python3 \
    at-spi2-core python3-pyatspi \
    pulseaudio pulseaudio-utils mesa-utils mesa-utils-extra libgl1-mesa-dri libegl1 \
    mesa-vulkan-drivers vulkan-tools fonts-dejavu fonts-noto-color-emoji locales \
    && locale-gen en_US.UTF-8 && rm -rf /var/lib/apt/lists/*
RUN curl -fL --retry 3 "https://github.com/VirtualGL/virtualgl/releases/download/${VIRTUALGL_VERSION}/virtualgl_${VIRTUALGL_VERSION}_amd64.deb" -o /tmp/virtualgl.deb \
    && apt-get update && apt-get install -y /tmp/virtualgl.deb && rm /tmp/virtualgl.deb && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --shell /bin/bash --uid 1001 desktop \
    && mkdir -p /shared /workspace && chown desktop:desktop /shared /workspace \
    && mkdir -p /usr/local/share/applications
LABEL io.wsl-desktop.desktop=true
ENV LANG=en_US.UTF-8 DISPLAY=:0 DESKTOP_RESOLUTION=1920x1080 \
    DESKTOP_AUDIO=false DESKTOP_MICROPHONE=false DESKTOP_GPU=false DESKTOP_SUDO=true \
    PULSE_SERVER=unix:/run/desktop/pulse/native
EXPOSE 5900
HEALTHCHECK --interval=10s --timeout=4s --start-period=30s CMD /usr/local/bin/desktop-healthcheck
ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/desktop-entrypoint"]

FROM common AS scripts
COPY desktop-entrypoint.sh /usr/local/bin/desktop-entrypoint
COPY desktop-session.sh /usr/local/bin/desktop-session
COPY desktop-screenshot.sh /usr/local/bin/desktop-screenshot
COPY desktop-diagnostics.sh /usr/local/bin/desktop-diagnostics
COPY gpu-run.sh /usr/local/bin/gpu-run
COPY desktop-gnome.sh /usr/local/bin/desktop-gnome
COPY desktop-healthcheck.sh /usr/local/bin/desktop-healthcheck
RUN chmod +x /usr/local/bin/desktop-* /usr/local/bin/gpu-run

FROM common AS xfce-packages
RUN apt-get update && apt-get install -y --no-install-recommends xfce4 xfce4-terminal \
    && rm -rf /var/lib/apt/lists/*
FROM xfce-packages AS xfce
COPY --from=scripts /usr/local/bin/ /usr/local/bin/
COPY gpu-terminal.desktop /usr/share/applications/gpu-terminal.desktop
ENV DESKTOP_SESSION=xfce
LABEL io.wsl-desktop.profile=ubuntu-xfce io.wsl-desktop.session=xfce \
    org.opencontainers.image.title="Ubuntu 24.04 · Lightweight XFCE" \
    org.opencontainers.image.description="Ubuntu LTS with a lightweight XFCE desktop, independent VNC display and optional GPU/audio" \
    org.opencontainers.image.version="24.04"

FROM common AS gnome-packages
RUN apt-get update && apt-get install -y --no-install-recommends \
    ubuntu-session gnome-session gnome-shell gnome-settings-daemon gnome-control-center \
    gnome-terminal nautilus gnome-shell-extension-ubuntu-dock gnome-shell-extension-appindicator \
    yaru-theme-gtk yaru-theme-icon yaru-theme-gnome-shell gsettings-ubuntu-schemas \
    gnome-themes-extra dbus-user-session at-spi2-core \
    && rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get install -y --no-install-recommends ubuntu-wallpapers fonts-ubuntu \
    librsvg2-common gir1.2-dbusmenu-glib-0.4 ibus \
    && rm -rf /var/lib/apt/lists/*
FROM gnome-packages AS gnome
COPY --from=scripts /usr/local/bin/ /usr/local/bin/
COPY gnome-container.session /usr/share/gnome-session/sessions/wsld-ubuntu.session
ENV DESKTOP_SESSION=gnome
LABEL io.wsl-desktop.profile=ubuntu-gnome io.wsl-desktop.session=gnome \
    org.opencontainers.image.title="Ubuntu 24.04 · GNOME" \
    org.opencontainers.image.description="Ubuntu GNOME Shell with Ubuntu session and Yaru appearance, independent VNC display and optional GPU/audio" \
    org.opencontainers.image.version="24.04"

# An unqualified docker build retains the lightweight default.
FROM xfce AS default

