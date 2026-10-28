"""The desktop's own proxy: it answers 503 until the manager gives it the container's settings;
it goes to an http proxy with CONNECT, or with the address in full for plain HTTP, adding the
user name and password that programs never see and dropping the ones they send; it signs in to
a SOCKS5 proxy and leaves the name to it; a refusal reaches the program without a challenge that
would make it ask for a password; and loopback and bypassed hosts go straight to their address."""

import asyncio
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import socket
import struct
import tempfile
import threading
import unittest

HELPER = Path(__file__).resolve().parents[1] / "helpers/outbound_proxy.py"
spec = importlib.util.spec_from_file_location("outbound_proxy", HELPER)
outbound = importlib.util.module_from_spec(spec)
spec.loader.exec_module(outbound)


def tunnel(one, other):
    def copy(source, destination):
        try:
            while True:
                data = source.recv(65536)
                if not data:
                    break
                destination.sendall(data)
        except OSError:
            pass
        finally:
            try:
                destination.shutdown(socket.SHUT_WR)
            except OSError:
                pass

    back = threading.Thread(target=copy, args=(other, one), daemon=True)
    back.start()
    copy(one, other)
    back.join(5)


class Origin(BaseHTTPRequestHandler):
    seen = []

    def log_message(self, *arguments):
        pass

    def do_GET(self):
        Origin.seen.append((self.requestline, {k.lower(): v for k, v in self.headers.items()}))
        body = f"hello {self.path}".encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class HttpProxy(BaseHTTPRequestHandler):
    seen = []
    expected = None

    def log_message(self, *arguments):
        pass

    def refused(self):
        HttpProxy.seen.append((self.requestline, {k.lower(): v for k, v in self.headers.items()}))
        if self.headers.get("Proxy-Authorization") == HttpProxy.expected:
            return False
        self.send_response(407)
        self.send_header("Proxy-Authenticate", 'Basic realm="corp"')
        self.send_header("Content-Length", "0")
        self.end_headers()
        return True

    def do_CONNECT(self):
        if self.refused():
            return
        host, port = self.path.rsplit(":", 1)
        remote = socket.create_connection(("127.0.0.1", int(port)))
        self.send_response(200)
        self.end_headers()
        self.close_connection = True
        tunnel(self.connection, remote)
        remote.close()

    def do_GET(self):
        if self.refused():
            return
        body = b"from the proxy"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class Socks:
    """A SOCKS5 proxy that takes one user and password, and reaches every name on 127.0.0.1."""

    def __init__(self, username, password):
        self.username, self.password = username, password
        self.names = []
        self.server = socket.create_server(("127.0.0.1", 0))
        self.port = self.server.getsockname()[1]
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while True:
            try:
                client, _ = self.server.accept()
            except OSError:
                return
            threading.Thread(target=self.serve, args=(client,), daemon=True).start()

    def serve(self, client):
        def exactly(count):
            data = b""
            while len(data) < count:
                more = client.recv(count - len(data))
                if not more:
                    raise OSError("closed")
                data += more
            return data

        try:
            _, count = exactly(2)
            methods = exactly(count)
            if 2 not in methods:
                client.sendall(b"\x05\xff")
                return
            client.sendall(b"\x05\x02")
            _, length = exactly(2)
            username = exactly(length).decode()
            password = exactly(exactly(1)[0]).decode()
            if (username, password) != (self.username, self.password):
                client.sendall(b"\x01\x01")
                return
            client.sendall(b"\x01\x00")
            _, command, _, kind = exactly(4)
            name = exactly(exactly(1)[0]).decode() if kind == 3 else None
            port = struct.unpack(">H", exactly(2))[0]
            self.names.append(name)
            remote = socket.create_connection(("127.0.0.1", port))
            client.sendall(b"\x05\x00\x00\x01" + bytes(4) + b"\x00\x00")
            tunnel(client, remote)
            remote.close()
        except OSError:
            pass
        finally:
            client.close()

    def close(self):
        self.server.close()


def serve(handler):
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def credentials(username, password):
    return "Basic " + base64.b64encode(f"{username}:{password}".encode()).decode()


