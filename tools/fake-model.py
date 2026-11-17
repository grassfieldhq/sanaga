#!/usr/bin/env python3
"""A stand-in for a local AI model (Ollama's address and answers), to try
Sanaga's intelligence features — the assistant's conversations and its
tools, summary, page translation, extension writing — without a model: the
progress, the bars and the buttons can be seen and tested with answers
that are always the same.

    tools/fake-model.py [seconds each answer takes, default 1.5]

Then, in Sanaga: Settings › Intelligence › Ollama. (Stop the real Ollama
first: both listen on 127.0.0.1:11434.) See docs/UX-AUDIT-2026-11-11.md.

In a conversation (`/api/chat`) it says how many attached texts it was
given and repeats the question, so that a test can read both. A question
that holds "tool:NAME {json}" makes it ask for that tool with that input
(when Sanaga offered it), then tell what the tool answered; "tool:NAME
@/a/file.json" reads the input from that file.

It also stands in for Claude's Messages API, on 127.0.0.1:11435, for a
debug build started with SANAGA_CLAUDE_URL=http://127.0.0.1:11435/v1/messages
(docs/TESTING.md). There it answers with a thinking block, words and tool
calls as events, and refuses what the real service refuses of a
conversation's history: a tool result without its call, and a thinking
block sent back after what came before it was changed.
"""
import hashlib, json, os, sys, threading, time
from http.server import BaseHTTPRequestHandler, HTTPServer

DELAY = float(sys.argv[1]) if len(sys.argv) > 1 else 1.5


def answer(system, prompt):
    if system.startswith("You translate"):
        # each text of the page, marked as translated
        try:
            texts = json.loads(prompt)
        except ValueError:
            texts = []
        return json.dumps(["«" + t.upper() + "»" for t in texts], ensure_ascii=False)
    if system.startswith("You summarise"):
        return "## En bref\nCette page présente **son sujet** en quelques paragraphes.\n• Premier point essentiel de la page.\n• Deuxième point, avec un chiffre : **600 km**.\n• Troisième point pour finir, un peu plus long pour passer à la ligne dans le panneau."
    if system.startswith("You write small"):
        return "document.addEventListener('DOMContentLoaded', function () {\n  var s = document.createElement('style');\n  s.textContent = '.ad, .banner { display: none !important; }';\n  document.head.appendChild(s);\n});"
    return "ok"


def chat_answer(asked):
    """What the stand-in says in a conversation: (words, tool calls)."""
    messages = asked.get("messages", [])
    offered = [t["function"]["name"] for t in asked.get("tools", [])]
    last = messages[-1] if messages else {}
    if last.get("role") == "tool":
        return "L'outil %s a répondu : %s" % (last.get("tool_name", "?"), last.get("content", "")[:300]), []
    users = [m for m in messages if m.get("role") == "user"]
    question = users[-1]["content"] if users else ""
    # the words after the attached texts are the person's
    own = question.rsplit(">\n\n", 1)[-1]
    if "tool:" in own:
        name, _, rest = own.split("tool:", 1)[1].strip().partition(" ")
        if name in offered:
            args = tool_input(rest)
            return "", [{"function": {"name": name, "arguments": args}}]
        return "Cet outil ne m'a pas été proposé : %s" % name, []
    if own.startswith("Summarise this page"):
        return answer("You summarise", ""), []
    texts = sum(m["content"].count("<page_content ") + m["content"].count("<selection ") + m["content"].count("<file ") for m in users)
    pictures = sum(len(m.get("images", [])) for m in users)
    return "Question n° %d : « %s ». Textes joints : %d. Images : %d. Outils proposés : %d." % (len(users), own[:200], texts, pictures, len(offered)), []


