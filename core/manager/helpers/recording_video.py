"""Make a finished recording's video from its frames, as the desktop user: H.264 in MP4 through
GStreamer, each frame shown from the moment it was captured, or an MJPEG AVI at the recording's
frame rate when the desktop image has no H.264 encoder or encoding runs out of time. The frames
stay. Prints the recordings' states, with the video under "video", as a JSON list."""
import fcntl
import json
import math
import os
import re
import struct
import sys
import time

FRAME = re.compile(r"frame-(\d+)-(\d+)\.jpg")
# The video keeps at most a 3840x2160 picture's area, within the H.264 level browsers decode;
# a larger frame is scaled down, and the frames themselves keep their size.
LARGEST_AREA = 3840 * 2160
# A clock step back between frames still leaves each one this long on screen.
SHORTEST_MS = 1
ELEMENTS = ("appsrc", "jpegdec", "videoconvert", "videoscale", "capsfilter", "x264enc", "mp4mux", "filesink")
# veryfast with two threads keeps the encoder from taking over the desktop's CPU; key frames come
# about every ten seconds of the recording's frame rate, so a player can seek. Without B-frames
# every frame is decoded in the order it shows, which keeps uneven frame times simple to mux.
PIPELINE = ("appsrc name=frames format=time ! jpegdec ! videoconvert ! videoscale add-borders=true "
            "! capsfilter name=size ! x264enc name=encoder speed-preset=veryfast threads=2 bframes=0 "
            "pass=qual quantizer=23 ! mp4mux name=muxer faststart=true ! filesink name=file")


class Unavailable(Exception):
    """The desktop image lacks the GStreamer parts the H.264 video needs."""


def frames(directory):
    """The frames' file names and capture times in milliseconds, in the order they were taken."""
    found = []
    for name in os.listdir(directory):
        match = FRAME.fullmatch(name)
        if match:
            found.append((int(match.group(1)), int(match.group(2)), name))
    found.sort()
    return [name for _, _, name in found], [captured for _, captured, _ in found]


def schedule(captured, fps, ended_at=None):
    """Each frame's start and length in milliseconds from the first frame: a frame stays until the
    next one was captured and the last until the recording ended, or one frame period without an
    end. A clock that steps back keeps the order instead of reversing it."""
    period = max(SHORTEST_MS, round(1000 / fps))
    starts = []
    for moment in captured:
        start = moment - captured[0]
        starts.append(max(start, starts[-1] + SHORTEST_MS) if starts else 0)
    plan = []
    for index, start in enumerate(starts):
        if index + 1 < len(starts):
            length = starts[index + 1] - start
        elif ended_at is not None and ended_at - captured[0] > start:
            length = max(SHORTEST_MS, ended_at - captured[0] - start)
        else:
            length = period
        plan.append((start, length))
    return plan


def video_size(width, height):
    """The encoded picture: even sides for 4:2:0 color, scaled down to at most LARGEST_AREA."""
    if width * height > LARGEST_AREA:
        scale = math.sqrt(LARGEST_AREA / (width * height))
        width, height = int(width * scale), int(height * scale)
    return max(2, width - width % 2), max(2, height - height % 2)


def jpeg_size(data):
    position = 2
    while position + 9 < len(data):
        if data[position] != 0xFF:
            return None
        marker = data[position + 1]
        if marker == 0xFF:
            position += 1
            continue
        if marker == 0x01 or 0xD0 <= marker <= 0xD8:
            position += 2
            continue
        if marker in (0xD9, 0xDA):
            return None
        length = int.from_bytes(data[position + 2:position + 4], "big")
        if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
            return (int.from_bytes(data[position + 7:position + 9], "big"),
                    int.from_bytes(data[position + 5:position + 7], "big"))
        position += 2 + length
    return None


def remove(*paths):
    for path in paths:
        try:
            os.unlink(path)
        except FileNotFoundError:
            pass


