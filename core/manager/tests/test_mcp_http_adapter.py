"""The adapter between the manager's stdio and a Streamable HTTP MCP server on the desktop's
loopback: the configuration arrives on stdin, so header values never reach a command line; the
server's session and protocol version are carried; JSON and event-stream answers both come back
one message per line; and the session is ended when the manager closes stdin."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import io
import json
from pathlib import Path
import threading
import unittest

ADAPTER = Path(__file__).resolve().parents[1] / "helpers/mcp_http_adapter.py"
spec = importlib.util.spec_from_file_location("mcp_http_adapter", ADAPTER)
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


class Server(BaseHTTPRequestHandler):
    seen = []

    def log_message(self, *arguments):
        pass

    def record(self, body=None):
        Server.seen.append({"method": self.command, "body": body,
                            "headers": {k.lower(): v for k, v in self.headers.items()}})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.record(body)
        if "id" not in body:
            self.send_response(202)
            self.end_headers()
            return
        if body["method"] == "initialize":
            reply = json.dumps({"jsonrpc": "2.0", "id": body["id"],
                                "result": {"protocolVersion": "2025-06-18", "capabilities": {},
                                           "serverInfo": {"name": "fake", "version": "1"}}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Mcp-Session-Id", "session-1")
            self.send_header("Content-Length", str(len(reply)))
            self.end_headers()
            self.wfile.write(reply)
            return
        # Other requests answer on an event stream with a notification before the result.
        events = [{"jsonrpc": "2.0", "method": "notifications/progress", "params": {"progress": 1}},
                  {"jsonrpc": "2.0", "id": body["id"], "result": {"tools": [{"name": "navigate"}]}}]
        stream = b"".join(b"event: message\ndata: " + json.dumps(e).encode() + b"\n\n" for e in events)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(stream)))
        self.end_headers()
        self.wfile.write(stream)

    def do_GET(self):
        self.record()
        self.send_response(405)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_DELETE(self):
        self.record()
        self.send_response(200)
        self.send_header("Content-Length", "0")
        self.end_headers()


class AdapterTest(unittest.TestCase):
    def setUp(self):
        Server.seen = []
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Server)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        self.url = "http://127.0.0.1:%d/mcp" % self.server.server_address[1]

    def run_adapter(self, messages, headers=None):
        lines = [json.dumps({"url": self.url, "headers": headers or {}})]
        lines += [json.dumps(m) for m in messages]
        source = io.BytesIO(("\n".join(lines) + "\n").encode())
        output, errors = io.StringIO(), io.StringIO()
        adapter.main(source, output, errors)
        return [json.loads(line) for line in output.getvalue().splitlines()], errors.getvalue()

    def test_messages_go_through_with_the_servers_session_and_version(self):
        replies, _ = self.run_adapter([
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        ], headers={"Authorization": "Bearer hunter2"})
        self.assertEqual(replies[0]["result"]["serverInfo"]["name"], "fake")
        self.assertIn({"jsonrpc": "2.0", "id": 2, "result": {"tools": [{"name": "navigate"}]}}, replies)
        self.assertIn("notifications/progress", [r.get("method") for r in replies])
        posts = [s for s in Server.seen if s["method"] == "POST"]
        self.assertEqual(posts[0]["headers"]["authorization"], "Bearer hunter2")
        self.assertNotIn("mcp-session-id", posts[0]["headers"])
        for later in posts[1:]:
            self.assertEqual(later["headers"]["mcp-session-id"], "session-1")
            self.assertEqual(later["headers"]["mcp-protocol-version"], "2025-06-18")
        # The session ends with stdin.
        deletes = [s for s in Server.seen if s["method"] == "DELETE"]
        self.assertEqual(len(deletes), 1)
        self.assertEqual(deletes[0]["headers"]["mcp-session-id"], "session-1")

    def test_an_unreachable_server_answers_the_request_with_an_error(self):
        self.server.shutdown()
        self.server.server_close()
        replies, _ = self.run_adapter([{"jsonrpc": "2.0", "id": 7, "method": "tools/list"}])
        self.assertEqual(replies[0]["id"], 7)
        self.assertIn("could not be reached", replies[0]["error"]["message"])

    def test_only_the_desktops_loopback_is_reached(self):
        for url in ("http://example.com:80/mcp", "http://10.0.0.1:80/mcp", "http://127.0.0.1/mcp",
                    "file:///etc/passwd"):
            with self.assertRaises(ValueError, msg=url):
                adapter.Adapter(url, {}, io.StringIO(), io.StringIO())


if __name__ == "__main__":
    unittest.main()
