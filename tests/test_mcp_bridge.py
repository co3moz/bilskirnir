import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import unittest
from email.message import Message
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("mcp_bridge", Path(__file__).resolve().parents[1] / "scripts/mcp_bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class BridgeTest(unittest.TestCase):
    def setUp(self):
        self.requests = []
        self.authorization = "Bearer test-secret"
        self.session_id = None
        received = self.requests
        test = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                message = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                received.append((message, dict(self.headers)))
                if self.headers.get("Authorization") != test.authorization:
                    self.send_response(401)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                if message.get("method", "").startswith("notifications/"):
                    self.send_response(202)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                result = {"jsonrpc": "2.0", "id": message.get("id"), "result": {"ok": True}}
                if message["method"] == "initialize":
                    result["result"]["protocolVersion"] = bridge.PROTOCOL
                data = json.dumps(result).encode()
                stream = message["method"] == "stream"
                if stream:
                    data = b"event: message\ndata: " + data + b"\n\n"
                self.send_response(200)
                if test.session_id:
                    self.send_header("Mcp-Session-Id", test.session_id)
                self.send_header("Content-Type", "text/event-stream" if stream else "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = f"http://127.0.0.1:{self.server.server_port}/mcp"

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def invoke(self, data, token="test-secret"):
        output, errors = io.StringIO(), io.StringIO()
        bridge.run(self.url, token, io.BytesIO(data), output, errors)
        return [json.loads(line) for line in output.getvalue().splitlines()], errors.getvalue()

    def test_authentication_negotiation_and_concurrent_messages(self):
        messages = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2026-07-28"}},
                    {"jsonrpc": "2.0", "method": "notifications/initialized"},
                    *({"jsonrpc": "2.0", "id": n, "method": "ping"} for n in range(2, 12))]
        output, errors = self.invoke(b"\n".join(json.dumps(m).encode() for m in messages) + b"\n")
        self.assertEqual({m["id"] for m in output}, set(range(1, 12)))
        self.assertEqual(errors, "")
        self.assertTrue(all(headers["Authorization"] == "Bearer test-secret" for _, headers in self.requests))
        self.assertEqual(next(m for m, _ in self.requests if m["method"] == "initialize")["params"]["protocolVersion"], bridge.PROTOCOL)
        self.assertTrue(all(headers["Mcp-Protocol-Version"] == bridge.PROTOCOL for _, headers in self.requests))

    def test_sse_is_decoded_to_stdio_json(self):
        class Response(io.BytesIO):
            status = 200
            headers = Message()
            headers["Content-Type"] = "text/event-stream"

        class Client:
            def open(self, *_args, **_kwargs):
                return Response(b'event: message\ndata: {"jsonrpc":"2.0","id":1,"result":{"ok":true}}\n\n')

        with patch.object(bridge.urllib.request, "build_opener", return_value=Client()):
            output, errors = self.invoke(b'{"jsonrpc":"2.0","id":1,"method":"stream"}\n')
        self.assertEqual(output[0]["result"], {"ok": True})
        self.assertEqual(errors, "")

    def test_http_errors_do_not_expose_credentials(self):
        output, errors = self.invoke(b'{"jsonrpc":"2.0","id":"request","method":"ping"}\n', token="private-token")
        self.assertEqual(output[0]["id"], "request")
        self.assertEqual(output[0]["error"]["code"], -32000)
        self.assertNotIn("private-token", json.dumps(output) + errors)

    def test_invalid_and_oversized_input_does_not_reach_http(self):
        output, _ = self.invoke(b"{\n" + b"x" * bridge.REQUEST_LIMIT + b"\n")
        self.assertEqual([m["error"]["code"] for m in output], [-32700, -32600])
        self.assertEqual(self.requests, [])

    def test_the_session_an_endpoint_assigns_is_named_on_every_later_request(self):
        # A scoped endpoint answers initialize with a session id that every later request must
        # carry; /mcp assigns none, and then no header is sent.
        self.session_id = "session-1"
        lines = (b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n'
                 b'{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n'
                 b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        replies, _ = self.invoke(lines)
        self.assertEqual(sorted(reply["id"] for reply in replies), [1, 2])
        by_method = {message["method"]: headers for message, headers in self.requests}
        self.assertNotIn("Mcp-Session-Id", by_method["initialize"])
        self.assertEqual(by_method["tools/list"].get("Mcp-Session-Id"), "session-1")
        self.assertEqual(by_method["notifications/initialized"].get("Mcp-Session-Id"), "session-1")
        self.requests.clear()
        self.session_id = None
        self.invoke(lines)
        self.assertTrue(all("Mcp-Session-Id" not in headers for _, headers in self.requests))

    def test_a_tokenless_manager_gets_no_authorization_header(self):
        self.authorization = None
        output, errors = self.invoke(b'{"jsonrpc":"2.0","id":1,"method":"ping"}\n', token="")
        self.assertEqual(output[0]["result"], {"ok": True})
        self.assertEqual(errors, "")
        self.assertNotIn("Authorization", self.requests[0][1])

    def test_only_the_tokenless_mode_starts_without_a_token(self):
        script = Path(bridge.__file__)
        for mode, code in ((None, 1), ("token", 1), ("none", 0)):
            env = {k: v for k, v in os.environ.items() if k not in ("MCP_TOKEN", "MCP_AUTH_MODE")}
            if mode:
                env["MCP_AUTH_MODE"] = mode
            result = subprocess.run([sys.executable, "-I", str(script)], input=b"", env=env,
                                    capture_output=True, timeout=30)
            self.assertEqual(result.returncode, code, mode)
            self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    unittest.main()
