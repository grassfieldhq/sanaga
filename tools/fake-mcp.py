#!/usr/bin/env python3
"""A stand-in for an MCP server, to try the assistant's outside tools
(Settings › Intelligence › Outside tools, docs/TESTING.md).

    tools/fake-mcp.py                 # as a program: one JSON message per line
    tools/fake-mcp.py --http 11436    # at http://127.0.0.1:11436/mcp
    tools/fake-mcp.py --http 11436 --token secret   # refuses another token
    tools/fake-mcp.py --http 11436 --events         # answers as a stream of events

Three tools: `echo` (gives back its text), `add_note` (keeps a note for
this run and says how many it has) and `fail` (always fails).
"""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

NOTES = []
TOOLS = [
    {"name": "echo", "description": "Give back the text it is given.", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}},
    {"name": "add_note", "description": "Keep a note.", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}},
    {"name": "fail", "description": "Always fails.", "inputSchema": {"type": "object", "properties": {}}},
]


def answer(msg):
    """The answer to one message (None: it expects none)."""
    method, ident = msg.get("method"), msg.get("id")
    if ident is None:
        return None
    ok = lambda result: {"jsonrpc": "2.0", "id": ident, "result": result}
    if method == "initialize":
        return ok({"protocolVersion": msg["params"].get("protocolVersion", "2025-06-18"), "capabilities": {"tools": {}}, "serverInfo": {"name": "fake-mcp", "version": "1"}})
    if method == "tools/list":
        return ok({"tools": TOOLS})
    if method == "tools/call":
        name, args = msg["params"].get("name"), msg["params"].get("arguments") or {}
        if name == "echo":
            return ok({"content": [{"type": "text", "text": str(args.get("text", ""))}]})
        if name == "add_note":
            NOTES.append(str(args.get("text", "")))
            return ok({"content": [{"type": "text", "text": "Note kept (%d in all): %s" % (len(NOTES), NOTES[-1])}]})
        if name == "fail":
            return ok({"content": [{"type": "text", "text": "this tool always fails"}], "isError": True})
        return {"jsonrpc": "2.0", "id": ident, "error": {"code": -32602, "message": "Unknown tool: %s" % name}}
    return {"jsonrpc": "2.0", "id": ident, "error": {"code": -32601, "message": "Method not found"}}


def as_program():
    for line in sys.stdin:
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        reply = answer(msg)
        if reply is not None:
            sys.stdout.write(json.dumps(reply) + "\n")
            sys.stdout.flush()


class Handler(BaseHTTPRequestHandler):
    token = None
    events = False

    def do_POST(self):
        msg = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if self.token and self.headers.get("Authorization") != "Bearer " + self.token:
            self.send_response(401)
            self.end_headers()
            return
        reply = answer(msg)
        if reply is None:
            self.send_response(202)
            self.end_headers()
            return
        if self.events:
            body = ("event: message\ndata: %s\n\n" % json.dumps(reply)).encode()
            kind = "text/event-stream"
        else:
            body = json.dumps(reply).encode()
            kind = "application/json"
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Mcp-Session-Id", "session-1")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


if "--http" in sys.argv:
    port = int(sys.argv[sys.argv.index("--http") + 1])
    if "--token" in sys.argv:
        Handler.token = sys.argv[sys.argv.index("--token") + 1]
    Handler.events = "--events" in sys.argv
    HTTPServer(("127.0.0.1", port), Handler).serve_forever()
else:
    as_program()
