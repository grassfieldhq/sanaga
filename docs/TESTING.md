# Testing Sanaga

Unit tests run with `cargo test` and need no display; they include the
repeatable fuzzing of the file and page readers ([FUZZING.md](FUZZING.md)).
The security rules (download names and marks, page-message checks,
`.desktop` escaping, the single-instance socket's limits, private folders;
`src/guard.rs`), the crash reports (`src/crash.rs`) and the codec module also
build on their own, without GTK and WebKitGTK:

```sh
cargo test --manifest-path tools/guard-tests/Cargo.toml
```

To check the real window, Sanaga reads a few switches (environment variables).
They are only meant for development, do nothing unless set, and exist in
**debug builds only** (`cargo build`, not `--release`).

## Keep your own data out of it

```sh
XDG_DATA_HOME=/tmp/sanaga-test ./target/debug/sanaga https://example.com
```

Sanaga then keeps everything (settings, history, cookies, frames…) in
`/tmp/sanaga-test/sanaga/`. You can also use a separate profile:
`--profile Test`.

## Scripted actions: `SANAGA_LIVE_TEST`

A comma-separated list of `seconds:action`, run in order after start:

```sh
SANAGA_SHOT_DIR=/tmp/shots \
SANAGA_LIVE_TEST="5:devices,6:devpick:22,9:shot,10:quit" \
  ./target/debug/sanaga https://en.wikipedia.org/wiki/Sanaga_River
```

| Action | Effect |
|---|---|
| `shot` | save a picture of Sanaga's window to `$SANAGA_SHOT_DIR/live-N.png` (default: the temp folder) |
| `quit` | close the window |
| `click:x:y`, `rclick:x:y`, `move:x:y` | mouse click / right click / move at (x, y), in window points |
| `rclick1:x:y` | right click with press and release in the same frame |
| `rpress:x:y`, `rrelease:x:y` | right button press / release |
| `gtkclick:x:y`, `gtkrclick:x:y` | click delivered through GTK itself |
| `type:text`, `key:Name` | typed text / a key (`Enter`, `ArrowDown`, `Escape`…) |
| `menu` | open or close the main menu |
| `swipe:dir:progress[:go]` | show the touchpad-swipe arrow (`-1` back, `1` forward), `go` navigates |
| `devices`, `devpick:N`, `devrotate`, `devcaptureN`, `devrecord` | mobile simulator: open, pick device N, rotate, screenshot (0 device, 1 screen, 2 page), start/stop video |
| `inspector-detach`, `inspector-attach` | move the web inspector to its own window / back |
| `dbgalloc`, `info` | print where the window's parts are / its scale and size |
| any shortcut name | e.g. `new`, `sidebar`, `inspector`, `source`, `next`, `addr` |

The synthetic clicks and keys go straight to Sanaga: they never reach other
applications, whatever window has the focus.

## The window in one command

```sh
tools/smoke.sh            # the debug build (test hooks are not in release builds)
```

It starts the real window on a temporary profile, waits for the interface,
loads `tests/smoke.html` and asks the page its title through Sanaga's own
channel. CI runs it under a virtual display (job `window`), and a failure
there stops the merge.

## Driven by test software

```sh
tools/webdriver-test.py   # needs WebKitWebDriver (docs/AUTOMATION.md)
```

The real window again, driven through WebDriver: a session, a page, a
script, an element, a screenshot, a second tab. Also part of CI's `window` job.

## After a crash

```sh
XDG_DATA_HOME=/tmp/sanaga-test ./target/debug/sanaga https://example.com &
sleep 5; kill -9 $!                                   # an abrupt end
XDG_DATA_HOME=/tmp/sanaga-test ./target/debug/sanaga  # the bar, and sanaga://crashes
```

`kill -TERM` (or logging out) is not a crash: Sanaga saves its tabs and
closes as with Quit.

## The intelligence features without a model

```sh
tools/fake-model.py &     # answers at Ollama's address, always the same
```

Then Settings › Intelligence › Ollama: the assistant's conversations, the
summary, the translation (its bar, Stop, Show original) and the writing of
an extension can be tried and captured. See
[UX-AUDIT-2026-11-11.md](UX-AUDIT-2026-11-11.md).

In a conversation the stand-in says how many texts and pictures it was
given, so a test can read it. A question that holds `tool:NAME {json}`
makes it ask for that tool (`tool:NAME @/a/file.json` reads the input from
a file), then tell what the tool answered. `FAKE_MODEL_LOG=/a/file` writes
every request it receives there: what Sanaga really sent.

```sh
FAKE_MODEL_LOG=/tmp/sent.log tools/fake-model.py &
XDG_DATA_HOME=/tmp/sanaga-test SANAGA_LIVE_TEST='6:ask,7:type:tool:list_tabs,8:key:Enter,12:shot,13:quit' \
  ./target/debug/sanaga file://$PWD/tests/assistant.html     # (settings.txt: ai=ollama)
```

The same program stands in for **Claude's Messages API** on port 11435, for
a debug build started with `SANAGA_CLAUDE_URL=http://127.0.0.1:11435/v1/messages`
and `ai=claude` (no key is asked for, none is sent). It answers with a
thinking block, words and tool calls as events, and refuses what the real
service refuses of a conversation's history: a tool result without its
call, a thinking block sent back after what came before it was changed.
It is a stand-in: what Sanaga sends to the real service was written from
its documentation and has not been tried against it.

