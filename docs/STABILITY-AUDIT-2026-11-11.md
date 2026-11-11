# Stability audit — 11 November 2026

**The question:** can a site make Sanaga crash, and can browsing or using a
feature of Sanaga ever lag?

**The honest answer first.** No audit can promise "never": pages run in
WebKit, which Sanaga does not write, and smoothness also depends on the
graphics card and the disk. What an audit can do is (1) look for everything
in *Sanaga's own code* that can crash it or freeze its window, (2) measure
it on the real window under hard conditions, (3) fix what is found, and
(4) leave tests that fail if it comes back. That is what was done. What
remains outside Sanaga's reach is listed at the end, plainly.

## Method

| Tool | What it shows |
|---|---|
| `SANAGA_DEBUG_STALLS=30` (new, `src/shell.rs`) | Every time the window stopped answering for 30 ms or more, whatever held it: a thread asks the window's loop for a sign of life and measures how late it comes. Each line also gives what Sanaga's own code took for the frame (layout) and for painting it. |
| `SANAGA_DEBUG_STALL_STACKS=200` (new) | Where the window's thread is at that moment (its call stack): tells a wait in Sanaga's code from the drawing by the system. |
| `tools/lag-audit.sh` (new) | The real window on a heavy profile — 5 000 visits, 400 favourites, 200 reading-list entries, 200 saved accounts, 80 tabs in groups, a 4 000-paragraph page — with the system's helper programs made slow (the keyring, `xdg-settings`, `xdg-mime`, `gtk-update-icon-cache`… each take 2 s, as on a busy or half-configured system, or a keyring waiting to be unlocked). It walks through 80 actions: menus, sidebar, overview, tab search, every internal page, every Settings category, Find, Reader, zoom, source view, split view, simulator, default browser, application icon, sync, tabs opened and closed. |
| Mutation tests (`src/mutate.rs`, `src/app/robust.rs`) | Every function that cuts or reads text coming from a page, a file, a model or the keyboard, fed thousands of damaged inputs. |
| Reading the code | Every program Sanaga runs, every `sleep`, every network call, every file write: is it ever waited for in the window's thread? Every `unwrap`, `expect`, slice and index: can it panic? |

Measured on Ubuntu 24.04, WebKitGTK 2.52.6, a virtual display with software
rendering (no graphics card), 4 processor cores.

## Results

| | Before | After |
|---|---|---|
| Longest frame of Sanaga's own code | **4 357 ms** (opening Settings) | **63–79 ms** (the very first frame at start) |
| Longest time the window did not answer | **24 s** (Settings, slow keyring) | **146–155 ms** (drawing, by the processor) |
| Each Settings category, Passwords page | 2 000 ms per frame | under 20 ms |
| Changing tab (saving the session) | 100–470 ms on this disk | not waited for |
| A panic inside a frame | Sanaga and every tab gone | the frame is dropped, a report kept, Sanaga goes on |
| `unwrap` / `expect` outside tests | 0 | 0 |

## What could crash Sanaga

| # | Found | State |
|---|---|---|
| C1 | **Find in page (Light engine, Reader)** panicked on a page holding letters whose small form takes a different number of bytes than the capital (İ, ẞ, the Kelvin sign): the place found in the lowercased text was used to cut the original (`split_find`, `src/app/render.rs`). A page could bring Sanaga down as soon as the person searched in it. | Fixed: found places are mapped back to whole letters of the original. Test: `find_in_page_cuts_text_on_characters`. |
| C2 | **Address-bar completion** cut the site's name by the *bytes* typed: a typed letter longer than the name's (the Kelvin sign for "k") could cut past its end. | Fixed (`src/app/toolbar.rs`): counted in letters. |
| C3 | **Importing Chrome bookmarks**: a file holding `[}` ran for ever; a broken surrogate pair overflowed (found by fuzzing, previous change). | Fixed, tested. |
| C4 | **Any panic while making a frame** ended Sanaga at once — every window and tab — because a panic cannot cross GTK. Almost all of Sanaga's logic runs inside the frame (pages' events included). | Fixed (`src/shell.rs`, `after_error`): the panic is caught, the frame dropped, the interface's state closed properly, what was open over the page is closed, the person is told and a report is kept (`sanaga://crashes`). The second time in a row the tab shown is left for a new one; the third time Sanaga stops and the next start brings the tabs back. Checked on the real window with a forced panic. |
| C5 | Text read from pages, files, models and the keyboard by code that had no damaged-input test: the source view (`src/app/source.rs`), the calculator, search shortcuts, the address shown, model answers (`src/ai.rs`, translation), website settings, user-script headers, the system's look file. | 100 000 damaged inputs: nothing else found. The tests stay (`src/app/robust.rs`, `damaged_answers_do_not_panic`). |

A page's own crash (its WebKit process) was already contained: the tab shows
"This page stopped working", the other tabs go on, a report is kept.

## What could make Sanaga lag

All of these held **the window's thread**: while they lasted, nothing in the
window answered — no click, no scroll, no typing.

