#!/usr/bin/env python3
"""A stand-in AI endpoint for the iPhone film footage (capture.sh).

    ai-stub.py PORT

Listens on 127.0.0.1:PORT and answers OpenAI-style POST /v1/chat/completions
with fixed, invented demo text, the way the app's stored demo notes and
study guides are fixed text: no model, no network, nothing leaves the Mac.
The film's tests point the app's one custom endpoint at it
(http://localhost:PORT/v1, set through launch arguments in
FilmFootageTests), so Chat and Follow-up email run through the app's real
code paths (Pro check, request, citations) and show this text.

- Chat (the app's chat system prompt): an answer about the demo deadlines
  that cites the passages the app sent, as [n], so the app shows its own
  citation chips. Facts whose passage isn't in the request are left out.
- Follow-up email: a draft for the "Brightwater pilot kickoff" demo meeting.
- Anything else: a short generic reply.

Only the demo data's invented words appear here. Nothing is logged but the
kind of request.
"""
import json
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CHAT_MARK = "You answer the user's questions about their own recordings"
EMAIL_MARK = "follow-up email"

EMAIL = """Subject: Brightwater pilot: next steps

Hi Dana and Jonah,

Thanks for walking us through how your inspections work today. Here's what we agreed:

- A pilot with five inspectors, starting October 14
- Visit summaries shared as PDF

Next steps:
- Alex: send the pilot agreement by Friday
- Jonah: share the three report templates by Wednesday
- Dana: choose the five pilot inspectors

We'll check in two weeks after launch.

Best,
Alex"""

# (sentence, words that find its passage). Sentences end with their [n].
DEADLINES = [
    ("**BIO 101:** problem set four is due next Thursday", ["problem set four"]),
    ("**Brightwater pilot:** Jonah sends the report templates by Wednesday", ["by wednesday"]),
    ("and the pilot agreement goes out by Friday", ["agreement by friday"]),
]


def passages(user_text):
    """[(n, header, text)] from the PASSAGES block the app sent."""
    block = user_text.split("PASSAGES:", 1)[-1].split("\n\nQUESTION:", 1)[0]
    out = []
    for chunk in re.split(r"\n\n(?=\[\d+\] )", block.strip()):
        m = re.match(r"\[(\d+)\] ([^\n]*)\n?(.*)", chunk, re.S)
        if m:
            out.append((int(m.group(1)), m.group(2), m.group(3)))
    return out


def find(ps, words):
    """The first transcript passage with any of `words`, else any passage."""
    hits = [(n, h) for n, h, t in ps if any(w in t.lower() for w in words)]
    for n, h in hits:
        if "· Transcript" in h:
            return n
    return hits[0][0] if hits else None


def chat_answer(user_text):
    ps = passages(user_text)
    parts = []
    for sentence, words in DEADLINES:
        n = find(ps, words)
        if n is not None:
            parts.append(f"{sentence} [{n}]")
    if not parts:
        return "The recordings in this scope don't mention a deadline."
    lines = ["Here's what's coming up:"]
    if len(parts) == 3:
        lines += [f"• {parts[0]}.", f"• {parts[1]}, {parts[2]}."]
    else:
        lines += [f"• {p}." for p in parts]
    return "\n".join(lines)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def reply(self, status, obj):
        body = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            return self.reply(200, {"object": "list", "data": [{"id": "film-demo", "object": "model"}]})
        self.reply(404, {"error": {"message": "not found"}})

    def do_POST(self):
        if not self.path.rstrip("/").endswith("/chat/completions"):
            return self.reply(404, {"error": {"message": "not found"}})
        length = int(self.headers.get("Content-Length") or 0)
        req = json.loads(self.rfile.read(length) or b"{}")
        msgs = req.get("messages") or []
        system = next((m.get("content") or "" for m in msgs if m.get("role") == "system"), "")
        user = next((m.get("content") or "" for m in reversed(msgs) if m.get("role") == "user"), "")
        if CHAT_MARK in system:
            kind, text, pause = "chat", chat_answer(user), 0.9
        elif EMAIL_MARK in system.lower():
            kind, text, pause = "email", EMAIL, 0.25
        else:
            kind, text, pause = "other", "Okay.", 0.2
        print(f"ai-stub: {kind}", flush=True)
        time.sleep(pause)   # long enough for the app's working state to show
        self.reply(200, {
            "id": "film-demo", "object": "chat.completion", "model": "film-demo",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        })


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"ai-stub: listening on 127.0.0.1:{port}", flush=True)
    server.serve_forever()
