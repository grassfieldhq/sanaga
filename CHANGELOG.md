# Changelog

All notable changes to Sanaga are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `SANAGA_DEBUG_STALLS=30`: every time the window did not answer, with what
  held it; `tools/lag-audit.sh`: the real window on a heavy profile with
  slow helper programs, every feature in turn, in CI.
- Crash reports (`sanaga://crashes`, also from About and the command
  palette): when Sanaga or a page's process stops, a short report is kept
  on the computer — what stopped, Sanaga's and the system's versions, no
  page address, the home folder's name taken out. Read it, copy it, send it
  as a pre-filled GitHub issue, or delete it; nothing is sent by itself. The
  newest 20 are kept.
- After an abrupt end (a crash, a kill, a power cut) the next start says so
  in a bar: the tabs are back, or one click reopens them when "restore
  tabs" is off.
- Asked to stop by the system (logging out, `kill`, Ctrl+C), Sanaga closes
  as with Quit: tabs saved.
- Activity page: memory and processor **tab by tab**. Each page's process
  tells Sanaga which tab it serves (a small library loaded by the page
  processes, `page-helper/`); processes are named after their tab, shared
  ones are marked, and a tab whose page no longer answers says so.
- `sanaga --automation`: test software drives Sanaga through WebDriver
  (WebKit's own driver), in a window that keeps nothing
  (docs/AUTOMATION.md).
- Fuzzing of the readers of outside files and pages (Light engine's HTML,
  extensions of other browsers, imported bookmarks, block lists): a
  repeatable fuzzer with the unit tests, libFuzzer targets in `fuzz/`
  (docs/FUZZING.md).
- Menus and panels open over the living page: the page keeps moving
  under them (a video goes on playing) instead of being replaced by a
  picture of itself, and opening a menu no longer makes the window stutter.
  The mobile simulator keeps the old way.
- Split view (Ctrl+Shift+E): the tab in use and the web tab used before
  it side by side; a click in a page makes it the tab in use.
- Mobile preview next to the page (command palette › "Aperçu mobile à côté
  de la page"): the same page as a phone gets it, a phone wide, following
  the page's address and scrolling.
- Activity page (`sanaga://activity`, command palette › "Activité"):
  memory and processor of Sanaga's interface, network and page processes,
  and every tab with its state; a running tab can be put to sleep.
- Emoji in colour (the system's Noto Color Emoji, else a built-in black and
  white set) and Chinese, Japanese and Korean text in the interface's own
  texts (tab titles, menus, history).
- A new tab grows into the tab bar, the other tabs take their new width
  smoothly, and the overview's cards rise into place.
- Command palette (Ctrl+K, also Ctrl+Shift+A): one field for the open
  tabs, Sanaga's commands, favourites, history and the web; arrows and
  Enter.
- The address bar completes a site's name as you type ("wiki" →
  "wikipedia.org", the added part selected) and looks its address up at
  once, so the page starts sooner. Not in private tabs.
- Full block lists: EasyList and EasyPrivacy (some 100 000 rules), fetched
  once a week from easylist.to, converted on this computer and compiled
  by WebKit. Settings › Privacy › "Listes de blocage complètes" (on by
  default; the small built-in list stays). The first compilation takes
  about two minutes in the background; the lists add about 40 MB to each
  page's process.
- Sync: favourites and reading list are merged entry by entry (an entry
  added, renamed, moved or removed on one computer reaches the others; two
  computers changing different things lose nothing), and the folder can be
  encrypted with a secret phrase (Settings › Sync; XChaCha20-Poly1305, key
  from the phrase with Argon2id, the phrase kept in the keyring).

### Changed
- Nothing is waited for in the window's thread any more (stability audit,
  docs/STABILITY-AUDIT-2026-11-11.md): the keyring, `xdg-settings`, the
  icon cache and the sync round run on threads; data files are saved by a
  thread, in order, still safely. Opening Settings with a slow keyring took
  24 s; it takes a frame.
- The new tab page and the lists no longer work the history and the site
  names out again at every frame.
- Block lists are handed to WebKit one at a time after their weekly
  refresh (three short holds instead of one long one).
- CI: Clippy and the test of the real window (it starts, a page answers,
  WebDriver drives it) now stop a merge; they were advice only. A weekly
  job fuzzes the readers with libFuzzer.
- Starts faster: the web engine is made after the window's first picture,
  and GTK is told light or dark before it reads its theme (measured: the
  interface drawn after about 1.0 s instead of 1.2–1.3 s).
- The sidebar slides in and out; the loading bar moves with the screen's
  refresh.
