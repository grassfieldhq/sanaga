# Changelog

All notable changes to Sanaga are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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

### Changed
- Sanaga's own pages have English addresses (`sanaga://settings`,
  `sanaga://newtab`…); the old French ones still work.
- Data files are written safely (copy, then swap) so a crash cannot corrupt them.
- Ctrl+N opens a new window (it opened a tab).

### Fixed
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