def write_mp4(directory, names, plan, size, fps, deadline):
    """Encode the frames as H.264 in MP4, each at its own time, or raise why it cannot."""
    try:
        import gi
        gi.require_version("Gst", "1.0")
        from gi.repository import Gst
    except (ImportError, ValueError) as error:
        raise Unavailable("GStreamer's Python bindings are not installed in this desktop image") from error
    Gst.init(None)
    missing = [name for name in ELEMENTS if Gst.ElementFactory.find(name) is None]
    if missing:
        raise Unavailable("GStreamer elements missing in this desktop image: " + ", ".join(missing))
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("No time was left to encode the video")
    path = os.path.join(directory, "recording.mp4")
    temporary, staging = path + ".tmp", path + ".moov.tmp"
    pipeline = Gst.parse_launch(PIPELINE)
    source = pipeline.get_by_name("frames")
    source.set_property("caps", Gst.Caps.from_string("image/jpeg,framerate=0/1"))
    pipeline.get_by_name("size").set_property("caps", Gst.Caps.from_string(
        "video/x-raw,format=I420,width=%d,height=%d,pixel-aspect-ratio=1/1,colorimetry=bt709" % size))
    pipeline.get_by_name("encoder").set_property("key-int-max", max(1, int(fps * 10)))
    pipeline.get_by_name("muxer").set_property("faststart-file", staging)
    pipeline.get_by_name("file").set_property("location", temporary)
    pending = iter(zip(names, plan))
    failures = []

    def feed(element, _length):
        """Hand appsrc the next frame when it asks, from its streaming thread."""
        item = next(pending, None) if not failures else None
        if item is None:
            element.emit("end-of-stream")
            return
        name, (start, length) = item
        try:
            with open(os.path.join(directory, name), "rb") as handle:
                buffer = Gst.Buffer.new_wrapped(handle.read())
        except OSError as error:
            failures.append(error)
            element.emit("end-of-stream")
            return
        buffer.pts = start * Gst.MSECOND
        buffer.duration = length * Gst.MSECOND
        element.emit("push-buffer", buffer)

    source.connect("need-data", feed)
    try:
        if pipeline.set_state(Gst.State.PLAYING) == Gst.StateChangeReturn.FAILURE:
            raise RuntimeError("The video pipeline did not start")
        message = pipeline.get_bus().timed_pop_filtered(
            int(remaining * Gst.SECOND), Gst.MessageType.EOS | Gst.MessageType.ERROR)
        if message is None:
            raise TimeoutError("Encoding took longer than %d seconds" % round(remaining))
        if message.type == Gst.MessageType.ERROR:
            error, _ = message.parse_error()
            raise RuntimeError("GStreamer: " + error.message)
        if failures:
            raise failures[0]
    except BaseException:
        pipeline.set_state(Gst.State.NULL)
        remove(temporary, staging)
        raise
    pipeline.set_state(Gst.State.NULL)
    remove(staging)
    os.replace(temporary, path)
    return {"path": path, "bytes": os.path.getsize(path), "width": size[0], "height": size[1],
            "codec": "H.264", "container": "MP4", "mime_type": "video/mp4", "frames": len(names),
            "duration_ms": plan[-1][0] + plan[-1][1]}


