#!/usr/bin/env python3
"""A stand-in for a local AI model (Ollama's address and answers), to try
Sanaga's intelligence features — summary, page translation, extension
writing — without a model: the progress, the bars and the buttons can be
seen and tested with answers that are always the same.

    tools/fake-model.py [seconds each answer takes, default 1.5]

Then, in Sanaga: Settings › Intelligence › Ollama. (Stop the real Ollama
first: both listen on 127.0.0.1:11434.) See docs/UX-AUDIT-2026-11-11.md.
"""
import json, sys, time
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
        return "Cette page présente son sujet en quelques paragraphes.\n• Premier point essentiel de la page.\n• Deuxième point, avec un chiffre : 600 km.\n• Troisième point pour finir."
    if system.startswith("You write small"):
        return "document.addEventListener('DOMContentLoaded', function () {\n  var s = document.createElement('style');\n  s.textContent = '.ad, .banner { display: none !important; }';\n  document.head.appendChild(s);\n});"
    return "ok"


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
        time.sleep(DELAY)
        self.reply({"model": asked.get("model", ""), "response": answer(asked.get("system", ""), asked.get("prompt", "")), "done": True})

    def log_message(self, *args):
        pass


HTTPServer(("127.0.0.1", 11434), Handler).serve_forever()
