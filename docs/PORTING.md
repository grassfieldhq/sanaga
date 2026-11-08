# Porting Sanaga to Windows and macOS

Sanaga runs on Linux today. This page lists what is tied to Linux, what each
part would become on Windows and macOS, and a suggested order of work.

## What already works everywhere

- **The whole interface** (`src/app/`): tabs, toolbar, menus, internal pages,
  the mobile simulator's studio — it is drawn with [egui](https://github.com/emilk/egui),
  which runs on Windows, macOS and Linux.
- **The Light engine** (`src/html.rs`, `src/net.rs`), translations, saved data
  formats (`src/store.rs`), bookmarks import, sync through a shared folder.
- **Device frames and browser bars** (`src/frames.rs`) are drawn with cairo;
  they can be drawn with any 2D library (or egui shapes) instead.
- **Video recording** uses ffmpeg, available on all three systems.

## What is tied to Linux

| Part | Linux today | Windows | macOS |
|---|---|---|---|
| Web engine | WebKitGTK 6 (`src/page.rs`) | **WebView2** (Edge, Chromium-based) | **WKWebView** (WebKit, same engine as Safari) |
| Window and page placement | GTK 4 window, egui in a GL area, pages as GTK widgets on top (`src/shell.rs`) | Win32 / winit window, WebView2 controller as a child | AppKit / winit window, WKWebView as a subview |
| Drawing over the page (simulator frame, swipe arrow) | GTK overlay layer | a transparent child window above the WebView2 controller | a transparent `NSView` above the web view |
| Page pictures (thumbnails, screenshots, video) | WebKitGTK snapshot | `CapturePreview` | `takeSnapshot(with:)` |
| Passwords | `secret-tool` (libsecret) | Credential Manager | Keychain |
| Memory shown in the toolbar | `/proc/*/smaps_rollup` | process memory APIs | `task_info` |
| Opening folders and files | `xdg-open` | `explorer` / `ShellExecute` | `open` |
| Data folder | `~/.local/share/sanaga` | `%APPDATA%\Sanaga` | `~/Library/Application Support/Sanaga` |
| Imports from other browsers | Linux profile paths | `%LOCALAPPDATA%\Google\Chrome\User Data`…; Chrome passwords use DPAPI | `~/Library/Application Support/Google/Chrome`…; Chrome passwords use the Keychain |
| Desktop integration | Hyprland window moving, title-bar tag, minimise (`src/desktop.rs`) | standard window commands | standard window commands, traffic lights native |
| GL loading | EGL / GLX (`src/shell.rs`) | WGL (or ANGLE) | CGL / Metal via wgpu |

## Suggested approach

1. **Put the engine behind one interface.** `src/page.rs` already wraps every
   call Sanaga makes to a page (load, back/forward, zoom, scripts, snapshots,
   downloads, permissions, user agent, inspector). Turn it into a trait with a
   WebKitGTK implementation, then add WebView2 and WKWebView ones — the
   [`wry`](https://github.com/tauri-apps/wry) crate already wraps both and can
   be used as a starting point on those two systems.
2. **Put the window behind one interface.** `src/shell.rs` does three things:
   run egui, place pages over the page area (`set_page_area`, `attach`), and
   carry window commands. On Windows and macOS, `eframe`/`winit` plus native
   child views for the pages is the natural fit (native child views are well
   supported there, unlike on Wayland).
3. **Small system services** — a `platform` module with `open_path`,
   `data_dir`, `memory_used`, and a `keyring` using the `keyring` crate.
4. **Imports** — per-system profile paths and password decryption.
5. **Packaging** — an MSI or MSIX for Windows, a signed `.app` / DMG for
   macOS, built by CI on `windows-latest` and `macos-latest` runners.

## Things to know

- On **Windows** the engine is Chromium-based (WebView2): pages render like in
  Edge or Chrome. The mobile simulator's iOS mode still sends an iPhone user
  agent, but the rendering engine stays Chromium.
- On **macOS** the engine is WebKit, like Safari — closest to iPhone and iPad
  rendering, which is good for the simulator.
- Some features depend on the engine's API and may differ: per-site
  permissions, the privacy report (WebKit's tracking prevention), the docked
  inspector, and the touchpad swipe.

The Linux version stays the reference: each port should pass the same unit
tests and the hand checks in [TESTING.md](TESTING.md).
