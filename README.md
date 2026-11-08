# Sanaga

**A fast, light web browser for Linux** — built in Rust on WebKitGTK, with a
Safari-inspired window, Chrome-style tabs and groups, and a built-in mobile
simulator for web developers.

Sanaga is named after the Sanaga, the longest river in Cameroon.

![Sanaga showing a Wikipedia article](docs/screenshots/browser.png)

| Mobile simulator | New tab |
|---|---|
| ![The mobile simulator showing a page in an iPhone frame with Safari-style bars](docs/screenshots/mobile-simulator.png) | ![The new tab page with favourites and reports](docs/screenshots/new-tab.png) |

---

## Why Sanaga

- **Light on memory.** Tabs only start their page when you first show them,
  a memory saver puts long-unused tabs to sleep, and only a few web pages run
  at the same time. A hundred open tabs do not mean a hundred running pages.
- **Quiet when idle.** The interface only redraws when something changes.
- **Native and smooth.** One native GTK window (Wayland or X11): the page and
  the interface are laid out and drawn together, so resizing stays smooth.
- **Private and safe by default.** Pages run in a sandbox (bubblewrap),
  cross-site tracking prevention, a privacy report, an optional ad blocker,
  passwords kept in the system keyring, no telemetry.

## Features

**Browsing**
- Full web engine (WebKitGTK), plus an optional *Light* engine for very fast,
  simplified pages.
- Chrome-style tab row: pinned tabs, drag to reorder, tab groups with colours,
  folding and naming, mute, duplicate, “new tab to the right”, reopen closed tab.
- Address bar with suggestions (history, bookmarks, open tabs, calculator,
  search shortcuts), arrow-key navigation, site info and zoom per site.
- Sidebar (bookmarks, reading list, tab groups, history), tab overview with
  thumbnails, tab search.
- Reader mode, find in page, print, save page, full screen, page source view
  with colours, line numbers and line wrapping (Ctrl+U).
- Private tabs, multiple profiles, sync of bookmarks/reading list and open tabs
  through any shared folder (Syncthing, Nextcloud, a USB key…).
- Passwords and AutoFill (system keyring), import of bookmarks, history and
  passwords from Chrome, Chromium, Brave, Edge, Vivaldi and Firefox.
- Simple script/style extensions (built-in: ad blocker, no autoplay, dark mode;
  drop your own `.js`/`.css` files in the extensions folder).
- Customisable toolbar (right-click it), touchpad swipe and mouse side buttons
  for back/forward, native tooltips.
- Web inspector (Ctrl+Shift+I or F12), docked or in its own window.
- Interface in English, French, Spanish, Portuguese and German.

**Mobile simulator** (Ctrl+Shift+M, or the phone button)
- 120 devices: iPhone, Pixel, Galaxy, Tecno, Infinix, itel, Xiaomi, OnePlus…,
  tablets and computers, or any free size; portrait and landscape; zoom.
- Drawn device frames (camera island, notch, punch-hole, home button, tablet,
  laptop, monitor) in five colours, with a status bar.
- The device's own browser around the page: Safari-style bars on Apple
  devices, Chrome-style bars on Android and computers. Their back, forward,
  reload buttons and the address field work.
- Mobile browser identity (iOS / Android user agent) and touch emulation, so
  sites send their mobile version.
- Screenshots (with the device, screen only, whole page) and **video
  recording** (MP4) of the device.
- Your own frame pictures: put a PNG with a transparent screen in the frames
  folder, named like the device; Sanaga finds the screen by itself.

## Install

Sanaga runs on Linux (Wayland or X11). You need:

| What | Arch / Manjaro | Debian / Ubuntu (24.04+) | Fedora |
|---|---|---|---|
| Rust 1.88+ | `rustup` | `rustup` | `rustup` |
| GTK 3 | `gtk3` | `libgtk-3-dev` | `gtk3-devel` |
| WebKitGTK 4.1 | `webkit2gtk-4.1` | `libwebkit2gtk-4.1-dev` | `webkit2gtk4.1-devel` |
| *(optional)* passwords | `libsecret` (`secret-tool`) | `libsecret-tools` | `libsecret` |
| *(optional)* video recording | `ffmpeg` | `ffmpeg` | `ffmpeg` |

Then:

```sh
git clone https://github.com/grassfieldhq/sanaga.git
cd sanaga
cargo build --release
./target/release/sanaga
```

To install it for your user — program in `~/.local/bin`, launcher in your
applications menu, icon — without admin rights:

```sh
packaging/install.sh              # build and install
packaging/install.sh --uninstall  # remove it (your data stays)
```

## Usage

```sh
sanaga [address …]            # open these addresses (or restore the last session)
sanaga --profile Work         # use the "Work" profile (its own data)
sanaga --app https://…        # open one site on its own, as a web app
sanaga --reader https://…     # open in Reader mode
sanaga --apercu               # start on the tab overview
sanaga --screenshot out.png   # draw the window once, save a picture, quit
```

