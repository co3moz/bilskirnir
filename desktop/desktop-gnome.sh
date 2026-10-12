#!/usr/bin/env bash
set -euo pipefail
export XDG_SESSION_TYPE=x11 XDG_CURRENT_DESKTOP=ubuntu:GNOME
export XDG_SESSION_DESKTOP=ubuntu GNOME_SHELL_SESSION_MODE=ubuntu
mkdir -p "$HOME/.config/autostart"
for component in Power UsbProtection Sharing Smartcard Rfkill; do
    printf '[Desktop Entry]\nType=Application\nHidden=true\n' > "$HOME/.config/autostart/org.gnome.SettingsDaemon.${component}.desktop"
done
# The shared session supervisor owns PulseAudio (including the explicitly disabled case).
printf '[Desktop Entry]\nType=Application\nHidden=true\n' > "$HOME/.config/autostart/pulseaudio.desktop"
# Desktop locking/logind integration is unavailable in this container session.
gsettings set org.gnome.desktop.screensaver lock-enabled false
gsettings set org.gnome.desktop.session idle-delay 0
if [[ ! -f "$HOME/.config/wsld-gnome-initialized" ]]; then
    gsettings set org.gnome.desktop.interface gtk-theme Yaru
    gsettings set org.gnome.desktop.interface icon-theme Yaru
    gsettings set org.gnome.shell enabled-extensions "['ubuntu-dock@ubuntu.com', 'ubuntu-appindicators@ubuntu.com']"
    if [[ -f /usr/share/backgrounds/warty-final-ubuntu.png ]]; then
        gsettings set org.gnome.desktop.background picture-uri file:///usr/share/backgrounds/warty-final-ubuntu.png
        gsettings set org.gnome.desktop.background picture-uri-dark file:///usr/share/backgrounds/warty-final-ubuntu.png
    fi
    touch "$HOME/.config/wsld-gnome-initialized"
fi
exec gnome-session --session=wsld-ubuntu --disable-acceleration-check
