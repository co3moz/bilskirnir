"""Relay stdio MCP to the manager's authenticated HTTP endpoint; stdout is protocol only."""
from concurrent.futures import ThreadPoolExecutor
import json
import os
import sys
import threading
import urllib.error
import urllib.request

PROTOCOL = "2025-11-25"
REQUEST_LIMIT = 1_100_000
RESPONSE_LIMIT = 8 * 1024 * 1024
# The session's stream carries a keep-alive every 15 seconds, so this long without one means it
# is lost.
STREAM_IDLE = 60
# Until the server notices a lost stream, at its next keep-alive, a new one would only be kept
# alive beside it and hear nothing, so the stream is opened again after this wait.
REOPEN_AFTER = 20


def events(response, whole):
    """The data of each server-sent event. With whole, the stream stays within RESPONSE_LIMIT as a
    whole, as the answer to one request; otherwise each event does, since a session's stream
    stays open."""
    pending = []
    received = 0
    while line := response.readline(RESPONSE_LIMIT + 1):
        received += len(line)
        if received > RESPONSE_LIMIT:
            raise ValueError("MCP event stream exceeds the 8 MiB limit")
        line = line.rstrip(b"\r\n")
        if not line:
            if pending:
                yield b"\n".join(pending)
                pending = []
            if not whole:
                received = 0
        elif line.startswith(b"data:"):
            data = line[5:].lstrip(b" ")
            if data:
                pending.append(data)
    if pending:
        yield b"\n".join(pending)


def run(url, token, source, output, errors):
    output_lock = threading.Lock()
    slots = threading.Semaphore(8)
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    # A scoped endpoint (/mcp/<name>) assigns a session on initialize and expects it back on
    # every later request; /mcp assigns none.
    session = {"id": None}
    stream = {"opened": False}
    stream_lock = threading.Lock()
    stopped = threading.Event()

    def emit(message):
        with output_lock:
            output.write(json.dumps(message, ensure_ascii=True, separators=(",", ":")) + "\n")
            output.flush()

    def failure(message, code, text):
        if not isinstance(message, dict) or "id" in message:
            emit({"jsonrpc": "2.0", "id": message.get("id") if isinstance(message, dict) else None,
                  "error": {"code": code, "message": text}})
        else:
            with output_lock:
                errors.write(text + "\n")
                errors.flush()

    def listen():
        # What the endpoint sends outside any request, such as notifications/tools/list_changed
        # when a desktop server is added, arrives on the session's own stream. Opened again
        # without Last-Event-ID, the server repeats what it kept of the stream, so a notification
        # sent meanwhile still arrives, possibly twice.
        while not stopped.is_set():
            headers = {
                "Accept": "text/event-stream",
                "MCP-Protocol-Version": PROTOCOL,
                "Mcp-Session-Id": session["id"],
            }
            if token:
                headers["Authorization"] = "Bearer " + token
            try:
                with client.open(urllib.request.Request(url, headers=headers), timeout=STREAM_IDLE) as response:
                    if response.headers.get_content_type() != "text/event-stream":
                        return
                    for data in events(response, whole=False):
                        try:
                            message = json.loads(data)
                        except ValueError:
                            continue
                        if isinstance(message, dict) and message.get("jsonrpc") == "2.0":
                            emit(message)
            except urllib.error.HTTPError as error:
                # 404 once the session ended, 405 where no stream is offered: nothing comes later.
                if 400 <= error.code < 500:
                    return
            except Exception:
                pass
            stopped.wait(REOPEN_AFTER)

    def open_stream():
        with stream_lock:
            if not session["id"] or stream["opened"]:
                return
            stream["opened"] = True
        threading.Thread(target=listen, daemon=True).start()

    def forward(message):
        try:
            params = message.get("params")
            if message.get("method") == "initialize" and isinstance(params, dict):
                params["protocolVersion"] = PROTOCOL
            if isinstance(params, dict) and isinstance(params.get("_meta"), dict):
                meta = params["_meta"]
                if "io.modelcontextprotocol/protocolVersion" in meta:
                    meta["io.modelcontextprotocol/protocolVersion"] = PROTOCOL
            body = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            if len(body) > REQUEST_LIMIT:
                failure(message, -32600, "MCP request exceeds the 1100000-byte limit")
                return
            headers = {
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
                "MCP-Protocol-Version": PROTOCOL,
            }
            if token:
                headers["Authorization"] = "Bearer " + token
            if session["id"]:
                headers["Mcp-Session-Id"] = session["id"]
            request = urllib.request.Request(url, data=body, headers=headers)
            with client.open(request, timeout=125) as response:
                assigned = response.headers.get("Mcp-Session-Id")
                if assigned:
                    session["id"] = assigned
                if response.status in (202, 204):
                    # The session takes its own stream once it is initialized.
                    if message.get("method") == "notifications/initialized":
                        open_stream()
                    return
                mime = response.headers.get_content_type()
                if mime == "application/json":
                    data = response.read(RESPONSE_LIMIT + 1)
                    if len(data) > RESPONSE_LIMIT:
                        raise ValueError("MCP response exceeds the 8 MiB limit")
                    emit(json.loads(data))
                elif mime == "text/event-stream":
                    for data in events(response, whole=True):
                        emit(json.loads(data))
                else:
                    raise ValueError("MCP endpoint returned an unexpected content type")
        except urllib.error.HTTPError as error:
            failure(message, -32000, f"MCP HTTP request failed with status {error.code}")
        except Exception:
            # Never put headers, credentials, script contents or the request URL in output.
            failure(message, -32000, "MCP request failed; check that the manager and MCP endpoint are available")
        finally:
            slots.release()

    with ThreadPoolExecutor(max_workers=8) as pool:
        while line := source.readline(REQUEST_LIMIT + 1):
            if len(line) > REQUEST_LIMIT:
                if not line.endswith(b"\n"):
                    while chunk := source.readline(REQUEST_LIMIT + 1):
                        if chunk.endswith(b"\n"):
                            break
                failure(None, -32600, "MCP request exceeds the 1100000-byte limit")
                continue
            if not line.strip():
                continue
            try:
                message = json.loads(line)
            except (ValueError, UnicodeDecodeError):
                failure(None, -32700, "Invalid JSON")
                continue
            if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
                failure(None, -32600, "Expected one JSON-RPC 2.0 message per line")
                continue
            slots.acquire()
            # The answer to initialize carries the session everything after it must name.
            if message.get("method") == "initialize":
                forward(message)
            else:
                pool.submit(forward, message)
    stopped.set()


if __name__ == "__main__":
    # A manager with MCP_AUTH_MODE=none takes no token, so the bridge sends none either.
    tokenless = os.environ.get("MCP_AUTH_MODE") == "none"
    token = "" if tokenless else os.environ.get("MCP_TOKEN", "")
    if not token and not tokenless:
        sys.stderr.write("Enable MCP and configure MCP_TOKEN on the manager before connecting.\n")
        raise SystemExit(1)
    run(os.environ.get("MCP_BRIDGE_URL", "http://127.0.0.1:8080/mcp"), token,
        sys.stdin.buffer, sys.stdout, sys.stderr)
