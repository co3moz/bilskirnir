"""Package recording frames and videos and application logs into a tar.gz as the desktop user; never
follow path symlinks."""
import io
import json
import os
import sys
import tarfile
import time
import uuid

destination, name, replace, manifest = sys.argv[1], sys.argv[2], sys.argv[3] == "true", json.loads(sys.argv[4])
VIDEOS = ("recording.mp4", "recording.avi")
folder = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
temporary = None
try:
    for part in filter(None, destination.lstrip("/").split("/")):
        try:
            os.mkdir(part, 0o755, dir_fd=folder)
        except FileExistsError:
            pass
        child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=folder)
        os.close(folder)
        folder = child
    temporary = ".thoric-export-" + uuid.uuid4().hex
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644, dir_fd=folder)
    included = {"recordings": [], "applications": [], "videos": {}}
    with os.fdopen(descriptor, "wb") as output, tarfile.open(fileobj=output, mode="w:gz") as archive:
        for kind in ("recordings", "applications"):
            for identifier, directory in manifest.get(kind, {}).items():
                if not os.path.isdir(directory):
                    continue
                added = set()
                for entry in sorted(os.listdir(directory)):
                    path = os.path.join(directory, entry)
                    if entry == "stop" or entry.endswith(".tmp") or os.path.islink(path) or not os.path.isfile(path):
                        continue
                    archive.add(path, arcname="%s/%s/%s" % (kind, identifier, entry), recursive=False)
                    added.add(entry)
                included[kind].append(identifier)
                # A recording's video: the MP4, or the MJPEG AVI made where the desktop had no encoder.
                video = next((name for name in VIDEOS if name in added), None)
                if kind == "recordings" and video:
                    included["videos"][identifier] = "recordings/%s/%s" % (identifier, video)
        data = json.dumps({"exported_at": int(time.time()), **included}, indent=2).encode()
        info = tarfile.TarInfo("manifest.json")
        info.size = len(data)
        info.mtime = int(time.time())
        archive.addfile(info, io.BytesIO(data))
        output.flush()
        os.fsync(output.fileno())
    if replace:
        os.replace(temporary, name, src_dir_fd=folder, dst_dir_fd=folder)
    else:
        try:
            os.link(temporary, name, src_dir_fd=folder, dst_dir_fd=folder, follow_symlinks=False)
        except FileExistsError:
            print("CONFLICT: File already exists. Set replace=true to overwrite it.", file=sys.stderr)
            sys.exit(17)
    size = os.stat(name, dir_fd=folder, follow_symlinks=False).st_size
    print(json.dumps({"path": destination.rstrip("/") + "/" + name, "bytes": size, **included}))
except Exception as error:
    print("Export failed: " + str(error), file=sys.stderr)
    sys.exit(1)
finally:
    if temporary is not None:
        try:
            os.unlink(temporary, dir_fd=folder)
        except FileNotFoundError:
            pass
    os.close(folder)
