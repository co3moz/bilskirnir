"""A recording's video: each frame stays on screen from its capture time until the next one, the
picture has even sides within the area browsers decode, a desktop image without GStreamer's
H.264 encoder gets the MJPEG AVI with the reason, a recording still running or without frames
gets none, a second encoder waits for the first, and the frames stay. Where GStreamer with x264enc
is installed, as in the desktop images, the MP4 itself is checked."""

import fcntl
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest

HELPER = Path(__file__).resolve().parents[1] / "helpers/recording_video.py"
spec = importlib.util.spec_from_file_location("recording_video", HELPER)
recording_video = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recording_video)

# Runs the helper as the manager does, with GStreamer's Python bindings out of reach, as in an
# XFCE desktop image from before the encoder.
WITHOUT_GSTREAMER = ("import sys; sys.modules['gi'] = None; path = sys.argv.pop(1); "
                     "exec(compile(open(path).read(), path, 'exec'))")


def gstreamer():
    """GStreamer with every element the video and these tests need, or None."""
    try:
        import gi
        gi.require_version("Gst", "1.0")
        from gi.repository import Gst
    except (ImportError, ValueError):
        return None
    Gst.init(None)
    needed = recording_video.ELEMENTS + ("videotestsrc", "jpegenc")
    return Gst if all(Gst.ElementFactory.find(name) for name in needed) else None


def jpeg_header(width, height):
    """The start of a baseline JPEG, enough for its size to be read."""
    return (b"\xff\xd8\xff\xc0\x00\x11\x08" + struct.pack(">HH", height, width)
            + b"\x03\x01\x22\x00\x02\x11\x01\x03\x11\x01\xff\xd9")


def boxes(data, start=0, end=None):
    """The MP4 boxes between start and end as (type, offset, size)."""
    found, position, end = [], start, len(data) if end is None else end
    while position + 8 <= end:
        size, kind = struct.unpack(">I4s", data[position:position + 8])
        if size < 8:
            break
        found.append((kind.decode("latin-1"), position, size))
        position += size
    return found


class ScheduleTest(unittest.TestCase):
    def test_a_frame_stays_until_the_next_was_captured(self):
        plan = recording_video.schedule([1000, 1500, 3500, 3600], 2, ended_at=4600)
        self.assertEqual(plan, [(0, 500), (500, 2000), (2500, 100), (2600, 1000)])

    def test_without_an_end_the_last_frame_lasts_one_frame_period(self):
        self.assertEqual(recording_video.schedule([10, 260], 4), [(0, 250), (250, 250)])
        self.assertEqual(recording_video.schedule([10], 5, ended_at=5), [(0, 200)])

    def test_a_clock_step_back_keeps_the_order(self):
        plan = recording_video.schedule([5000, 4000, 6000], 1)
        self.assertEqual([start for start, _ in plan], [0, 1, 1000])
        self.assertTrue(all(length >= 1 for _, length in plan))


class SizeTest(unittest.TestCase):
    def test_sides_are_even(self):
        self.assertEqual(recording_video.video_size(961, 541), (960, 540))
        self.assertEqual(recording_video.video_size(1, 1), (2, 2))

    def test_a_picture_larger_than_4k_is_scaled_down(self):
        width, height = recording_video.video_size(7680, 4320)
        self.assertLessEqual(width * height, 3840 * 2160)
        self.assertEqual((width % 2, height % 2), (0, 0))
        self.assertAlmostEqual(width / height, 16 / 9, places=2)
        self.assertEqual(recording_video.video_size(3840, 2160), (3840, 2160))


