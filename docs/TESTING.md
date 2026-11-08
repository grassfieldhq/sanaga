# Testing Sanaga by hand

Unit tests run with `cargo test` and need no display. To check the real window,
Sanaga reads a few switches (environment variables). They are only meant for
development and do nothing unless set.

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

## Other switches

| Variable | Effect |
|---|---|
| `SANAGA_DEBUG_FRAMES=1` | print each frame's duration |
| `SANAGA_DEBUG_TICK=1` | print the web engine ticks |
| `SANAGA_DEBUG_GL=1` | print the OpenGL version and errors |
| `SANAGA_DEBUG_FRAME=1` | print where the simulator's frame and browser buttons are |
| `SANAGA_HWACCEL=never\|always` | force WebKit's hardware acceleration policy |
| `SANAGA_BENCH_CYCLE=n` | show the next tab every 3 s, n times (memory tests) |
| `SANAGA_SHOT_STATE=dark,menu,sidebar,…` | states shown by `--screenshot` |
