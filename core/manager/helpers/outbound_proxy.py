"""The desktop's own HTTP proxy on 127.0.0.1, which the manager starts as nobody for a container
that uses a proxy. Programs reach it through HTTP_PROXY and HTTPS_PROXY, which name no
credentials; it goes on to the proxy the container's settings choose (http, https or socks5) with
the user name and password that only its memory holds, or straight to the address for a bypassed
host. Until the manager has applied the settings over the control socket it answers 503, so
nothing leaves the desktop past the proxy.

    python3 outbound_proxy.py DIRECTORY PORT
"""
import asyncio
import base64
import ipaddress
import json
import os
import socket
import ssl
import struct
import sys
from urllib.parse import urlsplit

HEAD_LIMIT = 65536
HEAD_TIMEOUT = 30
CONNECT_TIMEOUT = 30
MAX_CONNECTIONS = 512
CHUNK = 65536
# Headers that belong to one hop, which the next one must not see.
HOP_HEADERS = {"proxy-authorization", "proxy-connection", "connection", "keep-alive"}


class Refused(Exception):
    """The way on is closed: an HTTP status and a line for the program that asked."""

    def __init__(self, status, reason, detail):
        super().__init__(detail)
        self.status, self.reason, self.detail = status, reason, detail


def bypassed(host, bypass):
    """Whether a host goes straight to its address: a name in the list or below one of its
    domains (`example.com`, `.example.com` and `*.example.com` alike), an address in one of its
    networks, `*` for every host, and always the desktop's own loopback."""
    host = host.lower().rstrip(".")
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        address = None
    if host == "localhost" or (address is not None and address.is_loopback):
        return True
    for entry in bypass:
        entry = entry.strip().lower().rstrip(".")
        if entry == "*":
            return True
        if address is not None:
            try:
                if address in ipaddress.ip_network(entry, strict=False):
                    return True
            except ValueError:
                pass
            continue
        domain = entry.lstrip("*").lstrip(".")
        if domain and (host == domain or host.endswith("." + domain)):
            return True
    return False


def basic(upstream):
    """The Proxy-Authorization header line for an http(s) proxy, or nothing without a user."""
    if not upstream.get("username"):
        return b""
    token = f"{upstream['username']}:{upstream.get('password') or ''}".encode()
    return b"Proxy-Authorization: Basic " + base64.b64encode(token) + b"\r\n"


def target(host, port):
    return f"[{host}]:{port}" if ":" in host else f"{host}:{port}"


async def read_head(reader):
    try:
        head = await asyncio.wait_for(reader.readuntil(b"\r\n\r\n"), HEAD_TIMEOUT)
    except asyncio.LimitOverrunError:
        raise Refused(431, "Request Header Fields Too Large", "The request head is too large.")
    except (asyncio.IncompleteReadError, asyncio.TimeoutError):
        return None
    if len(head) > HEAD_LIMIT:
        raise Refused(431, "Request Header Fields Too Large", "The request head is too large.")
    return head


def split_head(head):
    lines = head[:-4].split(b"\r\n")
    return lines[0], lines[1:]


async def open_direct(host, port):
    return await asyncio.wait_for(asyncio.open_connection(host, port), CONNECT_TIMEOUT)


async def open_proxy(upstream):
    context = None
    if upstream["scheme"] == "https":
        context = ssl.create_default_context()
    return await asyncio.wait_for(
        asyncio.open_connection(upstream["host"], upstream["port"], ssl=context,
                                server_hostname=upstream["host"] if context else None),
        CONNECT_TIMEOUT)


async def connect_http(upstream, host, port):
    reader, writer = await open_proxy(upstream)
    request = (f"CONNECT {target(host, port)} HTTP/1.1\r\nHost: {target(host, port)}\r\n".encode()
               + basic(upstream) + b"\r\n")
    writer.write(request)
    await writer.drain()
    head = await read_head(reader)
    if head is None:
        writer.close()
        raise Refused(502, "Bad Gateway", "The proxy closed the connection.")
    status_line = split_head(head)[0].split(b" ", 2)
    status = int(status_line[1]) if len(status_line) > 1 and status_line[1].isdigit() else 502
    if not 200 <= status < 300:
        writer.close()
        reason = status_line[2].decode("latin-1") if len(status_line) > 2 else "Bad Gateway"
        raise Refused(status, reason, f"The proxy refused {target(host, port)}: {status} {reason}.")
    return reader, writer


