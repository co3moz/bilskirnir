"""Capture desktop or microphone audio as WAV, or play WAV data into a PulseAudio sink, as the desktop user."""
import base64
import json
import math
import struct
import subprocess
import sys
import time

RATE, CHANNELS = 48000, 2


def capture(seconds, source):
    total = int(seconds * RATE * CHANNELS * 2)
    process = subprocess.Popen(["parec", "--device=" + source, "--format=s16le", "--rate=%d" % RATE,
                                "--channels=%d" % CHANNELS, "--raw", "--latency-msec=40"],
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    data = b""
    deadline = time.monotonic() + seconds + 5
    try:
        while len(data) < total and time.monotonic() < deadline:
            chunk = process.stdout.read(min(65536, total - len(data)))
            if not chunk:
                break
            data += chunk
    finally:
        process.terminate()
        process.wait(timeout=5)
    if not data:
        raise RuntimeError("No audio data was captured; check that audio is enabled for this environment")
    samples = struct.unpack("<%dh" % (len(data) // 2), data[:len(data) // 2 * 2])
    peak = max(abs(sample) for sample in samples) / 32768
    rms = math.sqrt(sum(sample * sample for sample in samples) / len(samples)) / 32768
    header = (b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVE" + b"fmt "
              + struct.pack("<IHHIIHH", 16, 1, CHANNELS, RATE, RATE * CHANNELS * 2, CHANNELS * 2, 16)
              + b"data" + struct.pack("<I", len(data)))
    return {"wav_base64": base64.b64encode(header + data).decode(), "source": source,
            "seconds": round(len(data) / (RATE * CHANNELS * 2), 3), "sample_rate": RATE,
            "channels": CHANNELS, "peak": round(peak, 4), "rms": round(rms, 4), "silent": peak < 0.001}


def play(sink):
    data = sys.stdin.buffer.read()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("Provide 16-bit PCM WAV data")
    position, fmt, pcm = 12, None, None
    while position + 8 <= len(data):
        ident = data[position:position + 4]
        size = struct.unpack("<I", data[position + 4:position + 8])[0]
        body = data[position + 8:position + 8 + size]
        if ident == b"fmt ":
            fmt = struct.unpack("<HHIIHH", body[:16])
        elif ident == b"data":
            pcm = body
        position += 8 + size + (size % 2)
    if fmt is None or pcm is None:
        raise ValueError("WAV data is missing its fmt or data chunk")
    audio_format, channels, rate, _, _, bits = fmt
    if audio_format != 1 or bits != 16 or channels not in (1, 2) or not 8000 <= rate <= 192000:
        raise ValueError("Provide 16-bit PCM WAV with one or two channels")
    process = subprocess.Popen(["pacat", "--playback", "--device=" + sink, "--format=s16le",
                                "--rate=%d" % rate, "--channels=%d" % channels, "--raw"],
                               stdin=subprocess.PIPE, stderr=subprocess.DEVNULL)
    process.stdin.write(pcm)
    process.stdin.close()
    code = process.wait(timeout=120)
    if code:
        raise RuntimeError("pacat exited with status %d" % code)
    return {"sink": sink, "seconds": round(len(pcm) / (rate * channels * 2), 3),
            "sample_rate": rate, "channels": channels}


command, *arguments = sys.argv[1:]
try:
    if command == "capture":
        print(json.dumps(capture(float(arguments[0]), arguments[1])))
    elif command == "play":
        print(json.dumps(play(arguments[0])))
    else:
        sys.exit(2)
except Exception as error:
    print("Audio operation failed: " + str(error), file=sys.stderr)
    sys.exit(1)
