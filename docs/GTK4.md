# Sanaga on GTK 4 and WebKitGTK 6

Status: **done** (November 2026). Sanaga's window is GTK 4 and its pages are
WebKitGTK 6; the GTK 3 / WebKitGTK 4.1 version is retired.

## What it brings

- **The page is composed by GTK 4 on the graphics card** (GSK). GTK 3 copied
  every page frame into the window.
- **Screen readers see the pages too**: GTK 4 publishes the page's content
  (headings, links, fields) next to Sanaga's own interface (AccessKit).
- WebKitGTK 6 is where WebKit's new features land (it always runs pages in
  their sandbox).
- Smooth fractional scaling comes with GTK 4.

### Measured (Omarchy / Hyprland, Intel UHD laptop, release build, 10 s)

| Page | GTK 3 / WebKitGTK 4.1 | GTK 4 / WebKitGTK 6 |
|---|---|---|
| Still page — Sanaga | ≈ 2 % | **0 %** |
| Page with an endless animation — Sanaga | 7–13 % | **6–10 %** |
| Same, with the page's own processes | 11–17 % | 11–16 % |

The interface itself only wakes when something changes (about once a second
when idle). Most of what remains on animated pages is WebKit drawing the page
60 times a second, as in any browser.

## Needed to build

| | Arch / Omarchy | Debian 13 / Ubuntu 24.04+ | Fedora |
|---|---|---|---|
| GTK 4 | `gtk4` | `libgtk-4-dev` | `gtk4-devel` |
| WebKitGTK 6 | `webkitgtk-6.0` | `libwebkitgtk-6.0-dev` | `webkitgtk6.0-devel` |

Rust crates: `gtk4` 0.11, `webkit6` 0.6 (WebKitGTK 2.48+), `cairo-rs` 0.22.

## How the window works

- `src/shell.rs`: one `gtk::Window` holding a `GtkOverlay`. Its main child is
  a `GtkGLArea` where egui draws the whole interface; each web page is an
  overlay child placed over the page area with margins; the simulator's device
  frame (a `GtkDrawingArea` that lets clicks through) and the swipe arrow stay
  above the pages.
- **The interface runs outside GTK's drawing**: GTK 4 does not allow adding or
  moving a page while it draws. Input, timers and other threads *wake* the
  window; the interface runs when GTK is idle (or with the screen's next
  refresh during animations, or within 200 ms if the window is not on screen),
  then GTK draws the result.
- Input comes from GTK 4 event controllers (keys with the input method, click,
  motion, scroll); the mouse's back / forward buttons and two-finger swipes
  are caught on the pages.
- Window moves and resizes use `gdk::Toplevel::begin_move / begin_resize`.
- `src/page.rs`: each profile has an *engine* — a `WebContext` and a
  `NetworkSession` (cookies, cache, downloads, tracking prevention). Private
  tabs share one ephemeral session. Page pictures are GDK textures.
- File dialogs use `gtk::FileDialog`.
- Test pictures (`SANAGA_LIVE_TEST` … `shot`) are made by GTK's own renderer
  from the window's content, never from the screen.