- Address suggestions: the same page is listed once, addresses are shown
  decoded, without https:// and www.
- Extensions: manifests are read as real JSON (comments, commas inside
  patterns, translations), archives are checked (sizes, checksums, a total
  limit) and refused when damaged; AI answers are read from their place in
  the JSON.

### Security (audit of 8 November 2026)
- Sanaga's page scripts and its message channel run in an isolated
  JavaScript world, top frame only; page messages are checked against the
  page shown and against what the person started.
- Passwords are filled only after a click in Sanaga's bar (no fill on page
  load, no account list in the page), with the origin checked again.
- Downloads: safe file names, never an overwrite, extended attributes with
  their origin, and Safari's question for downloads a site starts by itself.
- Pages can no longer read the clipboard without a user gesture.
- AI translation and summaries only with tool-less models; translation is
  text-only, bounded and reset on navigation.
- No one-click codec install: the exact command in a terminal, and none at
  all on a distribution build (`SANAGA_DISTRO`).
- Re-audit fixes: light-engine downloads marked and asked about like
  WebKit's; view-source links only from Sanaga's own source page; the
  no-question download allowance tied to a real action in the page (none
  for restored tabs), kept per profile, container and private session, and
  applied to preview windows; "Save password?" only after a real sign-in;
  a slow peer can't stall the single-instance socket; extensions installed
  from a file are switched off until reviewed.
- After merging main: the error page's buttons go through Sanaga's
  isolated world and are accepted only for that failure; Ctrl+click and
  middle-click open only real clicks on web links, and keep private tabs and
  containers apart; codec packages are built with `SANAGA_DISTRO` and depend
  on WebKit's sandbox; release actions pinned, write access for publishing
  only.
- After the second merge of main: the certificate error page's buttons
  need a per-page secret and the exception ends at the tab's next page; a
  background tab's dialog waits on its tab; frames' prompts are worded
  neutrally; storage access without asking only for identity providers;
  guest windows keep no password or favicon; profile files 0600; Share
  keeps private tabs and containers apart; releases only from version
  tags, never replaced.
- Favicons come from WebKit; AI-written extensions are installed switched
  off; private temporary folders; escaped `.desktop` entries; a checked and
  bounded single-instance socket; debug-only test hooks; private data
  permissions; the system's certificate store for Sanaga's own requests.

### Changed
- Password AutoFill no longer lists accounts under the sign-in field nor
  fills a site's only account by itself: the accounts are offered in
  Sanaga's bar.
- The address bar shows the page's address as Chrome does
  (`fr.wikipedia.org/wiki/Yaoundé`: the site, then the path dimmed and
  decoded; https:// and www. left out) instead of the site's name only, so
  it changes from page to page. Settings › Apparence › "Toujours afficher
  les URL en entier" shows the whole address.

### Fixed
- Find in page (Light engine, Reader) made Sanaga crash on a page holding
  letters such as İ or ẞ; the address bar's completion could crash on a
  pasted Kelvin sign.
- A panic while making a frame no longer ends Sanaga with all its tabs:
  the frame is dropped, a report kept, and Sanaga goes on (three in a row
  still stop it, and the tabs come back at the next start).
- Importing bookmarks from a damaged Chrome `Bookmarks` file could run for
  ever (a file holding `[}`), found by fuzzing; a broken surrogate pair in
  a bookmark's name is now read as an unknown character.
- The address bar now follows every move of the page: sites that change
  page without loading a new one (YouTube, Google, Facebook, X… with
  history.pushState), #anchors and going back inside them. These moves are
  also kept in the history and the restored session.
- Sites behaving as if there were no Internet while connected (Google and
  others): Sanaga and its engine now know whether there is a network from
  the system's routes, not from NetworkManager's Internet check (on Ubuntu
  that check often fails while the connection works, and pages were told
  they were offline). A load error is only called "not connected" when the
  error itself says so.
- Video calls (Google Meet, Teams, Zoom, Jitsi, Whereby, Webex, Discord):
  when this system's WebKitGTK is built without WebRTC (the call can't
  connect, the "join" button stays stuck), they open in a browser that can
  make calls, the way Netflix does. The .deb, .rpm and Arch packages now
  bring libnice for GStreamer, which WebKit needs for calls.

### Changed
- Netflix, Disney+, Spotify and the other DRM sites are no longer loaded in
  Sanaga, where they can't play: an address typed, a favourite or a link
  clicked goes straight to the app or browser that plays them (Chrome,
  Chromium with Widevine, Brave, Edge, Firefox, also as Flatpaks, or the
  Spotify app), and the tab stays where it was. The first time, a Sanaga
  page offers it in one click, with "Always open this way" ticked; without
  such a browser it gives the command to install Firefox.
