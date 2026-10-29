import importlib.util
import io
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
import unittest
from email.message import Message
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("mcp_bridge", Path(__file__).resolve().parents[1] / "mcp_bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)

LIST_CHANGED = b'data: {"jsonrpc":"2.0","method":"notifications/tools/list_changed"}\n\n'
LOG = {"jsonrpc": "2.0", "method": "notifications/message", "params": {"level": "info", "data": "logged"}}
# What rmcp sends first on a session's stream, and between events.
PRIMING = b"id: 0\nretry: 3000\ndata:\n\n"
KEEP_ALIVE = b": keep-alive\n\n"
INITIALIZE = b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}\n'
INITIALIZED = b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'


class Source:
    """stdin that stays open until the test closes it, as a client's does."""

    def __init__(self, lines):
        self.lines = queue.Queue()
        for line in lines:
            self.lines.put(line)

    def readline(self, _limit):
        return self.lines.get()

    def close(self):
        self.lines.put(b"")


class BridgeTest(unittest.TestCase):
    def setUp(self):
        self.requests = []
        self.authorization = "Bearer test-secret"
        self.session_id = None
        # What each GET of a session's stream gets in turn: a status, or the events of a stream
        # that stays open while keep_open is set and ends otherwise.
        self.streams = []
        self.stream_replies = []
        self.keep_open = False
        self.closing = threading.Event()
        # The sessions each initialize opens in turn, those the server has forgotten, the
        # requests it answers 404 in any case, and the capabilities initialize answers with.
        self.next_sessions = []
        self.forgotten = set()
        self.refused = lambda message, session: False
        self.capabilities = None
        received = self.requests
        test = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def empty(self, status):
                self.send_response(status)
                self.send_header("Content-Length", "0")
                self.end_headers()

            def do_GET(self):
                test.streams.append(dict(self.headers))
                session = self.headers.get("Mcp-Session-Id")
                if session in test.forgotten:
                    reply = 404
                else:
                    reply = test.stream_replies.pop(0) if test.stream_replies else 405
                if isinstance(reply, int):
                    self.empty(reply)
                    return
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                self.wfile.write(reply)
                self.wfile.flush()
                # The stream stays open until the server forgets its session.
                while test.keep_open and session not in test.forgotten and not test.closing.wait(0.01):
                    pass

            def do_POST(self):
                message = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                received.append((message, dict(self.headers)))
                if self.headers.get("Authorization") != test.authorization:
                    self.empty(401)
                    return
                session = self.headers.get("Mcp-Session-Id")
                if session in test.forgotten or test.refused(message, session):
                    self.empty(404)
                    return
                if message.get("method", "").startswith("notifications/"):
                    self.empty(202)
                    return
                result = {"jsonrpc": "2.0", "id": message.get("id"), "result": {"ok": True}}
                if message["method"] == "initialize":
                    if test.next_sessions:
                        test.session_id = test.next_sessions.pop(0)
                    result["result"]["protocolVersion"] = bridge.PROTOCOL
                    if test.capabilities:
                        result["result"]["capabilities"] = test.capabilities
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
        self.closing.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def invoke(self, data, token="test-secret"):
        output, errors = io.StringIO(), io.StringIO()
        bridge.run(self.url, token, io.BytesIO(data), output, errors)
        return [json.loads(line) for line in output.getvalue().splitlines()], errors.getvalue()

    def start(self, lines):
        """Run the bridge with stdin held open; the test closes it."""
        source, output, errors = Source(lines), io.StringIO(), io.StringIO()
        thread = threading.Thread(target=bridge.run, args=(self.url, "test-secret", source, output, errors), daemon=True)
        thread.start()
        return source, output, errors, thread

    def wait_for(self, condition):
        deadline = time.monotonic() + 10
        while not condition():
            if time.monotonic() > deadline:
                self.fail("the bridge did not get there in 10 seconds")
            time.sleep(0.01)

    @staticmethod
    def lines(output):
        return [json.loads(line) for line in output.getvalue().splitlines()]

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

    def test_an_endpoints_notifications_reach_the_client_on_the_sessions_stream(self):
        # A desktop server added to the endpoint is told on the session's own stream, which a
        # stdio client cannot open itself.
        self.session_id = "session-1"
        self.keep_open = True
        self.stream_replies = [PRIMING + KEEP_ALIVE + LIST_CHANGED]
        source, output, errors, thread = self.start([
            b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n',
            b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n',
        ])
        self.wait_for(lambda: len(self.lines(output)) == 2)
        source.close()
        thread.join(10)
        self.assertEqual(self.lines(output), [
            {"jsonrpc": "2.0", "id": 1, "result": {"ok": True, "protocolVersion": bridge.PROTOCOL}},
            {"jsonrpc": "2.0", "method": "notifications/tools/list_changed"},
        ])
        self.assertEqual(errors.getvalue(), "")
        self.assertEqual(len(self.streams), 1)
        headers = self.streams[0]
        self.assertEqual(headers["Mcp-Session-Id"], "session-1")
        self.assertEqual(headers["Authorization"], "Bearer test-secret")
        self.assertEqual(headers["Accept"], "text/event-stream")
        self.assertEqual(headers["Mcp-Protocol-Version"], bridge.PROTOCOL)

    def test_a_lost_stream_is_opened_again_until_the_session_ends(self):
        self.session_id = "session-1"
        self.stream_replies = [LIST_CHANGED, LIST_CHANGED, 404]
        with patch.object(bridge, "REOPEN_AFTER", 0.05):
            source, output, errors, thread = self.start([
                b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n',
                b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n',
            ])
            self.wait_for(lambda: len(self.streams) == 3)
            # The 404 ends the listener: without a request that renews the session, no fourth
            # request however long the bridge runs.
            time.sleep(0.3)
            source.close()
            thread.join(10)
        self.assertEqual(len(self.streams), 3)
        self.assertEqual([m.get("method") for m in self.lines(output)],
                         [None, "notifications/tools/list_changed", "notifications/tools/list_changed"])
        self.assertEqual(errors.getvalue(), "")

    @staticmethod
    def sent(requests):
        return [(message.get("method"), headers.get("Mcp-Session-Id")) for message, headers in requests]

    def test_a_forgotten_session_is_opened_again_and_the_request_sent_once_more(self):
        # An endpoint forgets a session after five idle minutes, or all of them when the manager
        # restarts, and answers 404 to its ID from then on.
        self.next_sessions = ["session-1", "session-2"]
        self.capabilities = {"tools": {"listChanged": True}}
        self.keep_open = True
        self.stream_replies = [PRIMING, PRIMING + b"data: " + json.dumps(LOG).encode() + b"\n\n"]
        with patch.object(bridge, "REOPEN_AFTER", 0.05):
            source, output, errors, thread = self.start([INITIALIZE, INITIALIZED])
            self.wait_for(lambda: len(self.streams) == 1)
            self.forgotten.add("session-1")
            # The forgotten session's stream ends, and opened again it is refused.
            self.wait_for(lambda: len(self.streams) == 2)
            source.lines.put(b'{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n')
            self.wait_for(lambda: len(self.lines(output)) == 4 and len(self.streams) == 3)
            source.close()
            thread.join(10)
        replies = self.lines(output)
        # The new session's stream reaches the client, whenever it comes.
        self.assertIn(LOG, replies)
        # The answer to the initialize sent again stays with the bridge, and the client lists the
        # tools again, since nothing could tell of a change while there was no session.
        capabilities = {"tools": {"listChanged": True}}
        self.assertEqual([reply for reply in replies if reply != LOG], [
            {"jsonrpc": "2.0", "id": 1, "result": {"ok": True, "protocolVersion": bridge.PROTOCOL,
                                                   "capabilities": capabilities}},
            {"jsonrpc": "2.0", "method": "notifications/tools/list_changed"},
            {"jsonrpc": "2.0", "id": 2, "result": {"ok": True}},
        ])
        self.assertEqual(errors.getvalue(), "")
        self.assertEqual(self.sent(self.requests), [
            ("initialize", None), ("notifications/initialized", "session-1"), ("tools/list", "session-1"),
            ("initialize", None), ("notifications/initialized", "session-2"), ("tools/list", "session-2"),
        ])
        # The client's own initialize, with the protocol the bridge speaks.
        self.assertEqual(self.requests[3][0], self.requests[0][0])
        self.assertEqual(self.requests[3][0]["params"]["protocolVersion"], bridge.PROTOCOL)
        self.assertEqual([headers["Mcp-Session-Id"] for headers in self.streams],
                         ["session-1", "session-1", "session-2"])

    def test_a_request_refused_in_the_new_session_too_fails_instead_of_renewing_again(self):
        self.next_sessions = ["session-1", "session-2"]
        self.refused = lambda message, session: session is not None and message.get("method") == "tools/call"
        source, output, errors, thread = self.start([INITIALIZE, INITIALIZED])
        self.wait_for(lambda: len(self.lines(output)) == 1 and len(self.requests) == 2)
        source.lines.put(b'{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"screenshot"}}\n')
        self.wait_for(lambda: len(self.lines(output)) == 2)
        time.sleep(0.3)
        # The renewed session serves the next request.
        source.lines.put(b'{"jsonrpc":"2.0","id":3,"method":"ping"}\n')
        self.wait_for(lambda: len(self.lines(output)) == 3)
        source.close()
        thread.join(10)
        replies = self.lines(output)
        self.assertEqual(replies[1]["id"], 2)
        self.assertEqual(replies[1]["error"]["code"], -32000)
        self.assertIn("404", replies[1]["error"]["message"])
        # A server that does not say it tells of tool changes is not said to have changed them.
        self.assertEqual(replies[2], {"jsonrpc": "2.0", "id": 3, "result": {"ok": True}})
        self.assertEqual(errors.getvalue(), "")
        self.assertEqual(self.sent(self.requests), [
            ("initialize", None), ("notifications/initialized", "session-1"), ("tools/call", "session-1"),
            ("initialize", None), ("notifications/initialized", "session-2"), ("tools/call", "session-2"),
            ("ping", "session-2"),
        ])

    def test_a_session_that_cannot_be_opened_again_fails_each_request_after_one_attempt(self):
        self.next_sessions = ["session-1"]
        source, output, errors, thread = self.start([INITIALIZE, INITIALIZED])
        self.wait_for(lambda: len(self.lines(output)) == 1 and len(self.requests) == 2)
        # As when the endpoint was deleted: the administrator's token is told 404.
        self.forgotten.add("session-1")
        self.refused = lambda message, session: message.get("method") == "initialize"
        for n in (2, 3):
            source.lines.put(b'{"jsonrpc":"2.0","id":%d,"method":"ping"}\n' % n)
            self.wait_for(lambda: len(self.lines(output)) == n)
        source.close()
        thread.join(10)
        replies = self.lines(output)[1:]
        self.assertEqual([reply["id"] for reply in replies], [2, 3])
        self.assertTrue(all(reply["error"]["code"] == -32000 for reply in replies))
        self.assertEqual(errors.getvalue(), "")
        self.assertEqual(self.sent(self.requests), [
            ("initialize", None), ("notifications/initialized", "session-1"),
            ("ping", "session-1"), ("initialize", None),
            ("ping", "session-1"), ("initialize", None),
        ])

    def test_requests_refused_together_share_one_renewal(self):
        self.next_sessions = ["session-1", "session-2"]
        source, output, errors, thread = self.start([INITIALIZE, INITIALIZED])
        self.wait_for(lambda: len(self.lines(output)) == 1 and len(self.requests) == 2)
        self.forgotten.add("session-1")
        for n in range(2, 8):
            source.lines.put(b'{"jsonrpc":"2.0","id":%d,"method":"ping"}\n' % n)
        self.wait_for(lambda: len(self.lines(output)) == 7)
        source.close()
        thread.join(10)
        replies = self.lines(output)[1:]
        self.assertEqual(sorted(reply["id"] for reply in replies), list(range(2, 8)))
        self.assertTrue(all(reply["result"] == {"ok": True} for reply in replies))
        self.assertEqual(errors.getvalue(), "")
        methods = [method for method, _ in self.sent(self.requests)]
        self.assertEqual(methods.count("initialize"), 2)
        self.assertEqual(methods.count("notifications/initialized"), 2)
        retried = [session for method, session in self.sent(self.requests) if method == "ping"]
        self.assertEqual(retried.count("session-2"), 6)

    def test_a_404_without_a_session_is_the_clients_answer(self):
        # /mcp names no session, and 404 there is not a forgotten one.
        self.refused = lambda message, session: True
        replies, errors = self.invoke(INITIALIZE + b'{"jsonrpc":"2.0","id":2,"method":"ping"}\n')
        self.assertEqual([(reply["id"], reply["error"]["code"]) for reply in replies], [(1, -32000), (2, -32000)])
        self.assertEqual(errors, "")
        self.assertEqual(self.sent(self.requests), [("initialize", None), ("ping", None)])

    def test_mcp_without_a_session_opens_no_stream(self):
        replies, _ = self.invoke(b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n'
                                 b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        time.sleep(0.2)
        self.assertEqual([reply["id"] for reply in replies], [1])
        self.assertEqual(self.streams, [])

    def test_a_sessions_stream_is_bounded_per_event_and_an_answer_as_a_whole(self):
        # The session's stream stays open for hours, so keep-alives must not add up to a limit.
        data = (KEEP_ALIVE * 8) + LIST_CHANGED
        with patch.object(bridge, "RESPONSE_LIMIT", len(LIST_CHANGED) + 8):
            self.assertEqual(len(list(bridge.events(io.BytesIO(data), whole=False))), 1)
            with self.assertRaises(ValueError):
                list(bridge.events(io.BytesIO(data), whole=True))
            self.assertEqual(len(list(bridge.events(io.BytesIO(LIST_CHANGED * 2), whole=False))), 2)
            with self.assertRaises(ValueError):
                list(bridge.events(io.BytesIO(b"data: " + b"x" * 2 * len(LIST_CHANGED) + b"\n\n"), whole=False))

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
