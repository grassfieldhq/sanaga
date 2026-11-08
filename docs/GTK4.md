# Moving Sanaga to GTK 4 and WebKitGTK 6

Status: **started** — plan and prerequisites ready; the build needs the
WebKitGTK 6 development files, which are not installed on the development
machine yet.

## Why

GTK 3 copies every frame of a page into the window. On pages with animations
this keeps one processor core busy at 16–19 %. GTK 4 composes the page on the
graphics card (GSK): the same page should cost about as much as a still one
(≈ 2 %). GTK 4 also brings smooth fractional scaling (125 %, 150 %) and is
where WebKitGTK's new features land.

## Prerequisites

```sh
sudo pacman -S webkitgtk-6.0      # Arch / Omarchy (gtk4 is already installed)
# Debian / Ubuntu 24.04+: sudo apt install libwebkitgtk-6.0-dev libgtk-4-dev
# Fedora:                 sudo dnf install webkitgtk6.0-devel gtk4-devel
```

Rust crates: `gtk4` (0.9+), `webkit6` (0.4+), `glib` / `gio` 0.20.

## Plan (one step at a time, GTK 3 stays the default until the end)

1. **Second window shell behind a feature** (`cargo build --features gtk4`):
   a GTK 4 `ApplicationWindow` with a `GLArea` drawing egui (egui_glow works the
   same), and a `WebView` (WebKit 6) placed over the page area with a
   `GtkOverlay` / `GtkFixed`. Measure the processor on an animated page.
2. **Input**: GTK 4 event controllers (`EventControllerKey`, `GestureClick`,
   `EventControllerScroll`, `EventControllerMotion`) feeding egui, the input
   method (`IMMulticontext`), touchpad swipes (`GestureSwipe`).
3. **Pages**: `page.rs` on WebKit 6 — `NetworkSession` instead of the website
   data manager, asynchronous JavaScript (`evaluate_javascript`), new
   permission, download and policy objects, user scripts unchanged.
4. **Windows and extras**: several windows, the single-instance socket,
   tooltips, the simulator's device frame (a `GtkDrawingArea` above the page
   with input passing through), page snapshots for menus.
5. **Screen readers**: GTK 4 exposes its own accessibility (`GtkAccessible`);
   the AccessKit bridge added for GTK 3 keeps working.
6. **Switch**: GTK 4 becomes the default; GTK 3 kept for distributions without
   WebKitGTK 6 for one release, then removed.

The interface (`src/app/`, egui) does not change: only `src/shell.rs`,
`src/page.rs` and the WebKit calls in a few app files do.