class Handler(BaseHTTPRequestHandler):
    def reply(self, value):
        body = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self.reply({"models": [{"name": "test-model"}]})

    def do_POST(self):
        asked = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        # (FAKE_MODEL_LOG=/a/file: every request received, one per line —
        # a test reads there what Sanaga really sent)
        if os.environ.get("FAKE_MODEL_LOG"):
            with open(os.environ["FAKE_MODEL_LOG"], "a", encoding="utf-8") as f:
                f.write(json.dumps({"path": self.path, "body": asked}, ensure_ascii=False) + "\n")
        if self.path.startswith("/api/chat"):
            return self.chat(asked)
        text = answer(asked.get("system", ""), asked.get("prompt", ""))
        if not asked.get("stream"):
            time.sleep(DELAY)
            self.reply({"model": asked.get("model", ""), "response": text, "done": True})
            return
        # as Ollama does: one JSON object per line, a few words at a time
        self.send_response(200)
        self.send_header("Content-Type", "application/x-ndjson")
        self.end_headers()
        words = text.split(" ")
        try:
            for k, word in enumerate(words):
                time.sleep(DELAY / max(len(words), 1))
                piece = word + ("" if k == len(words) - 1 else " ")
                self.wfile.write((json.dumps({"response": piece, "done": False}) + "\n").encode())
                self.wfile.flush()
            self.wfile.write((json.dumps({"response": "", "done": True}) + "\n").encode())
        except (BrokenPipeError, ConnectionResetError):
            pass  # the request was stopped in Sanaga

    def chat(self, asked):
        # as Ollama's /api/chat does: one JSON object per line
        text, calls = chat_answer(asked)
        self.send_response(200)
        self.send_header("Content-Type", "application/x-ndjson")
        self.end_headers()
        words = text.split(" ") if text else []
        try:
            for k, word in enumerate(words):
                time.sleep(DELAY / max(len(words), 1))
                piece = word + ("" if k == len(words) - 1 else " ")
                self.wfile.write((json.dumps({"message": {"role": "assistant", "content": piece}, "done": False}) + "\n").encode())
                self.wfile.flush()
            if calls:
                self.wfile.write((json.dumps({"message": {"role": "assistant", "content": "", "tool_calls": calls}, "done": False}) + "\n").encode())
            self.wfile.write((json.dumps({"message": {"role": "assistant", "content": ""}, "done": True}) + "\n").encode())
        except (BrokenPipeError, ConnectionResetError):
            pass  # the request was stopped in Sanaga

    def log_message(self, *args):
        pass


def tool_input(rest):
    """The input of a "tool:NAME …" question: JSON, or @file holding it."""
    rest = rest.strip()
    try:
        if rest.startswith("@"):
            with open(rest[1:], encoding="utf-8") as f:
                return json.load(f)
        return json.loads(rest) if rest else {}
    except (ValueError, OSError):
        return {}


# ---------------------------------------------------------------- Claude


def signature(system, tools, before):
    """What ties a thinking block to the conversation before it."""
    seen = json.dumps([system, sorted(t["name"] for t in tools), before], sort_keys=True, ensure_ascii=False)
    return "sig-" + hashlib.sha256(seen.encode()).hexdigest()[:24]


def claude_check(asked):
    """What the service would refuse of this request (None: nothing)."""
    messages = asked.get("messages", [])
    if not messages or messages[0].get("role") != "user":
        return "messages: first message must use the \"user\" role"
    calls = set()
    for k, m in enumerate(messages):
        content = m.get("content")
        blocks = content if isinstance(content, list) else [{"type": "text", "text": content}]
        if not blocks:
            return "messages.%d: content must not be empty" % k
        if m.get("role") == "assistant":
            for i, b in enumerate(blocks):
                if b.get("type") == "thinking" and b.get("signature") != signature(asked.get("system", ""), asked.get("tools", []), messages[:k]):
                    return "messages.%d.content.%d: Invalid `signature` in `thinking` block. The block is bound to a different conversation." % (k, i)
            calls = {b["id"] for b in blocks if b.get("type") == "tool_use"}
        else:
            results = {b.get("tool_use_id") for b in blocks if b.get("type") == "tool_result"}
            if results != calls:
                return "messages.%d: `tool_use` ids were found without `tool_result` blocks immediately after (or the other way round)" % k
            calls = set()
    if calls:
        return "messages: the last `tool_use` has no `tool_result`"
    return None