async def connect_socks(upstream, host, port):
    reader, writer = await open_proxy(dict(upstream, scheme="socks5"))
    username = (upstream.get("username") or "").encode()
    password = (upstream.get("password") or "").encode()
    writer.write(b"\x05\x02\x00\x02" if username else b"\x05\x01\x00")
    await writer.drain()
    version, method = await reader.readexactly(2)
    if version != 5 or method not in (0, 2) or (method == 2 and not username):
        writer.close()
        raise Refused(502, "Bad Gateway", "The SOCKS proxy accepts none of the ways to sign in.")
    if method == 2:
        writer.write(bytes([1, len(username)]) + username + bytes([len(password)]) + password)
        await writer.drain()
        if (await reader.readexactly(2))[1] != 0:
            writer.close()
            raise Refused(407, "Proxy Authentication Required",
                          "The SOCKS proxy refused the user name or password.")
    try:
        address = ipaddress.ip_address(host)
        kind = b"\x01" if address.version == 4 else b"\x04"
        destination = kind + address.packed
    except ValueError:
        name = host.encode("idna")
        destination = b"\x03" + bytes([len(name)]) + name
    writer.write(b"\x05\x01\x00" + destination + struct.pack(">H", port))
    await writer.drain()
    reply = await reader.readexactly(4)
    if reply[1] != 0:
        writer.close()
        raise Refused(502, "Bad Gateway", f"The SOCKS proxy refused {target(host, port)} ({reply[1]}).")
    kind = reply[3]
    length = 4 if kind == 1 else 16 if kind == 4 else (await reader.readexactly(1))[0]
    await reader.readexactly(length + 2)
    return reader, writer


class Proxy:
    def __init__(self):
        # Nothing until the manager applies the container's settings.
        self.settings = None
        self.connections = 0

    def apply(self, settings):
        self.settings = settings

    def status(self):
        version = self.settings.get("version") if self.settings else None
        return {"state": "running", "version": version}

    async def route(self, host, port):
        """A connection to host:port for bytes either way, through the proxy unless bypassed."""
        upstream = self.settings.get("upstream")
        try:
            if upstream is None or bypassed(host, self.settings.get("bypass") or []):
                return await open_direct(host, port)
            if upstream["scheme"] == "socks5":
                return await connect_socks(upstream, host, port)
            return await connect_http(upstream, host, port)
        except Refused:
            raise
        except (OSError, asyncio.TimeoutError, asyncio.IncompleteReadError, ssl.SSLError) as error:
            raise Refused(502, "Bad Gateway", f"Cannot reach {target(host, port)}: {error}.")

    async def serve(self, reader, writer):
        if self.connections >= MAX_CONNECTIONS:
            writer.close()
            return
        self.connections += 1
        try:
            await self.exchange(reader, writer)
        except Refused as refusal:
            await answer(writer, refusal.status, refusal.reason, refusal.detail)
        except (OSError, asyncio.IncompleteReadError, ValueError):
            pass
        finally:
            self.connections -= 1
            writer.close()

    async def exchange(self, reader, writer):
        head = await read_head(reader)
        if head is None:
            return
        if self.settings is None:
            raise Refused(503, "Service Unavailable",
                          "Thoric has not given this desktop its proxy settings yet.")
        request, headers = split_head(head)
        parts = request.split(b" ")
        if len(parts) != 3:
            raise Refused(400, "Bad Request", "The request line is malformed.")
        method, location, version = parts
        if method == b"CONNECT":
            host, _, port = location.decode("latin-1").rpartition(":")
            host = host.strip("[]")
            if not host or not port.isdigit():
                raise Refused(400, "Bad Request", "CONNECT needs a host and a port.")
            remote_reader, remote_writer = await self.route(host, int(port))
            writer.write(b"HTTP/1.1 200 Connection established\r\n\r\n")
            await writer.drain()
            await relay(reader, writer, remote_reader, remote_writer)
            return
        url = urlsplit(location.decode("latin-1"))
        if url.scheme != "http" or not url.hostname:
            raise Refused(400, "Bad Request", "Send http:// addresses in full, and use CONNECT for others.")
        host, port = url.hostname, url.port or 80
        kept = [line for line in headers
                if line.split(b":", 1)[0].strip().lower().decode("latin-1") not in HOP_HEADERS]
        upstream = self.settings.get("upstream")
        through_http = (upstream is not None and upstream["scheme"] != "socks5"
                        and not bypassed(host, self.settings.get("bypass") or []))
        if through_http:
            # An http(s) proxy takes the address in full, with the credentials beside it.
            try:
                remote_reader, remote_writer = await open_proxy(upstream)
            except (OSError, asyncio.TimeoutError, ssl.SSLError) as error:
                raise Refused(502, "Bad Gateway", f"Cannot reach the proxy: {error}.")
            line = method + b" " + location + b" " + version
            extra = basic(upstream)
        else:
            remote_reader, remote_writer = await self.route(host, port)
            path = url.path or "/"
            if url.query:
                path += "?" + url.query
            line = method + b" " + path.encode("latin-1") + b" " + version
            extra = b""
        # One request a connection: the next one starts over with a head this proxy reads.
        remote_writer.write(line + b"\r\n" + b"".join(h + b"\r\n" for h in kept) + extra
                            + b"Connection: close\r\n\r\n")
        await remote_writer.drain()
        await relay(reader, writer, remote_reader, remote_writer)


