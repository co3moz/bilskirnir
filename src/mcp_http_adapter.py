"""Speak MCP over stdio for a Streamable HTTP server that listens on the desktop's loopback.

The manager runs this as the desktop user and writes one JSON line first: {"url": ..., "headers":
{...}}, so header values stay out of the process list. Every later line is a JSON-RPC message for
the server; its answers, and what it sends on its own, come back one per line on stdout, which
carries nothing else. The server's session is kept and ended when stdin closes.
"""
from concurrent.futures import ThreadPoolExecutor
import json
import sys
import threading
import urllib.error
import urllib.parse
import urllib.request

REQUEST_LIMIT = 4 * 1024 * 1024
RESPONSE_LIMIT = 16 * 1024 * 1024
LOOPBACK = ("127.0.0.1", "localhost", "::1")


class Adapter:
    def __init__(self, url, headers, output, errors):
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme not in ("http", "https") or parsed.hostname not in LOOPBACK or not parsed.port:
            raise ValueError("The server URL must be http(s) on the desktop's loopback with a port")
        self.url = url
        self.headers = dict(headers)
        self.output = output
        self.errors = errors
        self.lock = threading.Lock()
        self.session = None
        self.version = None
        self.listening = False
        self.client = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def emit(self, message):
        with self.lock:
            self.output.write(json.dumps(message, ensure_ascii=True, separators=(",", ":")) + "\n")
            self.output.flush()

    def note(self, text):
        with self.lock:
            self.errors.write(text + "\n")
            self.errors.flush()

    def request_headers(self):
        headers = dict(self.headers)
        headers["Accept"] = "application/json, text/event-stream"
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        if self.version:
            headers["MCP-Protocol-Version"] = self.version
        return headers

    def relay(self, response, initialize):
        """Write what a response carries: one JSON body, or the messages of an event stream."""
        if initialize:
            self.session = response.headers.get("Mcp-Session-Id") or self.session
        mime = response.headers.get_content_type()
        if response.status in (202, 204):
            return
        if mime == "application/json":
            data = response.read(RESPONSE_LIMIT + 1)
            if len(data) > RESPONSE_LIMIT:
                raise ValueError("The server's response exceeds 16 MiB")
            if data.strip():
                self.answer(json.loads(data), initialize)
            return
        if mime != "text/event-stream":
            raise ValueError("The server answered with " + mime)
        pending = []
        received = 0
        while line := response.readline(RESPONSE_LIMIT + 1):
            received += len(line)
            if received > RESPONSE_LIMIT:
                raise ValueError("The server's event stream exceeds 16 MiB")
            line = line.rstrip(b"\r\n")
            if not line:
                if pending:
                    self.answer(json.loads(b"\n".join(pending)), initialize)
                    pending = []
            elif line.startswith(b"data:"):
                data = line[5:].lstrip(b" ")
                if data:
                    pending.append(data)
        if pending:
            self.answer(json.loads(b"\n".join(pending)), initialize)

    def answer(self, message, initialize):
        if initialize and isinstance(message, dict) and isinstance(message.get("result"), dict):
            version = message["result"].get("protocolVersion")
            if isinstance(version, str):
                self.version = version
        self.emit(message)

    def forward(self, message):
        initialize = message.get("method") == "initialize"
        try:
            body = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            headers = self.request_headers()
            headers["Content-Type"] = "application/json"
            request = urllib.request.Request(self.url, data=body, headers=headers, method="POST")
            with self.client.open(request, timeout=900) as response:
                self.relay(response, initialize)
            if message.get("method") == "notifications/initialized":
                self.listen()
        except urllib.error.HTTPError as error:
            self.fail(message, "The server answered HTTP " + str(error.code))
        except Exception as error:
            # The URL's credentials and the headers never reach the output.
            self.fail(message, "The server could not be reached: " + type(error).__name__)

    def fail(self, message, text):
        if "id" in message and "method" in message:
            self.emit({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32000, "message": text}})
        else:
            self.note(text)

    def listen(self):
        """Receive what the server sends on its own, on the stream a GET opens, while it lasts."""
        if self.listening or not self.session:
            return
        self.listening = True

        def stream():
            try:
                request = urllib.request.Request(self.url, headers=self.request_headers(), method="GET")
                request.add_header("Accept", "text/event-stream")
                with self.client.open(request, timeout=None) as response:
                    self.relay(response, False)
            except urllib.error.HTTPError as error:
                if error.code != 405:
                    self.note("The server's notification stream answered HTTP " + str(error.code))
            except Exception as error:
                self.note("The server's notification stream ended: " + type(error).__name__)

        threading.Thread(target=stream, daemon=True).start()

    def close(self):
        if not self.session:
            return
        try:
            request = urllib.request.Request(self.url, headers=self.request_headers(), method="DELETE")
            with self.client.open(request, timeout=5):
                pass
        except Exception:
            pass

    def run(self, source):
        with ThreadPoolExecutor(max_workers=8) as pool:
            while line := source.readline(REQUEST_LIMIT + 1):
                if len(line) > REQUEST_LIMIT:
                    self.note("A message for the server exceeds 4 MiB")
                    return
                if not line.strip():
                    continue
                try:
                    message = json.loads(line)
                except (ValueError, UnicodeDecodeError):
                    self.note("The manager sent a line that is not JSON")
                    continue
                if not isinstance(message, dict):
                    continue
                # The handshake comes first and alone, so the session it opens is known to the rest.
                if message.get("method") in ("initialize", "notifications/initialized"):
                    self.forward(message)
                else:
                    pool.submit(self.forward, message)
        self.close()


def main(source, output, errors):
    first = source.readline(REQUEST_LIMIT + 1)
    config = json.loads(first)
    headers = config.get("headers") or {}
    if not isinstance(headers, dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in headers.items()):
        raise ValueError("headers must map names to text")
    Adapter(config["url"], headers, output, errors).run(source)


if __name__ == "__main__":
    try:
        main(sys.stdin.buffer, sys.stdout, sys.stderr)
    except (ValueError, KeyError) as error:
        sys.stderr.write("MCP adapter: " + str(error) + "\n")
        sys.exit(1)
