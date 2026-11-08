# What people dislike in Chrome, Firefox and Safari — and Sanaga

An audit of the complaints heard most often about the big browsers, checked
against Sanaga (November 2026). Each line says where Sanaga stands, with what
was fixed during the audit and what is left.

Legend: ✅ Sanaga does not have the problem · 🟡 partly · ❌ not yet
(Measurements: Omarchy / Hyprland, release build, `XDG_DATA_HOME` test profile.)

## Privacy

| Complaint | Usual culprit | Sanaga |
|---|---|---|
| The browser reports to its maker (telemetry, usage data) | Chrome, Edge, Firefox (opt-out) | ✅ No telemetry, no account, no update check. The only requests are the sites you open, their icons (from the site itself) and the search engine you chose. |
| Search goes to an ad company by default | Chrome, Safari (Google) | ✅ DuckDuckGo (light version) by default; Google only with the `g` shortcut. |
| Address bar sends every keystroke to a server for suggestions | Chrome, Firefox, Safari | ✅ Suggestions come from your history, favourites and tabs only — nothing leaves the computer while you type. |
| Cross-site trackers, third-party cookies | Chrome (Privacy Sandbox, Topics) | ✅ WebKit's tracking prevention (ITP) on by default, with a privacy report. No Topics / ad measurement APIs. |
| `<a ping>` reports which links you follow | Chrome, Safari | ✅ **Fixed during the audit**: Sanaga removes `ping` from a link before the click (WebKit ignores its own setting for this). |
| Notification permission spam | All | ✅ **Fixed**: requests are refused quietly (a short note says so); Settings › Privacy can let sites ask again. |
| Translation sends the page to a cloud service without saying so | Chrome | 🟡 **Labelled**: the menu now says “Translate page (Google Translate)”. An on-device translation would be better. |
| Passwords stored in the browser's own files | Chrome (Linux: basic store), Firefox (without primary password) | ✅ In the system keyring (Secret Service) — the list itself is read from the keyring. |
| Fingerprinting | All | 🟡 ITP limits some of it; no dedicated fingerprinting protection yet. |
| Cookie banners everywhere | All (Brave/Firefox auto-reject) | ✅ **Added**: built-in “No cookie banners” extension (on by default for new profiles) hides the common consent tools without accepting anything, and restores scrolling. |

## Resources and speed

| Complaint | Usual culprit | Sanaga |
|---|---|---|
| Huge memory use with many tabs | Chrome, Firefox | ✅ Tabs only start when first shown; memory saver puts idle tabs to sleep. **Measured**: 10 tabs restored, one shown: ~510 MB for everything (browser 160 MB + engine processes); 100 tabs open: under 200 MB for the interface. |
| Sleeping tabs lose what you typed | Chrome (Memory Saver) | ✅ **Fixed**: a tab with text typed in a form, a pinned tab, a tab playing sound or in a call (camera / micro on) never sleeps. |
| Battery drain / CPU when idle | Chrome, Firefox | 🟡 The interface redraws only when something changes (~1 frame/s when idle). **Measured**: static page ≈ 2 % of one core; a page with a looping animation ≈ 16–19 % — GTK 3 copies every page frame into the window. The GTK 4 / WebKitGTK 6 port (see PRODUCTION.md) removes that copy. |
| One tab crashes and leaves a blank page | Safari, older Chrome | ✅ **Fixed**: a page whose process crashed or ran out of memory shows “This page stopped working” with *Reload*; the other tabs are untouched (each page has its own process and sandbox). |
| Unusable with very many tabs | Safari, Chrome | ✅ Tabs keep a readable width; arrows and a counter button open the searchable list of all tabs; vertical tabs available. Closing works with 40+ tabs (fixed earlier). |
| Startup restores everything at once and freezes | Chrome, Firefox | ✅ Restored tabs load only when shown. |

## Annoyances

| Complaint | Usual culprit | Sanaga |
|---|---|---|
| Videos start with sound | All (settings vary) | ✅ **Fixed**: like Safari, pages may autoplay only muted. Plus an optional “No autoplay” extension. |
| Pop-ups and new windows | All | ✅ Windows only open after a click; sign-in pop-ups become linked tabs. |
| Sponsored tiles, shopping, news feeds on the new tab | Firefox, Edge, Chrome | ✅ None: favourites, frequently visited, reports. |
| Nagging to become default browser, to sign in, to sync | Chrome, Edge | ✅ Never asks; a button in Settings if you want it. |
| Data lost on crash or power cut | All (rare) | ✅ Files written safely (copy then swap), session saved continuously, crash log. |
| Accidental quit loses tabs | Firefox, Safari | ✅ Every window's tabs come back at next start. |
| Settings maze | Chrome, Firefox | ✅ macOS-style Settings with search and categories. |

## Compatibility and features

| Complaint | Usual culprit | Sanaga |
|---|---|---|
| “Your browser is not supported” on some sites | Safari, Firefox | 🟡 WebKitGTK announces itself like Safari; most sites work. Some Chrome-only web apps refuse it. Next: per-site browser identity. |
| DRM video (Netflix, Disney+, Spotify web) | Firefox on some Linux setups, Safari on Linux | ❌ WebKitGTK has no Widevine — not fixable in Sanaga. |
| Extensions limited (Manifest V3 limits ad blockers) | Chrome, Safari | 🟡 No extension store; built-in blockers and user scripts, plus extensions written by AI from a description. Blocking is by hiding (CSS), not by request filtering. |
| Weak developer tools | Safari | 🟡 WebKit's Web Inspector (docked or separate, dark theme) plus the mobile simulator. |
| Accessibility of the browser's own interface | — | ❌ Pages are accessible; Sanaga's own interface is not yet exposed to screen readers (PRODUCTION.md). |
| Reopen a closed window | — | ❌ Ctrl+Shift+T reopens tabs, not whole windows yet. |

## Changes made during this audit

1. Pages that crash or run out of memory show a recovery page instead of a blank one.
2. Autoplay only without sound.
3. `<a ping>` link tracking removed.
4. Notification requests refused quietly by default (setting to allow them).
5. Memory saver never puts to sleep tabs with typed text, calls, sound, or pinned tabs.
6. Built-in “No cookie banners” extension (on by default for new profiles).
7. Translation menu says it uses Google.
8. CPU and memory measured (above); the animated-page CPU cost is documented and
   tied to the GTK 4 port.
