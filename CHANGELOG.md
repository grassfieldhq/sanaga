# Changelog

All notable changes to Sanaga are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
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
