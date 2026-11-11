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

Then Settings › Intelligence › Ollama: the summary, the translation (its
bar, Stop, Show original) and the writing of an extension can be tried and
captured. See [UX-AUDIT-2026-11-11.md](UX-AUDIT-2026-11-11.md).

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

## Other switches

| Variable | Effect |
|---|---|
| `SANAGA_DEBUG_FRAMES=1` | print each frame's duration |
| `SANAGA_DEBUG_STALLS=30` | print every time the window did not answer for 30 ms or more (`stall … ms after <last command> (layout … ms, painting … ms)`), and each frame whose own work took 4 ms or more (`frame … ms after …`); all builds |
| `SANAGA_DEBUG_STALL_STACKS=200` | with it: where the window's thread is during a stall of 200 ms or more |
| `SANAGA_DEBUG_TICK=1` | print the web engine ticks |
| `SANAGA_DEBUG_GL=1` | print the OpenGL version and errors |
| `SANAGA_DEBUG_START=1` | print how long each step of the start took |
| `SANAGA_DEBUG_BLOCKER=1` | print the block lists' fetch and compilation |
| `SANAGA_DEBUG_FRAME=1` | print where the simulator's frame and browser buttons are |
| `SANAGA_HWACCEL=never\|always` | force WebKit's hardware acceleration policy |
| `SANAGA_BENCH_CYCLE=n` | show the next tab every 3 s, n times (memory tests) |
| `SANAGA_SHOT_STATE=dark,menu,sidebar,…` | states shown by `--screenshot` |
| `SANAGA_TEST_POPUPS=1` | let pages open windows without a click (to test sign-in windows) |
| `SANAGA_NO_SANDBOX=1` | run pages without the bubblewrap sandbox |
