# Driving Sanaga with test software (WebDriver)

Sanaga can be driven by test software — Selenium, WebdriverIO, or plain HTTP
requests — through [WebDriver](https://www.w3.org/TR/webdriver2/), the
standard browsers share. WebKitGTK brings the driver (`WebKitWebDriver`);
Sanaga lets it open and drive tabs when started with `--automation`.

```sh
sanaga --automation
```

A window started this way:

- **keeps nothing and shares nothing** with your profiles: no history, no
  cookies, no saved passwords, like a guest window (`--guest`);
- runs on its own, next to a Sanaga you may already have open;
- says so in a bar under the toolbar ("Sanaga is being controlled by
  automated test software");
- opens each page the test software asks for as a **tab** of that window.

Without `--automation`, nothing can drive Sanaga: the engine refuses.

## What you need

| System | Package with `WebKitWebDriver` |
|---|---|
| Debian, Ubuntu | `webkit2gtk-driver` |
| Fedora | `webkit2gtk-driver` |
| Arch | part of `webkitgtk-6.0` |

The driver and WebKitGTK must be the same version (they are, when both come
from your distribution).

## A complete example

Python's standard library only. It starts the driver, opens a session in
Sanaga, loads a page, reads its title, runs a script and saves a picture.

```python
import base64, json, subprocess, time, urllib.request

driver = subprocess.Popen(["WebKitWebDriver", "--port=4444", "--host=127.0.0.1"])
time.sleep(1)

def call(method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request("http://127.0.0.1:4444" + path, method=method, data=data,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as r:
        return json.load(r)["value"]

try:
    session = call("POST", "/session", {"capabilities": {"alwaysMatch": {
        "webkitgtk:browserOptions": {"binary": "/usr/bin/sanaga", "args": ["--automation"]}}}})
    s = "/session/" + session["sessionId"]
    call("POST", s + "/url", {"url": "https://example.com/"})
    print(call("GET", s + "/title"))
    print(call("POST", s + "/execute/sync", {"script": "return document.links.length;", "args": []}))
    open("page.png", "wb").write(base64.b64decode(call("GET", s + "/screenshot")))
    call("DELETE", s)
finally:
    driver.terminate()
```

With Selenium (Python):

```python
from selenium import webdriver
from selenium.webdriver.webkitgtk.options import Options

options = Options()
options.binary_location = "/usr/bin/sanaga"
options.add_argument("--automation")
driver = webdriver.WebKitGTK(options=options)
driver.get("https://example.com/")
print(driver.title)
driver.quit()
```

## Without a screen

Sanaga is a GTK 4 application: it needs a display to draw on, but not a
screen. On a server or in CI, give it a virtual one:

```sh
xvfb-run -a -s "-screen 0 1280x800x24" python3 my-test.py
```

`LIBGL_ALWAYS_SOFTWARE=1` draws without a graphics card. This is how
Sanaga's own CI runs (`.github/workflows/ci.yml`, job `window`).

## Sanaga's own WebDriver test

```sh
tools/webdriver-test.py            # the debug build
tools/webdriver-test.py /usr/bin/sanaga
```

It opens a session, loads `tests/smoke.html`, checks the title and the
address, runs a script, reads an element, takes a screenshot, opens a second
tab, and checks that nothing was written to a profile. CI runs it at every
push, after the window's smoke test.

## Limits

- One session at a time per Sanaga (WebKitGTK's rule).
- Sanaga's own pages (`sanaga://…`: settings, history…) are drawn by the
  interface, not by the engine: WebDriver sees web pages only. To script the
  interface itself, see the `SANAGA_LIVE_TEST` actions in
  [TESTING.md](TESTING.md) (debug builds).
- Protected-content sites (Netflix…) are still handed to another
  application, as in a normal window.
