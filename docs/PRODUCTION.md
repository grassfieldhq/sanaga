# Road to a daily-driver, production-ready Sanaga

This page lists what a browser needs before people can rely on it every day,
where Sanaga stands, and what is left — most important first.

Legend: ✅ done · 🟡 partly · ❌ not yet

## 1. Safety and reliability (must have)

| Need | Status | Notes |
|---|---|---|
| Pages isolated from the computer | ✅ | WebKit's bubblewrap sandbox is on when `bwrap` and `xdg-dbus-proxy` are installed (both are on Arch/Omarchy). |
| Engine security updates | 🟡 | Come from the distribution's WebKitGTK package — keep the system updated. Sanaga should warn when WebKitGTK is old. |
| Data files never half-written | ✅ | Settings, history, bookmarks, session… are written to a copy then swapped in. |
| Crash information kept | ✅ | A report per crash in `~/.local/share/sanaga/crashes/` (Sanaga itself, a page's process, an abrupt end), read on `sanaga://crashes`, sent only by the person (a GitHub issue, pre-filled). After an abrupt end the tabs come back and a bar says so. A crash inside a system library leaves no detail, only the note that the run ended abruptly. |
| One Sanaga per profile | ✅ | Links opened from other apps go to the running Sanaga as new tabs. |
| Passwords | ✅ | In the system keyring (libsecret); AutoFill only for the site really shown. |
| Never frozen, never taken down by a page | 🟡 | Audit of 11 November 2026 (docs/STABILITY-AUDIT-2026-11-11.md): nothing waited for in the window's thread any more, a panic in a frame no longer ends Sanaga, checked in CI. Left: the engine itself, and measuring drawing on real hardware. |
| Security review | 🟡 | Audit of 8 November 2026 (docs/SECURITY-AUDIT-2026-11-08.md). The readers of outside files and pages are fuzzed at every push and weekly (docs/FUZZING.md). |

## 2. The web must work (must have)

| Need | Status | Notes |
|---|---|---|
| Sign-in and payment windows (`window.open`) | ✅ | Real linked pages in new tabs; they can answer their opener and close themselves. |
| Video calls (Meet, Teams, Jitsi) | 🟡 | WebRTC and camera/microphone are on, with Sanaga's permission prompt. Needs testing on real calls; inside the sandbox, cameras go through PipeWire/portals. |
| Streaming video | 🟡 | Plain and adaptive (MSE) video work. **Netflix, Disney+, Spotify web and other DRM sites do not**: WebKitGTK has no Widevine. Nothing Sanaga can add. |
| File upload, downloads, printing | ✅ | GTK dialogs. Next: a download manager with pause/resume and "open folder". |
| Spell checking | 🟡 | On, in the interface language + English. Needs dictionaries: `sudo pacman -S hunspell-fr hunspell-en_us` (or your languages). |
| Web extensions (uBlock Origin, Bitwarden…) | 🟡 | Chrome / Firefox extensions install when they act inside pages (content scripts and styles, with a small `chrome.storage` stand-in); those needing a background page or a pop-up (password managers) don't — WebKitGTK has no WebExtensions API. Ads and trackers: built-in network blocker. |

## 3. Everyday comfort

| Need | Status | Notes |
|---|---|---|
| Several windows | ✅ | Ctrl+N, “Move to new window”, `--new-window`; all windows come back at start. Next: drag a tab out of the window. |
| Default browser | ✅ | Settings › General › Make default. |
| Smooth animated pages | ✅ | GTK 4 / WebKitGTK 6: pages are composed on the graphics card; still page 0 %, endless animation 6–10 % of one core (docs/GTK4.md). |
| Screen readers | ✅ | The interface (AccessKit) and the pages (GTK 4) are published over AT-SPI. Next: labels for every field, keyboard focus order. |
| Fractional scaling (125 %, 150 %) | 🟡 | Integer scales work; fractional ones depend on the desktop's XWayland/Wayland settings. |
| Sync between computers | 🟡 | Through a shared folder. Next: end-to-end encrypted sync of bookmarks, passwords and tabs. |

## 4. Shipping

| Need | Status | Notes |
|---|---|---|
| Install for one user | ✅ | `packaging/install.sh` |
| Packages | 🟡 | Recipes ready, codecs included as dependencies (docs/PACKAGING.md): Arch PKGBUILD (built and checked), Flatpak manifest with the ffmpeg extension, .deb metadata. Next: publish on the AUR and Flathub. |
| Updates | ❌ | Through the package manager once packaged; until then rerun `install.sh` on a new version. |
| Automatic tests of the real window | ✅ | In CI under a virtual display, and required: the window starts and a page answers (`tools/smoke.sh`), then WebDriver drives it (`tools/webdriver-test.py`, docs/AUTOMATION.md). Clippy is required too. Next: more scripted scenes of the interface. |
| Releases | ❌ | Tagged versions with notes (`CHANGELOG.md`) and signed builds. |

## GTK 4 / WebKitGTK 6

Done — see [GTK4.md](GTK4.md) for what changed and the measurements.

## Suggested order

1. **Use it daily for a few weeks** (dogfooding) and fix what breaks — the
   crash log helps. Track problems as GitHub issues.
2. **Security review** of the page scripts and internal pages; WebKitGTK
   version warning.
3. **AUR package** (quickest way to reach Omarchy users), then Flatpak.
4. **Drag a tab out to a new window** and a download manager.
5. ~~GTK 4 / WebKitGTK 6 port~~ — done (docs/GTK4.md).
6. ~~Accessibility of the interface~~ — done; next: field labels, focus order.
7. **Password-manager bridge** and encrypted sync.

Version **1.0** = sections 1 and 2 all ✅ (except DRM, which WebKitGTK cannot
do), packages, and accessibility.
