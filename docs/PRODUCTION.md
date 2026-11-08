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
| Crash information kept | ✅ | `~/.local/share/sanaga/crash.log`. Next: offer to send it (opt-in) and restore the session after a crash with a notice. |
| One Sanaga per profile | ✅ | Links opened from other apps go to the running Sanaga as new tabs. |
| Passwords | ✅ | In the system keyring (libsecret). |
| Security review | ❌ | Review the scripts Sanaga adds to pages (passwords, AutoFill, extensions), `sanaga://` pages and imports before 1.0. |

## 2. The web must work (must have)

| Need | Status | Notes |
|---|---|---|
| Sign-in and payment windows (`window.open`) | ✅ | Real linked pages in new tabs; they can answer their opener and close themselves. |
| Video calls (Meet, Teams, Jitsi) | 🟡 | WebRTC and camera/microphone are on, with Sanaga's permission prompt. Needs testing on real calls; inside the sandbox, cameras go through PipeWire/portals. |
| Streaming video | 🟡 | Plain and adaptive (MSE) video work. **Netflix, Disney+, Spotify web and other DRM sites do not**: WebKitGTK has no Widevine. Nothing Sanaga can add. |
| File upload, downloads, printing | ✅ | GTK dialogs. Next: a download manager with pause/resume and "open folder". |
| Spell checking | 🟡 | On, in the interface language + English. Needs dictionaries: `sudo pacman -S hunspell-fr hunspell-en_us` (or your languages). |
| Web extensions (uBlock Origin, Bitwarden…) | ❌ | Only Sanaga's simple script/style extensions. WebKitGTK has no WebExtensions API; a password-manager bridge would be the most useful first step. |

## 3. Everyday comfort

| Need | Status | Notes |
|---|---|---|
| Several windows, drag a tab out | ❌ | One window per profile today (Ctrl+N opens a tab). |
| Default browser | 🟡 | `xdg-settings set default-web-browser sanaga.desktop`. Next: a button in Settings. |
| Smooth animated pages | 🟡 | Pages with endless animations use more CPU than Chrome (GTK 3 copies each page frame). Moving to GTK 4 / WebKitGTK 6 fixes it — the biggest remaining technical step. |
| Screen readers | ❌ | Pages are accessible (WebKit), but Sanaga's own interface (drawn with egui) is not yet exposed to Orca. Needed for 1.0. |
| Fractional scaling (125 %, 150 %) | 🟡 | Integer scales work; fractional ones depend on the desktop's XWayland/Wayland settings. |
| Sync between computers | 🟡 | Through a shared folder. Next: end-to-end encrypted sync of bookmarks, passwords and tabs. |

## 4. Shipping

| Need | Status | Notes |
|---|---|---|
| Install for one user | ✅ | `packaging/install.sh` |
| Packages | ❌ | AUR package (Arch, Omarchy), Flatpak (all distributions), then .deb/.rpm. |
| Updates | ❌ | Through the package manager once packaged; until then rerun `install.sh` on a new version. |
| Automatic tests of the real window | 🟡 | `docs/TESTING.md` scripted actions work locally; run them in CI under a headless Wayland compositor (weston/cage). |
| Releases | ❌ | Tagged versions with notes (`CHANGELOG.md`) and signed builds. |

## Suggested order

1. **Use it daily for a few weeks** (dogfooding) and fix what breaks — the
   crash log helps. Track problems as GitHub issues.
2. **Security review** of the page scripts and internal pages; WebKitGTK
   version warning.
3. **AUR package** (quickest way to reach Omarchy users), then Flatpak.
4. **Multiple windows** and a download manager.
5. **GTK 4 / WebKitGTK 6 port** (CPU on animated pages, modern rendering).
6. **Accessibility** of the interface.
7. **Password-manager bridge** and encrypted sync.

Version **1.0** = sections 1 and 2 all ✅ (except DRM, which WebKitGTK cannot
do), multiple windows, packages, and accessibility.