def write_avi(directory, names, size, fps):
    """Write recording.avi (MJPEG) from the frame sequence without external encoders."""
    width, height = size
    sizes = [os.path.getsize(os.path.join(directory, name)) for name in names]
    chunk_sizes = [8 + length + (length % 2) for length in sizes]
    movi_size = 4 + sum(chunk_sizes)
    index_size = 16 * len(names)
    rate, scale = int(round(fps * 1000)), 1000

    def chunk(fourcc, data):
        return fourcc + struct.pack("<I", len(data)) + data + (b"\0" if len(data) % 2 else b"")

    avih = struct.pack("<IIIIIIIIII4I", int(1000000 / fps), 0, 0, 0x10 | 0x100, len(names), 0, 1, 0,
                       width, height, 0, 0, 0, 0)
    strh = (b"vids" + b"MJPG" + struct.pack("<IHHIIIIIIII", 0, 0, 0, 0, scale, rate, 0, len(names), 0,
                                               0xFFFFFFFF, 0) + struct.pack("<hhhh", 0, 0, width, height))
    strf = struct.pack("<IiiHH4sIiiII", 40, width, height, 1, 24, b"MJPG", width * height * 3, 0, 0, 0, 0)
    strl = chunk(b"LIST", b"strl" + chunk(b"strh", strh) + chunk(b"strf", strf))
    hdrl = chunk(b"LIST", b"hdrl" + chunk(b"avih", avih) + strl)
    riff_size = 4 + len(hdrl) + 8 + movi_size + 8 + index_size
    path = os.path.join(directory, "recording.avi")
    temporary = path + ".tmp"
    with open(temporary, "wb") as out:
        out.write(b"RIFF" + struct.pack("<I", riff_size) + b"AVI " + hdrl)
        out.write(b"LIST" + struct.pack("<I", movi_size) + b"movi")
        offsets = []
        offset = 4
        for name, length in zip(names, sizes):
            with open(os.path.join(directory, name), "rb") as handle:
                data = handle.read()
            out.write(chunk(b"00dc", data))
            offsets.append((offset, length))
            offset += 8 + length + (length % 2)
        out.write(b"idx1" + struct.pack("<I", index_size))
        for offset, length in offsets:
            out.write(b"00dc" + struct.pack("<III", 0x10, offset, length))
    os.replace(temporary, path)
    return {"path": path, "bytes": os.path.getsize(path), "width": width, "height": height,
            "codec": "MJPEG", "container": "AVI", "mime_type": "video/x-msvideo", "frames": len(names),
            "fps": fps, "duration_ms": round(len(names) * 1000 / fps)}


def make_video(directory, info, deadline):
    names, captured = frames(directory)
    if not names:
        return {"error": "The recording has no frames"}
    with open(os.path.join(directory, names[0]), "rb") as handle:
        size = jpeg_size(handle.read())
    if not size:
        return {"error": "The first frame is not a readable JPEG image"}
    fps = float(info.get("fps") or 2)
    try:
        return write_mp4(directory, names, schedule(captured, fps, info.get("ended_at")),
                         video_size(*size), fps, deadline)
    except Exception as error:
        reason = str(error)
    video = write_avi(directory, names, size, fps)
    video["fallback"] = reason
    return video


def status(directory):
    """The recorder's status file, read as app_control.py's recording-status reads it."""
    try:
        with open(os.path.join(directory, "status")) as handle:
            return json.load(handle)
    except (OSError, ValueError):
        pass
    if os.path.isdir(directory):
        return {"state": "starting", "frames": 0, "bytes": 0}
    return {"state": "unknown", "frames": 0, "bytes": 0,
            "error": "Recording files are not available; the desktop may have restarted"}


def record(directory, info):
    temporary = os.path.join(directory, "status.tmp")
    with open(temporary, "w") as handle:
        json.dump(info, handle)
    os.replace(temporary, os.path.join(directory, "status"))


def lock(directory, deadline):
    """Hold the recording's folder against a second encoder, such as an export during a stop."""
    descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    while True:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return descriptor
        except BlockingIOError:
            if time.monotonic() >= deadline:
                os.close(descriptor)
                return None
            time.sleep(0.2)


def video(directory, deadline):
    """The recording's state, with its video made first when it has finished without one."""
    try:
        descriptor = lock(directory, deadline)
    except OSError:
        return status(directory)
    if descriptor is None:
        return status(directory)
    try:
        info = status(directory)
        if info.get("state") == "finished" and info.get("frames") and not info.get("video"):
            try:
                info["video"] = make_video(directory, info, deadline)
            except Exception as error:
                info["video"] = {"error": str(error)}
            record(directory, info)
        return info
    finally:
        os.close(descriptor)


def main(arguments):
    seconds, directories = float(arguments[0]), arguments[1:]
    deadline = time.monotonic() + seconds
    os.nice(10)
    if directories:
        # GStreamer's plugin cache goes beside the recordings, not into the desktop user's home.
        os.environ.setdefault("GST_REGISTRY", os.path.join(
            os.path.dirname(directories[0].rstrip("/")), "gstreamer-registry.bin"))
    print(json.dumps([video(directory, deadline) for directory in directories]))


if __name__ == "__main__":
    main(sys.argv[1:])