- Sanaga's short messages are shown above the page (they were hidden under
  web pages).
- Release tags follow Semantic Versioning exactly: `0.4.0`, no `v` in front
  (the releases are named the same way).

## [0.4.0] — 2026-11-09

### Added
- Profiles as in Chrome: the profile's picture (photo, sign or initial on its
  colour) at the right of the toolbar, with the name for extra profiles; its
  menu lists this profile, the others (one click opens their window), add a
  profile, a Guest window (`--guest`, nothing kept), manage profiles and close
  this profile. `sanaga://profiles` is now “Who's using Sanaga?”: profile
  cards, adding (name and colour), deleting, and this profile customised
  (name, colour, picture: initial, a sign or a photo).
- Share popover redesigned: the page, its link with a Copy button, “Send to”
  (e-mail, WhatsApp, Telegram, X, Facebook, LinkedIn), the actions as tiles
  and the QR code.

### Fixed
- With the address bar (or the tabs) at the bottom, the suggestions, the ⋮
  menu, Share, the » menu, the ✦ menu, the favourite popover, the group
  editor and the website settings open above it instead of off the bottom.

## [0.3.0] — 2026-11-09

### Fixed
- “Sign in with Google” (and Apple, Microsoft, Facebook, GitHub…) inside
  other sites (X, Reddit…): the sign-in frame's request to use its cookies
  (Storage Access) was refused silently; sign-in services now get it, other
  sites are asked about, as in Safari.
- A window opened by a page (sign-in, payment) that closes itself brings back
  the page that opened it, not the next tab.
- A message from a background page (e.g. a video it cannot play) no longer
  brings that tab to the front.
