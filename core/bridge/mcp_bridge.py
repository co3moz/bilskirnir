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
INITIALIZED = b'{"jsonrpc":"2.0","method":"notifications/initialized"}'
LIST_CHANGED = {"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}


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


def lists_tool_changes(answer):
    """Whether an answer to initialize says the server tells of changes to its tool list."""
    result = answer.get("result") if isinstance(answer, dict) else None
    capabilities = result.get("capabilities") if isinstance(result, dict) else None
    tools = capabilities.get("tools") if isinstance(capabilities, dict) else None
    return isinstance(tools, dict) and tools.get("listChanged") is True


def run(url, token, source, output, errors):
    output_lock = threading.Lock()
    slots = threading.Semaphore(8)
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    # A scoped endpoint (/mcp/<name>) assigns a session on initialize and expects it back on
    # every later request; /mcp assigns none. The client's initialize is kept to open the
    # session again once the endpoint has forgotten it.
    session = {"id": None, "initialize": None}
    # Held while a session is opened, by the client's initialize or by a renewal.
    session_lock = threading.Lock()
    stream = {"session": None}
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

    def headers(accept, session_id):
        named = {"Accept": accept, "MCP-Protocol-Version": PROTOCOL}
        if token:
            named["Authorization"] = "Bearer " + token
        if session_id:
            named["Mcp-Session-Id"] = session_id
        return named

    def listen(listening):
        # What the endpoint sends outside any request, such as notifications/tools/list_changed
        # when a desktop server is added, arrives on the session's own stream. Opened again
        # without Last-Event-ID, the server repeats what it kept of the stream, so a notification
        # sent meanwhile still arrives, possibly twice. A renewed session gets a listener of its
        # own, and this one stops.
        while not stopped.is_set() and session["id"] == listening:
            request = urllib.request.Request(url, headers=headers("text/event-stream", listening))
            try:
                with client.open(request, timeout=STREAM_IDLE) as response:
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
                # 404 once the session ended, 405 where no stream is offered: nothing comes later
                # on this session.
                if 400 <= error.code < 500:
                    return
            except Exception:
                pass
            stopped.wait(REOPEN_AFTER)

    def open_stream():
        with stream_lock:
            listening = session["id"]
            if not listening or stream["session"] == listening:
                return
            stream["session"] = listening
        threading.Thread(target=listen, args=(listening,), daemon=True).start()

    def post(body, session_id):
        named = headers("application/json, text/event-stream", session_id)
        named["Content-Type"] = "application/json"
        return client.open(urllib.request.Request(url, data=body, headers=named), timeout=125)

    def answers(response):
        """The messages that answer a POST, within RESPONSE_LIMIT."""
        if response.status in (202, 204):
            return
        mime = response.headers.get_content_type()
        if mime == "application/json":
            data = response.read(RESPONSE_LIMIT + 1)
            if len(data) > RESPONSE_LIMIT:
                raise ValueError("MCP response exceeds the 8 MiB limit")
            yield json.loads(data)
        elif mime == "text/event-stream":
            for data in events(response, whole=True):
                yield json.loads(data)
        else:
            raise ValueError("MCP endpoint returned an unexpected content type")

    def deliver(message, body, session_id):
        with post(body, session_id) as response:
            if message.get("method") == "initialize":
                session["id"] = response.headers.get("Mcp-Session-Id")
            for answer in answers(response):
                emit(answer)
        # The session takes its own stream once it is initialized.
        if message.get("method") == "notifications/initialized":
            open_stream()

    def renew(expired):
        """Open a new session in place of one the endpoint no longer knows, with the client's
        initialize, whose answer the client already has, and tell whether a request refused in
        the old one may be sent again."""
        with session_lock:
            if session["id"] != expired:
                # Another request renewed it meanwhile, or the client initialized again.
                return True
            if session["initialize"] is None:
                return False
            try:
                with post(session["initialize"], None) as response:
                    renewed = response.headers.get("Mcp-Session-Id")
                    answered = [answer for answer in answers(response)
                                if isinstance(answer, dict) and "id" in answer]
                if len(answered) != 1 or not isinstance(answered[0].get("result"), dict):
                    return False
                with post(INITIALIZED, renewed):
                    pass
            except Exception:
                return False
            session["id"] = renewed
        open_stream()
        # Nothing could tell of a change to the tools while there was no session, so the client
        # is asked to list them again.
        if lists_tool_changes(answered[0]):
            emit(LIST_CHANGED)
        return True

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
            if message.get("method") == "initialize":
                # initialize opens a session, so it names none.
                with session_lock:
                    session["initialize"] = body
                    deliver(message, body, None)
                return
            sent = session["id"]
            try:
                deliver(message, body, sent)
            except urllib.error.HTTPError as error:
                # An endpoint forgets a session that carried nothing for five minutes, and every
                # one when the manager restarts, and answers 404 to its ID. The request is sent
                # once more in a new session, and a second refusal goes to the client.
                if error.code != 404 or not sent or not renew(sent):
                    raise
                # A renewal sends notifications/initialized itself.
                if message.get("method") != "notifications/initialized":
                    deliver(message, body, session["id"])
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