### Keyboard shortcuts

| Action | Keys |
|---|---|
| New tab / close tab | Ctrl+T / Ctrl+W |
| Reopen closed tab | Ctrl+Shift+T |
| New private tab | Ctrl+Shift+N |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Address bar | Ctrl+L, F6 |
| Reload | Ctrl+R, F5 |
| Back / forward / home | Alt+← / Alt+→ / Alt+Home |
| Find in page | Ctrl+F |
| Bookmark this page / bookmarks bar | Ctrl+D / Ctrl+Shift+B |
| Add to reading list | Ctrl+Shift+D |
| History / downloads | Ctrl+H / Ctrl+J |
| Reader | Ctrl+Shift+R |
| Tab overview / tab search | Ctrl+Shift+O / Ctrl+Shift+A |
| Sidebar | Ctrl+Shift+L |
| Zoom in / out / reset | Ctrl++ / Ctrl+− / Ctrl+0 |
| Page source / print / save | Ctrl+U / Ctrl+P / Ctrl+S |
| Web inspector | Ctrl+Shift+I, F12 |
| Mobile simulator | Ctrl+Shift+M |
| Full screen | F11 |
| Clear browsing data | Ctrl+Shift+Delete |

### Where your data is

Everything stays on your computer, in `~/.local/share/sanaga/`
(or `$XDG_DATA_HOME/sanaga/`): settings, history, bookmarks, session, site
data (`webkit/`), extensions (`extensions/`), device frames (`cadres/`), and one
sub-folder per extra profile (`profils/<name>/`). Passwords are not stored
there: they go to the system keyring.

## How it is built

| Part | Files |
|---|---|
| Native window: GTK 3 window, the interface drawn by [egui](https://github.com/emilk/egui) in a GL area, pages as WebKit widgets on top, input and window commands | `src/shell.rs` |
| One web page (WebKitGTK view, scripts, downloads, signals) | `src/page.rs` |
| The interface (tabs, toolbar, menus, internal pages, simulator…) | `src/app/` |
| Light engine (HTML to blocks, Reader) | `src/html.rs`, `src/net.rs` |
| Saved data, profiles, sync, keyring, imports | `src/store.rs`, `src/sync.rs`, `src/keyring.rs`, `src/migrate.rs`, `src/import.rs` |
| Device frames and drawn mobile browsers (cairo) | `src/frames.rs`, `src/app/devices.rs`, `src/app/scene.rs` |
| Translations | `src/i18n.rs`, `assets/i18n/` |

`eframe` is only used for `--screenshot` (a picture of the interface without
web engine). Comments in the code explain *why* things are done the way they are.

## Platforms

| System | Status |
|---|---|
| Linux (Wayland, X11) | ✅ Supported |
| FreeBSD and other BSDs | Should work (GTK 3 and WebKitGTK are available), untested |
| Windows | ❌ Not yet — planned, see [docs/PORTING.md](docs/PORTING.md) |
| macOS | ❌ Not yet — planned, see [docs/PORTING.md](docs/PORTING.md) |

Building on Windows or macOS stops with a clear message. The interface code
(egui) already runs everywhere; the parts to port are the window, the web
engine bridge and a few system services.

## Known limitations

- Linux only for now (GTK 3 + WebKitGTK 4.1) — see [Platforms](#platforms).
- Pages with endless animations cost more CPU than in Chrome or Firefox: GTK 3
  copies every page frame into the window. Moving to GTK 4 / WebKitGTK 6 would
  remove that copy.
- Extensions are simple page scripts and styles; Chrome Web Store extensions are
  not supported.
- Video recording has no sound; its size is the device's size on screen (use
  100 % zoom for sharper videos).
- Window moving, maximising and the title bar integration have extra support for
  Hyprland; other desktops use the standard GTK behaviour.

## Roadmap

What is left before Sanaga is a production-ready daily browser, in order:
[docs/PRODUCTION.md](docs/PRODUCTION.md).

## Contributing

Contributions are welcome — bug reports, translations, devices, code. Please read
[CONTRIBUTING.md](CONTRIBUTING.md) and our [Code of Conduct](CODE_OF_CONDUCT.md).
Security issues: see [SECURITY.md](SECURITY.md).

## License

Sanaga is released under the [MIT License](LICENSE).
The bundled fonts (Inter, Noto Sans, Noto Serif) are under the SIL Open Font
License 1.1 — see [assets/FONTS-LICENSE.md](assets/FONTS-LICENSE.md).

*Sanaga is an independent project. It is not affiliated with Google, Apple,
Samsung or any other company. Product names such as Chrome, Safari, iPhone,
iPad, Pixel or Galaxy are used only to describe what the simulator imitates;
they belong to their owners. The device frames and browser bars are drawn by
Sanaga itself and contain no third-party logos or artwork.*