- The per-site identity is no longer switched for frames inside a page (it
  could reload the whole tab with the frame's address).

### Changed
- Less processor use: Sanaga's own window measures its memory in the
  background every 10 s (it walked every process every 2 s on the window's
  thread: a 30 ms hitch each time); a page playing sound no longer makes the
  window refresh 4 times a second; the password helper only looks at new
  form fields (it watched every style change of busy pages such as Gmail, X
  or YouTube).
- Video: without an AV1 decoder in the graphics card, sites get VP9 or H.264
  instead of AV1 (decoded by the processor otherwise); on YouTube, VP9 too
  when the card only decodes H.264.
- Memory: a page's process gives memory back (caches, JavaScript memory)
  from about 340 MB on 8 GB computers, 680 MB from 16 GB (WebKit waited for
  1 GB); drawing apps and games draw with the graphics card (2D canvas).
- Browser games can capture the mouse (pointer lock), as in Chrome.

## [0.2.0] — 2026-11-09

### Added
- Sign-in and payment windows opened by sites (`window.open`) are real pages
  linked to their opener, shown as new tabs, and close themselves when done.
- One Sanaga per profile: addresses opened from other applications go to the
  running Sanaga as new tabs.
- Pages run in WebKit's bubblewrap sandbox when available.
- Video calls: WebRTC and camera/microphone (with Sanaga's permission prompt).
- Spell checking in text fields (interface language + English).
- Crash information written to `crash.log`.
- `sanaga --export-icon` and `packaging/install.sh` (user install).

- Several windows (Ctrl+N, “Move to new window” on a tab, `--new-window`);
  every window's tabs come back at the next start.
- Password AutoFill: saved accounts listed under the sign-in field, automatic
  fill when a site has one account, two-step sign-ins and sign-in forms that
  appear after the page loads.
- Settings page with categories, search, and a “Make default” button.
- The web inspector and GTK dialogs follow Sanaga's dark theme.

- Layout choices: tabs on top, at the bottom, on the left or on the right
  (vertical tabs), address bar on top, at the bottom or floating.
- Page summaries (✦ button, Ctrl+Shift+Y): Sanaga's own (no AI), a local model
  through Ollama, or Claude with your API key (kept in the keyring).
- Create an extension by describing it (`sanaga://create-extension`).
- Follows the Grassfield system's accent colour and light / dark appearance
  ("System"), or a chosen accent; new Grassfield-style icon (also as SVG).

### Added (November 2026)
- Packages for every Linux distribution, built and published by GitHub at
  every push: Flatpak, .deb (Debian, Ubuntu), .rpm (Fedora), Arch package and
  a .tar.gz with an installer (`docs/RELEASING.md`).
- Settings › Appearance: the tabs' close button on the left (macOS) or the
  right (Chrome), in the tab bar and the vertical tabs.
- Error pages in Sanaga's design for every kind of failure (DNS, offline with
  automatic reload, refused, timeout, certificate, blocked port, missing file).
- alert / confirm / prompt / leave-page / HTTP login shown as Sanaga sheets.
- ✦ menu: summarise or translate; translation target language (Settings ›
  Intelligence, or in the menu).
- Right-click a link › Open link in a private tab.
- Tab switcher: Ctrl+Tab shows the tabs used last as pictures (most recent
  first); Ctrl+Page Down / Up follow the tab bar.
- Containers: tabs with their own cookies, logins and site data.
- Network blocker of ads and trackers (WebKit content blocker), on by default,
  switchable per site.
- Website settings per site: JavaScript, web fonts, blocker, dark mode, page
  font, no sticky bars, copy unlocked, removed elements; the identity and its
  zoom in the same panel.
- Page tools: floating video, remove an element, edit the page's text, unlock
  copy and paste, archived version (Internet Archive), link preview window
  (`--preview URL`), PDF reader (WebKit's built-in viewer).
- Focus mode (F9), keep on top (Hyprland), compact mode,
  window border (accent, gradient, animated), app icon colours, low power mode
  (automatic on battery).
- Programmable toolbar buttons and the » overflow menu.
- Tree tabs in the vertical tab list; move a tab group to a new window;
  windows listed in the sidebar.
- Extensions from Chrome (.crx, .zip, manifest.json) and Firefox (.xpi), and
  user scripts with `@include`.
- Profile emoji and a launcher of its own per profile.

### Changed
- Tab groups work as in Chrome: every tab stays in the tab bar; a group is a
  coloured label before its tabs with a line of its colour under them; click
  folds it; a new group opens its editor at once (name, colour, new tab,
  ungroup, move to a new window, close); dragging a tab into or out of a
  group adds or removes it; colours, names and folding are kept with the
  session. New tabs (Ctrl+T) are no longer put in the shown tab's group.
- Sanaga now runs on **GTK 4 and WebKitGTK 6** (it needs `gtk4` and
  `webkitgtk-6.0`): pages are composed on the graphics card, an idle page costs
  nothing, animated pages cost less, and screen readers can read the pages as
  well as the interface. See `docs/GTK4.md`.
- The web inspector shortcut closes the inspector when pressed again.
- Sanaga's own pages have English addresses (`sanaga://settings`,
  `sanaga://newtab`…); the old French ones still work.
- Data files are written safely (copy, then swap) so a crash cannot corrupt them.
- Ctrl+N opens a new window (it opened a tab).

### Fixed
- Make default browser: on Omarchy (and desktops with a routing entry), the
  choice goes to the general mimeapps.list that the routing entry reads, with
  web links and HTML files; checked the same way the desktop does.
- Ctrl+click / middle click on a link opens it in a tab behind (Ctrl+Shift:
  in front, Shift: a new window), under the page and in its container.
- A file the browser cannot show (archive, installer, unknown type) sent
  without a “download” header was silently ignored; it is now downloaded.
- Clicking an address suggestion did nothing.
- A page could ask for the saved password of another site (the site is now
  checked against the page really shown).

## [0.1.0] — first public release

### Browser
- Native GTK 3 window (Wayland and X11) with the interface drawn by egui and
  web pages shown by WebKitGTK 4.1; optional Light engine for simplified pages.
- Safari-inspired toolbar and address field; Chrome-style tab row with pinned
  tabs, drag and drop, tab groups (colours, names, folding), tab menu,
  scrolling and narrow tabs for many open tabs.
- Address suggestions (history, bookmarks, open tabs, calculator, search
  shortcuts) with keyboard navigation.
- Sidebar, tab overview, tab search, Reader mode, find in page, print, save,
  full screen, page source view with colours and line numbers.
- Private tabs, profiles, sync through a shared folder, passwords and AutoFill
  in the system keyring, imports from Chrome-based browsers and Firefox.
- Built-in extensions (ad blocker, no autoplay, dark mode) and user script/style
  extensions.
- Memory saver, lazy loading of tabs, per-site zoom and permissions, privacy
  report, cross-site tracking prevention.
- Customisable toolbar, touchpad swipe and mouse buttons for back/forward,
  native tooltips, right-click menus that open on press.
- Web inspector (docked or detached).
- Interface in English, French, Spanish, Portuguese and German.

### Mobile simulator
- 120 devices (phones, tablets, computers) and free sizes, rotation, zoom.
- Drawn device frames with status bar, five colours, and your own frame pictures.
- Safari-style / Chrome-style browser bars inside the device, working buttons and
  editable address.
- Mobile user agents and touch emulation.
- Screenshots and MP4 video recording.
