import importlib.util
import io
import json
from pathlib import Path
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
        received = self.requests

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                message = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                received.append((message, dict(self.headers)))
                if self.headers.get("Authorization") != "Bearer test-secret":
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


if __name__ == "__main__":
    unittest.main()
