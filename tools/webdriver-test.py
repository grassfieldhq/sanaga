#!/usr/bin/env python3
"""Drives the real Sanaga through WebDriver (WebKit's own driver), as test
software would: a session, a page, its title, a script, an element, a
screenshot. Used by CI under a virtual display; works on a desktop too.

    tools/webdriver-test.py [path/to/sanaga]

Needs WebKitWebDriver (Debian/Ubuntu: webkit2gtk-driver, Arch: in
webkitgtk-6.0, Fedora: webkit2gtk-driver). See docs/AUTOMATION.md.
"""
import base64, json, os, shutil, socket, subprocess, sys, tempfile, time, urllib.error, urllib.request

here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
binary = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "target/debug/sanaga"))
driver = shutil.which("WebKitWebDriver")
if not driver:
    sys.exit("WebKitWebDriver is not installed (see docs/AUTOMATION.md)")

with socket.socket() as s:  # a free port
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
base = f"http://127.0.0.1:{port}"
data = tempfile.mkdtemp(prefix="sanaga-webdriver-")
env = dict(os.environ, XDG_DATA_HOME=data)
proc = subprocess.Popen([driver, f"--port={port}", "--host=127.0.0.1"], env=env)


def call(method, path, body=None):
    req = urllib.request.Request(base + path, method=method, data=None if body is None else json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.load(r)["value"]
    except urllib.error.HTTPError as e:
        raise SystemExit(f"{method} {path}: {e.code} {e.read().decode(errors='replace')}")


failures = []


def check(name, ok, got=None):
    print(f"{name}: {'ok' if ok else 'FAILED'}" + ("" if ok else f" (got {got!r})"))
    if not ok:
        failures.append(name)


session = None
try:
    for _ in range(100):  # the driver listens
        try:
            call("GET", "/status")
            break
        except (urllib.error.URLError, ConnectionError):
            time.sleep(0.1)
    caps = {"capabilities": {"alwaysMatch": {"webkitgtk:browserOptions": {"binary": binary, "args": ["--automation"]}}}}
    value = call("POST", "/session", caps)
    session = value["sessionId"]
    s = f"/session/{session}"
    check("session", value["capabilities"].get("browserName") == "Sanaga", value["capabilities"])
    call("POST", s + "/url", {"url": "file://" + os.path.join(here, "tests/smoke.html")})
    check("title", call("GET", s + "/title") == "smoke 2", call("GET", s + "/title"))
    check("address", call("GET", s + "/url").endswith("/tests/smoke.html"))
    check("script", call("POST", s + "/execute/sync", {"script": "return document.querySelector('h1').textContent + arguments[0];", "args": [7]}) == "Sanaga smoke test7")
    element = call("POST", s + "/element", {"using": "css selector", "value": "h1"})
    check("element", call("GET", f"{s}/element/{list(element.values())[0]}/text") == "Sanaga smoke test")
    png = base64.b64decode(call("GET", s + "/screenshot"))
    check("screenshot", png[:8] == b"\x89PNG\r\n\x1a\n" and len(png) > 500, len(png))
    # a second page, as a tab of the same window
    call("POST", s + "/window/new", {"type": "tab"})
    check("tabs", len(call("GET", s + "/window/handles")) == 2, call("GET", s + "/window/handles"))
    # nothing of the test may land in a profile
    kept = [f for _, _, files in os.walk(data) for f in files if f in ("history.tsv", "session.tsv", "cookies")]
    check("nothing kept", not kept, kept)
finally:
    if session:
        try:
            call("DELETE", f"/session/{session}")
        except SystemExit as e:
            print(e)
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
    shutil.rmtree(data, ignore_errors=True)
sys.exit(1 if failures else 0)