| # | Found | State |
|---|---|---|
| L1 | **Settings and the Passwords page started `secret-tool` at every frame** (to know whether the keyring is there), and Settings asked `xdg-settings` whether Sanaga is the default browser, waiting for the answer. With a slow keyring: 2 s per frame, 24 s to open Settings. On a fast system: a program started sixty times a second. | Fixed: asked once, on a thread (`keyring_ok`, `first_time`). |
| L2 | **Every keyring request was waited for**: when a login is sent (is it already saved?), when filling, showing, copying, deleting a password, saving or forgetting the Claude key or the sync phrase. A locked keyring shows its own unlock window: Sanaga stayed frozen until the person answered it. | Fixed: on threads (`off_thread`). Filling checks again, once the keyring has answered, that the tab still shows the site the password was saved for. |
| L3 | **The translate menu read the Claude key from the keyring at every frame** it was shown (`ai_provider`). | Fixed: only "is there a key" is kept; the key is read where the request is made, on its thread. |
| L4 | **Every save waited for the disk** (`fsync`) in the window's thread: at every tab shown, page visited, setting, favourite, zoom. 100–470 ms each on this machine's disk; a few ms on a fast SSD, far more on a slow or busy one. | Fixed (`src/store.rs`, "saving without waiting"): the window hands the file to a thread that writes in the order asked, still safely (copy, then put in place); saves of the same file asked in a row come down to the last; reading back and quitting wait for it. |
| L5 | **The sync round ran in the window's thread every 30 s**: the shared folder (a network drive, a USB key waking up…) and, the first time, the key made from the secret phrase (Argon2, deliberately slow). | Fixed: on a thread, one round at a time. A favourite changed *during* a round is joined with the round's result (`sync::rejoin`, three-way), not lost. |
| L6 | **"Make default browser" and the application icon** waited for `xdg-settings`, `xdg-mime`, five pictures and `gtk-update-icon-cache`. | Fixed: on threads. |
| L7 | **The new tab page counted the whole history again at every frame** ("frequently visited"): 5–90 ms per frame with 5 000 visits. | Fixed: counted again only when a visit or a favourite changes. |
| L8 | **Site names were worked out again for every row at every frame** (tabs, sidebar, lists, badges): 20 ms per frame with the sidebar open on 600 entries. | Fixed: remembered (`host_of`). |
| L9 | **Taking a block list holds WebKit — and the window — about 150 ms** (40 000 rules; measured: 149, 168, 72 ms). After the weekly refresh the three lists were handed over together: one hold of about 360 ms. | Reduced: one list at a time, each once the one before is compiled — three short holds seconds apart, once a week. The hold itself is inside WebKit. |

Checked and fine: the network, the AI models, the codecs check, the video
recorder, imports, the block-list fetch and the Hyprland helpers already ran
on threads; nothing else in the interface runs a program and waits.

## What is not covered, and why

- **The web engine.** A site can still crash or hang *its own page*: that is
  WebKit. Sanaga keeps it to that tab, says so, and keeps a report. Engine
  fixes come with WebKitGTK updates.
- **Smoothness of drawing on real hardware.** This machine has no graphics
  card. The 100–150 ms stalls left in the audit are the processor drawing
  the window (the call stacks show GTK's renderer and Mesa's software
  rasteriser, and they shrink with the window: 1 200 ms at 1280×800, 146 ms
  at 640×480). They say nothing about a computer with a graphics card. To
  measure yours: `SANAGA_DEBUG_STALLS=30 sanaga`, use it for a while, and
  read the lines.
- **The first frame at start** takes 60–110 ms here with 80 tabs (fonts,
  first layout). It was not reduced.
- **A panic outside a frame** (in one of the small functions GTK or WebKit
  call directly: a signal of a page, a timer) still ends Sanaga. None was
  found that can panic; they are short and mostly forward to the frame.
- **Lists are drawn whole** (history, favourites, sidebar): fine at today's
  limits (5 000 visits: 25–35 ms for the first frame, under 4 ms after), to
  be drawn by window if the limits are raised.
- **Three rare actions still write a small file in the window's thread**:
  a profile's photo, a new profile's first settings, an extension written
  by a model.
- **GitHub's runners.** The checks below were run here, not yet there.

## So that it stays that way

- `tools/lag-audit.sh` runs in CI (job `window`) and fails when a frame of
  Sanaga's own code takes 500 ms, or the window stops answering for 1 900 ms
  — every helper program takes 2 s in that run, so *one* of them waited for
  in the window's thread, anywhere, fails the build.
- The mutation tests run with `cargo test` at every push.
- The rule is in CONTRIBUTING.md: nothing that may wait runs in the window's
  thread; use `off_thread`.

## Running it

```sh
cargo build
xvfb-run -a -s "-screen 0 640x480x24" env LIBGL_ALWAYS_SOFTWARE=1 \
    dbus-run-session -- tools/lag-audit.sh          # or on your desktop, without xvfb-run
LAG_AUDIT_STACKS=200 tools/lag-audit.sh              # with call stacks (slower)
SANAGA_DEBUG_STALLS=30 ./target/debug/sanaga         # your own use, your own profile
```
