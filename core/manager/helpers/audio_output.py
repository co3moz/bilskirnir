"""Stop the container-side recorder when the docker exec client disconnects."""
import subprocess
import sys
import threading

process = subprocess.Popen([
    "parec", "--device=desktop-output.monitor", "--format=float32le",
    "--rate=48000", "--channels=2", "--latency-msec=40",
], stdout=sys.stdout.buffer, stderr=subprocess.DEVNULL)

def disconnected():
    # docker exec -i closes the remote stdin when its local client is dropped.
    while sys.stdin.buffer.read(1):
        pass
    if process.poll() is None:
        process.terminate()

threading.Thread(target=disconnected, daemon=True).start()
sys.exit(process.wait())