class OutboundProxyTest(unittest.TestCase):
    def setUp(self):
        Origin.seen, HttpProxy.seen = [], []
        HttpProxy.expected = credentials("ana", "p:ss word")
        self.origin = serve(Origin)
        self.upstream = serve(HttpProxy)
        self.directory = tempfile.mkdtemp()
        self.loop = asyncio.new_event_loop()
        self.thread = threading.Thread(target=self.loop.run_forever, daemon=True)
        self.thread.start()
        started = asyncio.run_coroutine_threadsafe(outbound.start(self.directory, 0), self.loop)
        self.proxy, self.server, self.controls = started.result(5)
        self.port = self.server.sockets[0].getsockname()[1]

    def tearDown(self):
        async def close():
            self.server.close()
            self.controls.close()
            for task in asyncio.all_tasks():
                if task is not asyncio.current_task():
                    task.cancel()

        asyncio.run_coroutine_threadsafe(close(), self.loop).result(5)
        self.loop.call_soon_threadsafe(self.loop.stop)
        self.thread.join(5)
        self.loop.close()
        for server in (self.origin, self.upstream):
            server.shutdown()
            server.server_close()

    def control(self, request):
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(5)
            connection.connect(os.path.join(self.directory, "control"))
            connection.sendall((json.dumps(request) + "\n").encode())
            reply = b""
            while not reply.endswith(b"\n"):
                reply += connection.recv(65536)
            return json.loads(reply)

    def apply(self, upstream, bypass=()):
        return self.control({"command": "apply",
                             "settings": {"version": "v1", "upstream": upstream, "bypass": list(bypass)}})

    def http_upstream(self, password="p:ss word"):
        return {"scheme": "http", "host": "127.0.0.1", "port": self.upstream.server_address[1],
                "username": "ana", "password": password}

    def exchange(self, *messages):
        """Send each message once the answer to the one before has begun, and read to the end."""
        with socket.create_connection(("127.0.0.1", self.port), timeout=5) as client:
            answer = b""
            for message in messages:
                client.sendall(message)
                if message is not messages[-1]:
                    answer += client.recv(65536)
            while True:
                data = client.recv(65536)
                if not data:
                    return answer
                answer += data

    def origin_url(self, path="/"):
        return f"http://127.0.0.1:{self.origin.server_address[1]}{path}"

    def test_nothing_leaves_before_the_manager_gives_the_settings(self):
        self.assertEqual(self.control({"command": "status"}), {"state": "running", "version": None})
        answer = self.exchange(f"GET {self.origin_url()} HTTP/1.1\r\nHost: x\r\n\r\n".encode())
        self.assertTrue(answer.startswith(b"HTTP/1.1 503 "), answer)
        self.assertEqual(Origin.seen, [])

    def test_the_settings_are_kept_in_memory_and_the_version_reported(self):
        self.assertEqual(self.apply(None), {"state": "running", "version": "v1"})
        self.assertEqual(self.control({"command": "status"})["version"], "v1")
        self.assertEqual(os.listdir(self.directory), ["control"])
        # A second proxy in the same folder leaves the running one alone.
        again = asyncio.run_coroutine_threadsafe(outbound.start(self.directory, 0), self.loop)
        self.assertIsNone(again.result(5))

    def test_without_a_proxy_a_request_goes_to_its_address_in_origin_form(self):
        self.apply(None)
        answer = self.exchange((f"GET {self.origin_url('/a?b=1')} HTTP/1.1\r\nHost: origin\r\n"
                                "Proxy-Authorization: Basic c2VjcmV0\r\nConnection: keep-alive\r\n\r\n").encode())
        self.assertIn(b"hello /a?b=1", answer)
        line, headers = Origin.seen[0]
        self.assertEqual(line, "GET /a?b=1 HTTP/1.1")
        self.assertNotIn("proxy-authorization", headers)
        self.assertEqual(headers["connection"], "close")
        self.assertEqual(headers["host"], "origin")

    def test_connect_goes_through_the_http_proxy_with_its_credentials(self):
        self.apply(self.http_upstream())
        target = f"origin.test:{self.origin.server_address[1]}"
        answer = self.exchange(f"CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n".encode(),
                               b"GET /tunnel HTTP/1.0\r\n\r\n")
        self.assertTrue(answer.startswith(b"HTTP/1.1 200 "), answer)
        self.assertIn(b"hello /tunnel", answer)
        line, headers = HttpProxy.seen[0]
        self.assertEqual(line, f"CONNECT {target} HTTP/1.1")
        self.assertEqual(headers["proxy-authorization"], credentials("ana", "p:ss word"))

    def test_plain_http_goes_to_the_http_proxy_in_full_with_its_credentials_only(self):
        self.apply(self.http_upstream())
        answer = self.exchange(("GET http://example.test/page HTTP/1.1\r\nHost: example.test\r\n"
                                "Proxy-Authorization: Basic b3duOnZhbHVl\r\n\r\n").encode())
        self.assertIn(b"from the proxy", answer)
        line, headers = HttpProxy.seen[0]
        self.assertEqual(line, "GET http://example.test/page HTTP/1.1")
        self.assertEqual(headers["proxy-authorization"], credentials("ana", "p:ss word"))
        self.assertEqual(headers["connection"], "close")

    def test_a_refusal_reaches_the_program_without_a_challenge(self):
        self.apply(self.http_upstream(password="wrong"))
        target = f"origin.test:{self.origin.server_address[1]}"
        answer = self.exchange(f"CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n".encode())
        self.assertTrue(answer.startswith(b"HTTP/1.1 407 "), answer)
        self.assertNotIn(b"Proxy-Authenticate", answer)
        self.assertEqual(Origin.seen, [])

    def test_socks5_signs_in_and_leaves_the_name_to_the_proxy(self):
        socks = Socks("ana", "secret")
        self.addCleanup(socks.close)
        self.apply({"scheme": "socks5", "host": "127.0.0.1", "port": socks.port,
                    "username": "ana", "password": "secret"})
        target = f"origin.test:{self.origin.server_address[1]}"
        answer = self.exchange(f"CONNECT {target} HTTP/1.1\r\n\r\n".encode(),
                               b"GET /socks HTTP/1.0\r\n\r\n")
        self.assertIn(b"hello /socks", answer)
        self.assertEqual(socks.names, ["origin.test"])
        # Plain HTTP goes the same way, in origin form.
        answer = self.exchange(f"GET http://origin.test:{self.origin.server_address[1]}/p HTTP/1.1\r\n\r\n".encode())
        self.assertIn(b"hello /p", answer)
        self.assertEqual(Origin.seen[-1][0], "GET /p HTTP/1.1")

    def test_loopback_goes_straight_to_its_address_whatever_the_proxy(self):
        closed = socket.create_server(("127.0.0.1", 0))
        dead = closed.getsockname()[1]
        closed.close()
        self.apply({"scheme": "http", "host": "127.0.0.1", "port": dead, "username": None, "password": None})
        target = f"127.0.0.1:{self.origin.server_address[1]}"
        answer = self.exchange(f"CONNECT {target} HTTP/1.1\r\n\r\n".encode(), b"GET /direct HTTP/1.0\r\n\r\n")
        self.assertIn(b"hello /direct", answer)
        # Another host has to go through the proxy, which is not there.
        answer = self.exchange(b"CONNECT example.test:443 HTTP/1.1\r\n\r\n")
        self.assertTrue(answer.startswith(b"HTTP/1.1 502 "), answer)

    def test_bypass_entries_match_names_domains_and_networks(self):
        bypass = ["intranet.corp", ".internal.example", "*.dev.example", "10.0.0.0/8", "fd00::/8"]
        for host in ["intranet.corp", "wiki.intranet.corp", "a.internal.example", "internal.example",
                     "x.dev.example", "10.1.2.3", "fd00::1", "localhost", "127.0.0.5", "::1"]:
            self.assertTrue(outbound.bypassed(host, bypass), host)
        for host in ["corp", "notintranet.corp", "example", "11.0.0.1", "fe80::1", "github.com"]:
            self.assertFalse(outbound.bypassed(host, bypass), host)
        self.assertTrue(outbound.bypassed("anything.test", ["*"]))


if __name__ == "__main__":
    unittest.main()