class Recording(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.directory = self.root / "abc"
        self.directory.mkdir()

    def status(self, **fields):
        info = {"state": "finished", "frames": 0, "bytes": 0, "started_at": 1700000000000,
                "ended_at": None, "reason": "stopped", "fps": 2}
        info.update(fields)
        (self.directory / "status").write_text(json.dumps(info))

    def run_helper(self, seconds="60", gstreamer_hidden=True, *directories):
        """Run the helper as the manager does, by default as in an image without GStreamer."""
        command = [sys.executable, "-c", WITHOUT_GSTREAMER] if gstreamer_hidden else [sys.executable]
        result = subprocess.run(command + [str(HELPER), seconds] + [str(d) for d in directories or [self.directory]],
                                capture_output=True, timeout=120,
                                env={**os.environ, "GST_REGISTRY": str(self.root / "registry.bin")})
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        return json.loads(result.stdout)


class FallbackTest(Recording):
    def frames(self, count):
        for index in range(count):
            (self.directory / ("frame-%05d-%d.jpg" % (index, 1700000000000 + index * 500))).write_bytes(
                jpeg_header(321, 241))
        self.status(frames=count, ended_at=1700000000000 + count * 500)

    def test_without_an_encoder_the_frames_go_into_an_mjpeg_avi(self):
        self.frames(3)
        [info] = self.run_helper()
        video = info["video"]
        self.assertEqual((video["codec"], video["container"], video["mime_type"]), ("MJPEG", "AVI", "video/x-msvideo"))
        self.assertEqual(video["path"], str(self.directory / "recording.avi"))
        self.assertEqual((video["width"], video["height"], video["frames"]), (321, 241, 3))
        self.assertIn("GStreamer", video["fallback"])
        data = (self.directory / "recording.avi").read_bytes()
        self.assertEqual((data[:4], data[8:12]), (b"RIFF", b"AVI "))
        self.assertEqual(video["bytes"], len(data))
        self.assertEqual(json.loads((self.directory / "status").read_text())["video"], video)
        self.assertEqual(len(list(self.directory.glob("frame-*.jpg"))), 3)
        self.assertEqual(list(self.directory.glob("*.tmp")), [])

    def test_a_video_is_made_once(self):
        self.frames(2)
        [first] = self.run_helper()
        modified = (self.directory / "recording.avi").stat().st_mtime_ns
        [second] = self.run_helper()
        self.assertEqual(first, second)
        self.assertEqual((self.directory / "recording.avi").stat().st_mtime_ns, modified)

    def test_a_running_recording_or_one_without_frames_gets_no_video(self):
        self.frames(2)
        self.status(state="recording", frames=2)
        [info] = self.run_helper()
        self.assertNotIn("video", info)
        self.status(frames=0)
        for frame in self.directory.glob("frame-*.jpg"):
            frame.unlink()
        [info] = self.run_helper()
        self.assertNotIn("video", info)
        self.assertFalse((self.directory / "recording.avi").exists())

    def test_a_missing_recording_is_unknown(self):
        [info] = self.run_helper("60", True, self.root / "gone")
        self.assertEqual(info["state"], "unknown")

    def test_a_second_encoder_waits_for_the_first(self):
        self.frames(2)
        descriptor = os.open(self.directory, os.O_RDONLY | os.O_DIRECTORY)
        self.addCleanup(os.close, descriptor)
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        [info] = self.run_helper("0.5")
        self.assertNotIn("video", info)
        self.assertFalse((self.directory / "recording.avi").exists())
        fcntl.flock(descriptor, fcntl.LOCK_UN)
        [info] = self.run_helper()
        self.assertEqual(info["video"]["container"], "AVI")

    def test_frames_go_in_the_order_they_were_taken(self):
        for index in (9, 10, 2):
            (self.directory / ("frame-%05d-%d.jpg" % (index, 1000 + index))).write_bytes(b"")
        (self.directory / "status").write_text("{}")
        (self.directory / "recording.avi").write_bytes(b"")
        names, captured = recording_video.frames(self.directory)
        self.assertEqual(names, ["frame-00002-1002.jpg", "frame-00009-1009.jpg", "frame-00010-1010.jpg"])
        self.assertEqual(captured, [1002, 1009, 1010])


@unittest.skipUnless(gstreamer(), "GStreamer with x264enc, mp4mux and jpegenc is not installed")
class EncoderTest(Recording):
    def frames(self, moments, size=(321, 241)):
        Gst = gstreamer()
        for index, moment in enumerate(moments):
            path = self.directory / ("frame-%05d-%d.jpg" % (index, moment))
            pipeline = Gst.parse_launch(
                "videotestsrc num-buffers=1 pattern=%d ! video/x-raw,width=%d,height=%d ! jpegenc ! filesink location=%s"
                % (index % 20, size[0], size[1], path))
            pipeline.set_state(Gst.State.PLAYING)
            pipeline.get_bus().timed_pop_filtered(10 * Gst.SECOND, Gst.MessageType.EOS | Gst.MessageType.ERROR)
            pipeline.set_state(Gst.State.NULL)
        self.status(frames=len(moments), ended_at=moments[-1] + 700)

    def test_the_frames_become_an_h264_mp4_at_their_capture_times(self):
        start = 1700000000000
        self.frames([start, start + 500, start + 2500, start + 2600])
        [info] = self.run_helper("60", False)
        video = info["video"]
        self.assertNotIn("fallback", video)
        self.assertEqual((video["codec"], video["container"], video["mime_type"]), ("H.264", "MP4", "video/mp4"))
        self.assertEqual((video["width"], video["height"], video["frames"]), (320, 240, 4))
        self.assertEqual(video["duration_ms"], 3300)
        data = (self.directory / "recording.mp4").read_bytes()
        top = [kind for kind, _, _ in boxes(data)]
        self.assertEqual(top[0], "ftyp")
        self.assertLess(top.index("moov"), top.index("mdat"), "the index comes first, so playback starts at once")
        _, offset, size = next(box for box in boxes(data) if box[0] == "moov")
        moov = data[offset:offset + size]
        self.assertIn(b"avc1", moov)
        self.assertIn(b"avcC", moov)
        _, header, _ = next(box for box in boxes(moov, 8) if box[0] == "mvhd")
        if moov[header + 8]:
            timescale, duration = struct.unpack(">IQ", moov[header + 28:header + 40])
        else:
            timescale, duration = struct.unpack(">II", moov[header + 20:header + 28])
        self.assertAlmostEqual(duration / timescale, 3.3, delta=0.05)
        self.assertEqual(len(list(self.directory.glob("frame-*.jpg"))), 4)
        self.assertEqual(list(self.directory.glob("*.tmp")), [])
        self.assertFalse((self.directory / "recording.avi").exists())

    def test_without_time_left_the_avi_is_made_instead(self):
        self.frames([1000, 1500])
        [info] = self.run_helper("0", False)
        self.assertEqual(info["video"]["container"], "AVI")
        self.assertIn("time", info["video"]["fallback"])
        self.assertFalse((self.directory / "recording.mp4").exists())


if __name__ == "__main__":
    unittest.main()
