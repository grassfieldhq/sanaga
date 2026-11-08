# Strict review of Sanaga's features — November 2026

Every feature was checked in a temporary profile, with its own empty keyring
(private D-Bus session), on Omarchy / Hyprland, release build. "Measured" means
the result was read inside the page (a script probe), not only seen.

Legend: ✅ works · 🔧 a fault was found and fixed in this review · ⚠️ known limit

## Pages and navigation

| Feature | How it was checked | Result |
|---|---|---|
| Internal pages (history, downloads, favourites, passwords, extensions, profiles, clear data, sites, import, create an extension, welcome tour, about) | each opened, picture checked | ✅ |
| Page source (Ctrl+U) | Arabic Wikipedia page | 🔧 **crash fixed**: the source view stopped Sanaga on text in non-Latin alphabets (a cut inside a character). Now covered by a test. |
| Light engine (Reader, simplified pages) | test with odd pages: mixed alphabets, broken tags, cut entities, unclosed comments | ✅ (new robustness test) |
| Reader, page summary without AI | Arabic Wikipedia page | ✅ no crash · ⚠️ right-to-left text (Arabic, Hebrew) is drawn in left-to-right word order in Sanaga's own Reader and summary (pages shown by WebKit are correct) |
| Downloads of files a page can't show | real download | ✅ |
| PDF reader | local PDF | ✅ (WebKit's viewer: search, zoom, print) |
| Archived version (Internet Archive) | menu, address checked | ✅ |

## Tabs, windows, session

| Feature | How it was checked | Result |
|---|---|---|
| Private tabs | cookie set in a normal tab, absent in the private tab | ✅ measured |
| Reopen closed tab | close then Ctrl+Shift+T | ✅ |
| Tree tabs | tabs opened from pages, session file, restart | 🔧 **the tree was lost at restart**: the tab each one came from is now saved with the session |
| Containers | cookie in a container, absent in a normal tab; both still there after restart | ✅ measured |
| Session restore with containers | session file + restart | ✅ |
| Link preview from another program (`--preview`) | second launch while Sanaga runs | ✅ preview window opens in the running Sanaga |
| Web app mode (`--app`) | picture | ✅ |
| Focus mode | keys pressed in a page | ✅ F9 · 🔧 **Ctrl+Shift+K removed**: on Grassfield it opens the Activity monitor (a desktop-wide shortcut), so it could never reach Sanaga |
| Floating video, keep on top, several windows, screen readers | previous rounds | ✅ |

## Privacy and website settings

| Feature | How it was checked | Result |
|---|---|---|
| Network blocker (global switch) | page loading two trackers and a library | ✅ measured: trackers blocked, library loads; switch off: all load |
| Blocker off for one site | w3schools.com | ✅ measured (0 → 4 tracker requests) |
| JavaScript off for one site | en.wikipedia.org | ✅ measured (`client-js` absent) |
| Site fonts off | github.com | ✅ measured (2 → 0 web fonts loaded) |
| No sticky bars | w3schools.com | ✅ measured (1 → 0 bars stuck at the top) |
| Copy unlocked | probe | ✅ |
| Element remover | simulated pointing and click, reload | ✅ kept after reload · 🔧 **remembered selector was too broad** (`p:nth-of-type(1)` would hide the first paragraph of every block of the site); it now starts from the page's body |
| Link preview / floating video from a private tab or a container | code review | 🔧 **they used the profile's normal cookies**; now a private tab's small windows keep nothing, a container's keep its own cookies |
| "Site settings" page | picture | 🔧 the per-site settings (JavaScript, fonts, look, removed elements) were missing there; now listed with a reset button |
| Passwords | isolated keyring | ✅ (tests must never use the system keyring: a temporary profile reads it) |

## Extensions

| Feature | How it was checked | Result |
|---|---|---|
| Firefox `.xpi` extension | installed, injected, storage | ✅ (previous round) |
| User scripts | `@match`, `@exclude`, `GM_addStyle`, `GM_setValue/GM_getValue`, `GM_info` | 🔧 `@exclude` was ignored and the Greasemonkey functions were missing; ✅ measured now |
| Install button | Extensions page | 🔧 it was only in Settings; now also on the Extensions page |
| Extension switched off | previous round | ✅ applies at once |

## Comfort

| Feature | How it was checked | Result |
|---|---|---|
| Low power mode | interface wake-ups over 10 s, idle | ✅ measured: 11 → 3 |
| Window border, compact mode, app icons, own buttons, » menu, settings search | pictures (previous round) | ✅ |
| `--screenshot` | run with the screen on | ✅ (works through the GTK window) |

## Not checked live (and why)

- The page's right-click menu entries: the keyboard can't open WebKit's menu
  in a test, and moving your pointer was not acceptable. The same tools were
  checked from the ⋮ menu.
- File dialogs (install an extension, import, save page), print dialog,
  camera / microphone prompts: they need a person at the window.
- Translation and AI features with an account (Claude, Codex…): they need the
  person's accounts.
- Keyboard shortcuts against desktop-wide ones: only the shortcuts listed
  above were tried, to avoid opening the person's applications.

## Video and music (November 2026)

| Site | What it needs | Sanaga |
|---|---|---|
| YouTube, Vimeo, Twitch… | GStreamer decoders: H.264, AAC (gst-libav), AV1, Opus parser (gst-plugins-bad), VP9 (gst-plugins-good) | 🔧 **YouTube said "can't play this video" on this computer**: H.264, AAC, AV1 and the Opus parser were missing from the system. Sanaga now checks the decoders, shows a bar on video sites and installs them in one click (the system asks for the password); Settings › General › Video and music. |
| Netflix, Disney+, Prime Video, Spotify web… | Widevine (Google's DRM) — not available to WebKitGTK at all | ⚠️ can't be played inside Sanaga. Sanaga opens them in the Spotify app or in a browser that has Widevine, as an app window, by itself when "Open protected sites in a compatible browser" is on. |

## Known limits

- Right-to-left text in Sanaga's own Reader / summary (see above).
- Emoji typed in Sanaga's own interface show as initials.
- DRM video (Netflix, Spotify web) can't play in WebKitGTK.
- Extensions needing a background page or a pop-up are not supported.
- Keep on top and placing the floating video need Hyprland.
