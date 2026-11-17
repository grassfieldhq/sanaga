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
(when Sanaga offered it), then tell what the tool answered.
"""
import json, os, sys, time
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
            try:
                args = json.loads(rest) if rest.strip() else {}
            except ValueError:
                args = {}
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


HTTPServer(("127.0.0.1", 11434), Handler).serve_forever()
