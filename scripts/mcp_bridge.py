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


def run(url, token, source, output, errors):
    output_lock = threading.Lock()
    slots = threading.Semaphore(8)
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))

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
            request = urllib.request.Request(url, data=body, headers={
                "Authorization": "Bearer " + token,
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
                "MCP-Protocol-Version": PROTOCOL,
            })
            with client.open(request, timeout=125) as response:
                if response.status in (202, 204):
                    return
                mime = response.headers.get_content_type()
                if mime == "application/json":
                    data = response.read(RESPONSE_LIMIT + 1)
                    if len(data) > RESPONSE_LIMIT:
                        raise ValueError("MCP response exceeds the 8 MiB limit")
                    emit(json.loads(data))
                elif mime == "text/event-stream":
                    pending = []
                    received = 0
                    while line := response.readline(RESPONSE_LIMIT + 1):
                        received += len(line)
                        if received > RESPONSE_LIMIT:
                            raise ValueError("MCP event stream exceeds the 8 MiB limit")
                        line = line.rstrip(b"\r\n")
                        if not line:
                            if pending:
                                emit(json.loads(b"\n".join(pending)))
                                pending = []
                        elif line.startswith(b"data:"):
                            data = line[5:].lstrip(b" ")
                            if data:
                                pending.append(data)
                    if pending:
                        emit(json.loads(b"\n".join(pending)))
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
            pool.submit(forward, message)


if __name__ == "__main__":
    token = os.environ.get("MCP_TOKEN", "")
    if not token:
        sys.stderr.write("Enable MCP and configure MCP_TOKEN on the manager before connecting.\n")
        raise SystemExit(1)
    run(os.environ.get("MCP_BRIDGE_URL", "http://127.0.0.1:8080/mcp"), token,
        sys.stdin.buffer, sys.stdout, sys.stderr)
