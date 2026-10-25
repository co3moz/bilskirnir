"""Run a volume helper's command as the desktop user, first claiming an empty volume.

Docker creates a volume's root owned by root. A desktop that mounts a new volume over one of its
own folders hands it that folder's owner; a volume opened from the dashboard first gets the
desktop user, so files can be added at its root. A volume that holds anything keeps its owner.
"""
import os
import sys

VOLUME = "/volume"
DESKTOP = 1001

info = os.stat(VOLUME)
if info.st_uid == 0 and not os.listdir(VOLUME):
    os.chown(VOLUME, DESKTOP, DESKTOP)
os.setgroups([])
os.setgid(DESKTOP)
os.setuid(DESKTOP)
os.execvp(sys.argv[1], sys.argv[1:])