def claude_answer(asked):
    """(words, tool calls) for Claude's stand-in, as chat_answer for Ollama's."""
    messages = asked.get("messages", [])
    offered = [t["name"] for t in asked.get("tools", [])]
    last = messages[-1]["content"]
    blocks = last if isinstance(last, list) else [{"type": "text", "text": last}]
    results = [b for b in blocks if b.get("type") == "tool_result"]
    if results:
        return "L'outil a répondu : %s" % str(results[0].get("content", ""))[:300], []
    own = [b["text"] for b in blocks if b.get("type") == "text"][-1]
    if "tool:" in own:
        name, _, rest = own.split("tool:", 1)[1].strip().partition(" ")
        if name in offered:
            args = tool_input(rest)
            return "Je regarde.", [{"name": name, "input": args}]
        return "Cet outil ne m'a pas été proposé : %s" % name, []
    if own.startswith("Summarise this page"):
        return answer("You summarise", ""), []
    users = [m for m in messages if m["role"] == "user" and isinstance(m["content"], list)]
    count = lambda kind: sum(1 for m in users for b in m["content"] if b.get("type") == kind)
    texts = sum(1 for m in users for b in m["content"] if b.get("type") == "text" and b["text"].startswith("<"))
    questions = sum(1 for m in users if not any(b.get("type") == "tool_result" for b in m["content"]))
    return "Question n° %d : « %s ». Textes joints : %d. Images : %d. PDF : %d. Outils proposés : %d." % (questions, own[:200], texts, count("image"), count("document"), len(offered)), []


class Claude(BaseHTTPRequestHandler):
    def do_POST(self):
        asked = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if os.environ.get("FAKE_MODEL_LOG"):
            with open(os.environ["FAKE_MODEL_LOG"], "a", encoding="utf-8") as f:
                f.write(json.dumps({"path": self.path, "body": asked}, ensure_ascii=False) + "\n")
        wrong = None if self.headers.get("x-api-key") and self.headers.get("anthropic-version") else "missing x-api-key or anthropic-version header"
        wrong = wrong or claude_check(asked)
        if wrong:
            body = json.dumps({"type": "error", "error": {"type": "invalid_request_error", "message": wrong}}).encode()
            self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        text, calls = claude_answer(asked)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()

        def event(value):
            self.wfile.write(("event: %s\ndata: %s\n\n" % (value["type"], json.dumps(value))).encode())
            self.wfile.flush()

        try:
            event({"type": "message_start", "message": {"id": "msg_test", "type": "message", "role": "assistant", "content": [], "model": asked.get("model", "")}})
            # a thinking block, shown empty, tied to what was sent
            event({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": "", "signature": ""}})
            event({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": signature(asked.get("system", ""), asked.get("tools", []), asked.get("messages", []))}})
            event({"type": "content_block_stop", "index": 0})
            event({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}})
            words = text.split(" ")
            for k, word in enumerate(words):
                time.sleep(DELAY / max(len(words), 1))
                event({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": word + ("" if k == len(words) - 1 else " ")}})
            event({"type": "content_block_stop", "index": 1})
            for k, call in enumerate(calls):
                event({"type": "content_block_start", "index": 2 + k, "content_block": {"type": "tool_use", "id": "toolu_%d_%d" % (len(asked.get("messages", [])), k), "name": call["name"], "input": {}}})
                whole = json.dumps(call["input"], ensure_ascii=False)
                half = len(whole) // 2
                for piece in (whole[:half], whole[half:]):
                    event({"type": "content_block_delta", "index": 2 + k, "delta": {"type": "input_json_delta", "partial_json": piece}})
                event({"type": "content_block_stop", "index": 2 + k})
            event({"type": "message_delta", "delta": {"stop_reason": "tool_use" if calls else "end_turn"}, "usage": {"output_tokens": 1}})
            event({"type": "message_stop"})
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, *args):
        pass


threading.Thread(target=lambda: HTTPServer(("127.0.0.1", 11435), Claude).serve_forever(), daemon=True).start()
HTTPServer(("127.0.0.1", 11434), Handler).serve_forever()