More scripted actions for the assistant: `ask`, `summary`, `attach:/a/file`
(a file attached without the file window), `toolyes` / `toolno` (the
person's answer to a tool the model asks for), `mcpadd:Name|command or address`
(an outside tools' server added without the form). `tests/assistant.html`
has a frame and a web component whose text must reach the model;
`tests/form.html` has a password, card fields (marked by `autocomplete`,
or only by their name, label or placeholder), fields hidden in several
ways (a hidden input, invisible, transparent, placed outside the page, of
no size) and a field in a frame: none of them may be listed nor filled.
It says whether the form was sent.

## Outside tools without a server

```sh
tools/fake-mcp.py                       # as a program (what Sanaga starts)
tools/fake-mcp.py --http 11436          # at http://127.0.0.1:11436/mcp
tools/fake-mcp.py --http 11436 --events # answers as a stream of events
```

Three tools: `echo`, `add_note`, `fail`. `cargo test` runs the whole
exchange with it as a program (`mcp::tests::a_program_is_greeted_listed_and_called`).

## The administrator's policy

`SANAGA_POLICY=/a/file` (debug builds) is read in place of
`/etc/sanaga/policy.txt`: `ai=off`, or `ai_no_sites=example.org`.

## Looking for lag

```sh
tools/lag-audit.sh        # a heavy profile, slow helper programs, every feature
```

See [STABILITY-AUDIT-2026-11-11.md](STABILITY-AUDIT-2026-11-11.md). It reads
the lines of `SANAGA_DEBUG_STALLS` (below), which also works in a release
build, on your own profile: `SANAGA_DEBUG_STALLS=30 sanaga`.

More scripted actions for it: `go:ADDRESS` (in this tab), `opentab:ADDRESS`,
`section:N` (the Nth Settings category), `syncnow`, and `panic` (a panic
inside a frame: Sanaga must go on).

## Without a mouse, without the screen

```sh
SANAGA_DEBUG_A11Y=1 SANAGA_LIVE_TEST="5:key:Tab,6:key:Tab,7:shot,8:quit" ./target/debug/sanaga
python3 tools/contrast.py     # the tab bar's texts against what is behind them (4.5:1 at least)
```

With a screen reader — or only the accessibility bus — running, what the
reader is really given can be read through AT-SPI, and Orca can be run
without a sound card (its log holds what it would say):

```sh
tools/a11y-tree.py --all                 # needs python3-gi, gir1.2-atspi-2.0
orca --debug-file=/tmp/orca.log          # then: grep 'SPEECH OUTPUT' /tmp/orca.log
```

Sanaga's interface and the page are two trees named `sanaga`: the bars and
menus ("Sanaga — bars and menus", published by Sanaga), and the window with
the page's document (published by GTK and the web engine).

`SANAGA_DEBUG_A11Y=1` (debug builds) prints what a screen reader is told,
each time it changes: one line per named control — `a11y Tab "Wikipédia"
selected`, `a11y MenuItem "History"`, `a11y Status "Address copied."
live=Polite` — with `FOCUS` on the one the keyboard is on (and `(no name)`
if it has none: a defect). Each `key:Tab` moves the keyboard to the next
control; the picture must show a ring around it.

## Other switches

| Variable | Effect |
|---|---|
| `SANAGA_DEBUG_FRAMES=1` | print each frame's duration |
| `SANAGA_DEBUG_A11Y=1` | print what screen readers are told, and where the keyboard is (see above); debug builds |
| `SANAGA_DEBUG_STALLS=30` | print every time the window did not answer for 30 ms or more (`stall … ms after <last command> (layout … ms, painting … ms)`), and each frame whose own work took 4 ms or more (`frame … ms after …`); all builds |
| `SANAGA_DEBUG_STALL_STACKS=200` | with it: where the window's thread is during a stall of 200 ms or more |
| `SANAGA_DEBUG_TICK=1` | print the web engine ticks |
| `SANAGA_DEBUG_SUGGEST=1` | print the address bar's list of suggestions each time it is put together (when the text or the history's answer changes — never at every frame) |
| `SANAGA_DEBUG_GL=1` | print the OpenGL version and errors |
| `SANAGA_DEBUG_START=1` | print how long each step of the start took |
| `SANAGA_DEBUG_BLOCKER=1` | print the block lists' fetch and compilation |
| `SANAGA_DEBUG_FRAME=1` | print where the simulator's frame and browser buttons are |
| `SANAGA_HWACCEL=never\|always` | force WebKit's hardware acceleration policy |
| `SANAGA_BENCH_CYCLE=n` | show the next tab every 3 s, n times (memory tests) |
| `SANAGA_SHOT_STATE=dark,menu,sidebar,…` | states shown by `--screenshot` |
| `SANAGA_MOCK_CAPTURE=1` | a camera, a microphone and a screen made up by the engine: the questions and the capture sign can be tried on a machine without any |
| `SANAGA_TEST_POPUPS=1` | let pages open windows without a click (to test sign-in windows) |
| `SANAGA_DANGER_LIST=/a/file` | the list of sites reported as dangerous, one name per line, instead of the lists fetched (to see the warning without going near a real one) |
| `SANAGA_DEBUG_DNS=1` | encrypted DNS: prints each name that could not be resolved, and why |
| `SANAGA_UPDATE_URL=http://…` | the address "Look for a new version" asks instead of GitHub (a file holding `{"tag_name": "0.9.0"}`) |
| `SANAGA_NO_SANDBOX=1` | run pages without the bubblewrap sandbox |