async def answer(writer, status, reason, detail):
    body = (detail + "\n").encode()
    try:
        writer.write(f"HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\n"
                     f"Content-Length: {len(body)}\r\nConnection: close\r\n\r\n".encode() + body)
        await writer.drain()
    except OSError:
        pass


async def pipe(reader, writer):
    try:
        while True:
            data = await reader.read(CHUNK)
            if not data:
                break
            writer.write(data)
            await writer.drain()
    except (OSError, asyncio.IncompleteReadError):
        pass
    finally:
        try:
            if writer.can_write_eof():
                writer.write_eof()
        except OSError:
            pass


async def relay(client_reader, client_writer, remote_reader, remote_writer):
    """Carry bytes both ways until the far side is done, then close both."""
    upward = asyncio.ensure_future(pipe(client_reader, remote_writer))
    downward = asyncio.ensure_future(pipe(remote_reader, client_writer))
    await asyncio.wait([downward])
    upward.cancel()
    remote_writer.close()


async def control(proxy, reader, writer):
    """The manager's requests, one JSON line each: `status`, or `apply` with the settings."""
    try:
        request = json.loads(await asyncio.wait_for(reader.readline(), 10))
        if request.get("command") == "apply":
            proxy.apply(request["settings"])
        reply = proxy.status()
    except (ValueError, KeyError, TypeError, asyncio.TimeoutError) as error:
        reply = {"state": "error", "error": str(error)}
    writer.write((json.dumps(reply) + "\n").encode())
    await writer.drain()
    writer.close()


def running(path):
    probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        probe.connect(path)
        return True
    except OSError:
        return False
    finally:
        probe.close()


async def start(directory, port, host="127.0.0.1"):
    """Listen for programs on host:port and for the manager on DIRECTORY/control."""
    proxy = Proxy()
    path = os.path.join(directory, "control")
    if running(path):
        return None
    if os.path.exists(path):
        os.unlink(path)
    server = await asyncio.start_server(proxy.serve, host, port, limit=HEAD_LIMIT)
    controls = await asyncio.start_unix_server(lambda r, w: control(proxy, r, w), path)
    os.chmod(path, 0o600)
    return proxy, server, controls


def main(directory, port):
    error_file = os.path.join(directory, "error")
    # A failure of an earlier start says nothing about this one.
    if os.path.exists(error_file):
        os.unlink(error_file)

    async def run():
        try:
            started = await start(directory, port)
        except OSError as error:
            with open(error_file, "w") as handle:
                handle.write(f"Cannot listen on 127.0.0.1:{port}: {error}")
            return
        if started is None:
            return
        _, server, controls = started
        async with server, controls:
            await asyncio.gather(server.serve_forever(), controls.serve_forever())

    null = os.open(os.devnull, os.O_RDWR)
    for descriptor in (0, 1, 2):
        os.dup2(null, descriptor)
    os.close(null)
    asyncio.run(run())


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]))
