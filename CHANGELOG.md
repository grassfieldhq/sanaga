# Changelog

All notable changes to Sanaga are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

The security review and the functional review of 0.6.0.

### Security
- **An outside tool's call is shown whole before it is accepted**: what is
  sent is shown in full, in a box of its own (monospace, wrapped), with the
  characters that draw nothing written out; "Accept" waits until the box
  has been scrolled to its end, in a box tall enough to be read. A call too
  long to be read is refused, not shown in part. A word that mixes letters
  of alphabets with look-alikes is pointed out. (It was cut at 600
  characters, without a mark.)
- **A site the administrator keeps from models stays kept when the person
  changes tabs**: the model is chosen, and the policy applied, for the tab
  the question was asked in, as decided when it was asked — and again for
  what each tab shows when its page is read, by a question or by a tool.
  A conversation that holds Sanaga's own summary of such a site is kept
  from every model from then on (its follow-ups too), and the assistant's
  search of earlier conversations never returns it. (Going to another tab
  while a summary of such a site was loading sent its text to the model.)
- **"Accept" is inert for half a second** after a new action appears, and
  takes a click begun after that: the second click of a double click no
  longer accepts the next action unread.
- **Forms**: card fields are also recognised by their name, id, label or
  placeholder (card number, security code, expiry date, in the interface's
  languages), and fields that are invisible, transparent, of no size,
  clipped away or outside the page are neither listed nor filled; fields
  in frames (payment services' forms) never were. The claim is now what
  is guaranteed: these are heuristics on what the page shows, and what
  holds whatever the page does is that each value is shown before it is
  typed and no form is ever sent.
- **Outside tools' programs get a small environment**: `PATH`, `HOME`,
  `LANG`, `LANGUAGE`, `LC_*` and `XDG_RUNTIME_DIR`, plus the variables the
  person writes before the command (`NAME=value command`) — no longer all
  of Sanaga's (keys, tokens, sessions).
- **Sync folder**: its files are read as plain files only (a link, a pipe
  or a device in their place is refused) and the folders Sanaga keeps in it
  must be real folders, held open for the whole round; saved requests travel only through a folder
  encrypted with a secret phrase — in a plain folder anyone who can write
  in it could slip one in.

### Fixed
- Clearing the history no longer opens the conversations' database on the
  window's thread when the assistant had not opened it yet.
- A Sanaga packaged by a distribution (`SANAGA_DISTRO`) no longer offers to
  look for new versions on GitHub: they come with the system's updates.
- The encrypted DNS relay serves at most 256 connections at once; more wait
  a moment, then are refused. A connection that has not given the relay's
  secret yet has one of 32 places of its own and 10 seconds to give it.
- A message longer than 4 MB from an outside tools' server ends the call
  cleanly instead of being read whole into memory.

## [0.6.0] — 2026-11-17

What the comparison with the Dia browser asked for
([docs/DIA-AUDIT-2026-11-17.md](docs/DIA-AUDIT-2026-11-17.md)): the ✦
button was a summary, it is now an assistant.

### Added
- **The assistant**: a conversation at the side of the page (Ctrl+E, the ✦
  button, the page's menu on selected words). A question about the page
  shown, follow-up questions, Stop, the last question taken back and asked
  differently. The page's text goes once, with the question it is attached
  to; it now holds the page as shown, its open shadow trees and the frames
  of the same site. A summary is such a conversation's first question.
- **More than one page**: other tabs attached with "@" or "+ Attach" (a tab
  asleep is loaded first), text files, pictures and PDFs (a PDF shown in a
  tab is taken from the engine; only Claude reads PDFs). What a model
  cannot read is said before anything is sent.
- **Saved requests**: `/name` in the assistant's field — four provided
  (summarise, explain, compare, to-do), the person's own as text files in
  the profile's `skills` folder, managed on `sanaga://skills`, found by the
  palette, carried by the sync folder.
- **Tools the model is given by Sanaga, and by nothing else.** Reading: the
  history by words and by days, the open tabs, a tab's page, favourites,
  earlier conversations — free for a model on this computer, asked about
  once per conversation for a model outside it. Acting: open an address,
  close or group tabs, run a command, fill a form's fields — each use
  shown with exactly what it would do, and waiting for the person, every
  time. Password, card and hidden fields are never listed nor filled, and
  a form is never sent.
- **Outside tools**: the person's own MCP servers (a program, or an
  address with an optional token kept in the keyring), on `sanaga://tools`.
  Every call is shown with what is sent, unless that tool was let free.
- **Conversations kept on this computer** (`chats.db`): reopened and gone
  on with, searched, deleted, cleared with the history — never those
  holding something of a private tab.
- **Notes**: an answer kept as a text file in the profile's `notes` folder,
  read on `sanaga://notes`, gone on with, or opened as a page to print.
- A question typed in the address bar can be asked of the assistant (one
  more line under the bar; Shift+Enter).
- For organisations: `/etc/sanaga/policy.txt` switches AI off (`ai=off`) or
  keeps given sites from every model (`ai_no_sites=…`).
- The About page, and a message once per version, say when the system's
  WebKitGTK belongs to a series that no longer gets security fixes.
- "Copy the link as Markdown" among the commands.
- For tests: `tools/fake-model.py` answers conversations, calls tools, and
  stands in for Claude's Messages API (it refuses what the service refuses
  of a conversation's history); `tools/fake-mcp.py` is a stand-in MCP
  server; `tests/assistant.html`, `tests/form.html`.

### Changed
- Once something of a private tab is in a conversation, only a model on
  this computer answers it.
- The summary's long text is cut to what the model can take and says so,
  to the model and to the person (it was cut at 24,000 characters,
  silently).

### Not done (said in the audit, section "What was done")
- Video subtitles as context; dictation; the sign-in windows (OAuth) some
  MCP services need; passkeys (WebKitGTK gives pages no WebAuthn);
  changeable shortcuts, groups inside groups, links of a site always to a
  container, a remembered split layout; the Windows and macOS ports.

## [0.5.0] — 2026-11-17

### Changed
- Keyboard: the place where the keyboard is shows everywhere — tabs,
  the star, favourites, the side panel's rows, the new tab's tiles, the
  menus; the empty part of a bar no longer takes the Tab key.
- Screen readers: tabs are tabs (the one shown is said), menu lines are
  menu items, favourites and tiles are links named by their title, the
  address field has a name, Sanaga's short messages are announced, the
  interface's window says what it is, and a button that cannot be pressed
  now (Back with nowhere to go) is still a button. Checked through the
  accessibility bus and with Orca (`tools/a11y-tree.py`).
- Tab titles and group names stand out at 4.5:1 at least against what is
  behind them, in the light, dark and private looks (43 pairs were under;
  `tools/contrast.py` computes them).
- The small crosses and signs (a tab's close and sound, the favourites
  bar's and the reading list's cross, a group's colours) take the click
  24 points across.
- With "reduce animations" set on the system, tabs, the side panel and
  the border do not animate.
- Sanaga's own pages no longer read the disk at every frame (Settings,
  Extensions, Profiles, Import, the reports: up to 4,500 file calls in
  five seconds of moving the mouse, none now); a tab loading behind no
  longer redraws the window at the screen's rate; the animated border
  rests when the window is not the one in use.
- Site icons are right in every window (a second window showed blank
  ones), and their memory is bounded.
- Long lists (favourites, passwords, devices) lay out only the rows in
  view: 3,000 favourites took 13 ms a frame, 0.5 ms now. A page's source
  with thousands of scripts no longer freezes the window.
- The history is kept in a database (`history.db` in the profile's
  folder) instead of a text file read whole into memory. It has no limit
  any more (it was cut at 5,000 visits, and an import could push your own
  visits out); the history file of before is taken in once, whole, and
  kept as `history.tsv.imported`. Clearing the history clears that copy
  too.
- Address bar: suggestions are ranked — sites whose name starts with what
  you type first, then the most visited lately — and looked up when the
  text changes, off the window's thread (they were recomputed over the
  whole history at every frame). A site's name is completed to where you
  really visited it (`http://nas.lan:8080`, it was always `https://name`).
- History page: every visit can be reached and searched (it stopped at 400
  lines); End and Home go to the oldest and the newest visit.
- "Frequently visited" counts the visits of a whole site (each address
  was counted apart).
- A page visited in one window shows in the others' history without
  their whole profile being read again.
- Import from another browser takes every visit (it took the last visit
  of 5,000 pages), so imported pages are ranked by how often you went.

### Added
- Downloads: they go to the folder your desktop names (Téléchargements,
  Descargas... wherever it is) or to one chosen in Settings › General,
  and Sanaga can ask where to save each file.
- A proxy setting (Settings › General › Network): the system's, none, or
  an address (`http://host:3128`, `socks5://host:1080`). Pages and
  Sanaga's own requests (block lists, translation, updates) all go
  through it; this computer itself is always reached directly.
- HTTPS first (Settings › Privacy, on): an `http://` address you open —
  typed, a favourite, a link clicked — is asked over https; over http
  only when the site did not answer there (refused, a certificate that is
  not the site's, nothing after five seconds). A name typed without its
  scheme falls back the same way; an address written with `https://`
  never does. An address a page goes to by itself (a script, a form, a
  page that sends on) is treated the same once its answer comes: what
  came over http is not shown, https is asked.
- Encrypted DNS (Settings › Privacy, off unless you choose a resolver:
  Quad9, Cloudflare, Mullvad or an address): the names of the sites you
  visit are asked over https instead of being sent in clear to the
  network's DNS. Pages go through a small relay Sanaga runs on this
  computer (the web engine has no setting for it), Sanaga's own requests
  use the same resolver. When the resolver does not answer, nothing is
  asked in clear instead. Not used together with a proxy.
- A warning before a site reported as phishing or malware (Settings ›
  Privacy, on). The lists (malware-filter's phishing-filter and
  urlhaus-filter) are fetched once a day and looked up on this computer:
  no address is sent anywhere. The warning has a way back and, under
  "Advanced", a way on.
- A link dragged from a page, a file from the file manager or words can
  be dropped on the tab bar, the toolbar or the side panel: they open in
  new tabs (words are searched for). A tab dragged out of the tab bar
  gets a window of its own.
- The keyboard's play and pause keys pause or start what plays, from a
  page or from the interface. (A desktop that keeps these keys for itself
  uses the media player the engine publishes for each page that plays —
  with next and previous.)
- Sanaga can look for a new version once a day (Settings › General, off
  unless you ask): one request, nothing installed.
- The address of the link under the mouse shows at the bottom left of a
  web page.
- A page's full-screen button (a video) fills the screen: Sanaga's bars
  go while it lasts.
- The bar under the address field shows how far the page has come.
- Downloads under way are listed with what was received, the announced
  size and a bar, and can be cancelled.
- Ctrl+1…8 show that tab, Ctrl+9 the last one; Ctrl+F5 and Shift+F5 load
  again without the cache; F3 and Ctrl+G go to the next match; Alt+D goes
  to the address field.
- A link opened in a tab behind (middle click) loads at once.
- A tab keeps its back and forward pages across a restart, and when it
  is reopened after being closed.
- Closing a tab in which something was typed asks the page first (its
  "leave this page?").
- A tab that uses the camera, the microphone or the screen shows a red
  camera; a click stops it. Sharing the screen is asked about instead of
  being refused.
- About › "Look for a new version": asks GitHub for the latest release,
  only when clicked.
- Find in page says which match is shown ("3/8"), not only how many
  there are.
- A web page's right-click menu, with words selected: **Search the web
  for the selection** and **Copy link to this passage**.
- Settings: a category arrives with a short fade (not when the system
  asks for no animations).
- The keyboard alone: Tab reaches every row, switch and choice of Sanaga's
  own pages and the toolbar, with a focus ring; Space or Enter acts.
- Summaries and extensions appear as the model writes them, with **Stop**.
  The summary draws headings, lists and emphasis, in a panel resized by
  its edge.
- A page in another language offers its translation in a bar (a switch in
  Settings; "never on this site").
- Settings: **Restore the defaults** per category; the search also finds
  what lives one page further.
- An extension can be tried on the page in front of you before it is
  switched on.
- Find in page uses the engine's own search on web pages: every match is
  marked, their number is shown ("No results" in red), the words are kept
  for next time.
- A favourite's right-click menu: **Edit…** and **Copy address**.
- About shows the WebKitGTK version and links to the Activity page; the
  Extensions page links to the extension writer.
- Each Settings category has an address (`sanaga://settings/privacy`): the
  toolbar's arrows, Alt+← / → and the mouse's side buttons go back and
  forth between categories, a category can be opened directly, and the
  session restores it.
- Translating a page with an AI shows a bar while it works (what, with
  which model, how far, **Stop**) and afterwards **Show original**, which
  puts the page's own words back at once.
- Writing an extension: the seconds while the model works, **Discard**,
  and **Write it again** (in place of the draft, not next to it).
- `tools/fake-model.py`: a stand-in model to try the intelligence features.
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
- `MECHANICS.md` and `mechanics.json` are removed from the tree (see the
  audit of 16 November); the values stay in the code, described for what
  they do.
- Right-click menus (tab, group, tab bar, favourite, container, links):
  a symbol on each line, the shortcut at the right, sub-menus alike.
- A web page's right-click menu is in Sanaga's language throughout (the
  engine's lines came in the system's, or in English); the inspector's
  line goes last, "Stop" shows only while a page loads.
- Windows of a profile share their history as it happens.
- What can't be undone asks twice: clearing the history, deleting a
  password or an extension, resetting a site's settings.
- One panel at a time: opening a menu or a panel closes the others, and
  Escape closes whichever is open.
- A site's settings panel and the Extensions page use the Settings' cards,
  rows and switches.
- A file of this computer is shown by its name, then its folder, in the
  address bar, the tabs and the lists.
- Settings (interface audit, docs/UX-AUDIT-2026-11-11.md): a row's text is
  never cut any more (it wraps, the row grows); the search finds every
  setting shown — also by a menu's choices, without accents — says which
  category each result is in and tints the rows that match; things done on
  the spot have a button, not a chevron; Ctrl+F searches the settings.
- The ✦ menu shows its keys on the right; "Translate" and "Copy" have
  their own icons (they were envelopes).
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

### Fixed (issues #4 and #5, and the audit of 16 November 2026)
- **A page no longer shrinks when a menu opens (issue #4).** On a screen
  with a scale of 2, the picture of the page shown under a menu was half
  its size, in the top left corner. Every menu of 0.4.0 did this; since
  menus open over the living page, the mobile simulator still did.
- **"Sanaga ended without closing" says how (issue #5).** The report
  kept after an abrupt end names the signal that ended Sanaga (a fault in
  Sanaga or a system library, a stack that ran out), or says that the
  computer was started again or the desktop session ended with Sanaga
  open, and when the lost run had started.
- **Shortcuts are read from the keyboard, not by a script in the page.**
  Ctrl+A, B, I, Y… work again in text fields (select all, bold, italic,
  redo); a web application that handles Ctrl+S, K, F… keeps it; Sanaga's
  shortcuts work in a frame of another site, on a page that no longer
  answers, and on a keyboard set to Russian, Greek, Arabic or Hebrew
  (AZERTY: Ctrl+0 resets the zoom).
- **Saved passwords: the list shows every account.** It was read from
  the wrong output of the keyring tool and fell to a single account at
  every start, the others no longer offered (they stayed in the keyring).
- **Favourites are not replaced by the first ones when their file reads
  badly** (one damaged byte was enough). A file that cannot be read is
  left alone; the state before each run stays as `<file>.bak`; two
  Sanagas on one profile can no longer put a half-written file in place.
- **Sync:** changing the folder no longer removes this computer's
  favourites; an encrypted folder refuses a plain file put there; a file
  that cannot be read is not taken for an empty one; a round starts when
  the folder's path is finished, not at each letter typed; the windows
  no longer read the whole profile again every 30 seconds.
- **The certificate error page's buttons work** ("Back to safety",
  "Continue"): a site with an invalid certificate could not be opened.
- **The padlock follows the certificate** of the page shown: none on an
  error page, red and struck out after going on despite an invalid
  certificate. It was drawn for any address starting with https://.
- A site's settings (JavaScript off, video formats) apply when the site
  is reached by a link, not only when its address is typed.
- A guest window stays a guest: a window opened from it used the main
  profile.
- Private tabs: zoom, site settings, permission answers and download
  marks are no longer written; the page is not sent to an online model
  or translation service; the private session ends with its last tab.
- Clear browsing data clears this window's data and every container's
  (it cleared whatever the first tab used).
- Closing one of several windows ends its pages (they went on running,
  sound included, until Sanaga quit).
- The interface no longer draws faster and faster while a page loads
  (past 2000 frames a second after a quarter of a minute).
- A tab showing one of Sanaga's pages or the Reader ends its web page
  (it went on playing under it).
- The history no longer lists loads that failed or downloads; a page
  without a title no longer takes the title of the page before it.
- The energy saver ignores the battery of a wireless mouse or keyboard.
- Thousands of brackets pasted in the address bar no longer end Sanaga.
- Editing a favourite after removing another no longer overwrites the
  wrong one; the star's bubble no longer renames another tab's
  favourite; a password shown stays on its own account's line and hides
  after half a minute.
- A protected site restored at the start no longer stops Sanaga (and
  truncates the saved tabs) when "always open this way" is on.
- Enter typed in the address field no longer answers a page's question.
- Containers "A B" and "A-B" no longer share (and erase) their data;
  deleting a profile removes its secrets from the keyring; "Close this
  profile" quits it.
- Vertical tabs can be dragged downwards; a button of the person's that
  closes the tab no longer raises an error; a video recording stopped at
  once no longer waits for ever; the simulator keeps the device's width
  at any zoom; a page's source keeps working after its tab slept.
- Links to the mail program, the phone or a torrent program (`mailto:`,
  `tel:`, `sms:`, `magnet:`) are handed to the system instead of showing
  an error page; an address on a port browsers refuse says so.
- Translating a page with the AI works for every alphabet (a Korean,
  Hindi, Greek, Hebrew or Thai page had "nothing to translate" and was
  called translated) and no longer waits for ever after a long paragraph.
- Contact AutoFill only fills fields the person can see (a page showing
  a "name" field got the e-mail, phone and address in hidden ones).
- An assistant run on the command line can no longer leave Sanaga and
  itself waiting on each other with a long page.
- A window opened by a page (sign-in, payment) no longer takes that page
  down when it closes.
- One Sanaga at a time writes a profile: two launches at the same moment,
  or a session without a runtime folder, no longer give two Sanagas on
  the same files.
- Sanaga closes while a page's question (confirm, alert) is waiting; it
  could stay there for more than half a minute.
- Clear browsing data removes cookies for a period too (the engine
  removes none "since a time": only "all time" did).
- Hyprland: with several windows, the green button, F11 and moving by
  the bar act on the window in use.
- Chrome passwords of 32 characters or more are imported whole; user
  scripts read their whole header and run on their own sites only
  (`*://*.bank.example/*` no longer matches another site's address);
  installing an unpacked extension reads ordinary files only, with
  limits; the Light engine never reads a local file named by a page from
  the network; a download's name is read properly from its header.
- Text typed with an input method (Chinese, Japanese, Korean, dead keys)
  shows in the field while it is being composed; it appeared only once
  finished.
- On Wayland the first frames come before the window has its size: they
  are no longer laid out in a window one pixel wide (a debug build
  reported errors of its own there, and three in a row stopped it).
- The history's file could not be opened: said in the window, once (it
  was only printed on the terminal).
- "Reduce animations" is followed by the switches, the tab overview and
  the menus that unfold; the window's three buttons take the click 20 by
  24; the site icons kept on disk are bounded (the 3,000 used last); the
  side panel shows the first 200 favourites instead of laying out all of
  them at every frame.
- A guest window reaches the network the way its owner set it: it went
  round the proxy of the settings.
- The proxy and the other network settings count from the very first
  request: one made during the first frames (the daily look for a new
  version) could leave before they were told.
- The thread that listens for other launches of Sanaga sleeps until one
  connects: it woke twenty times a second for as long as Sanaga was open.
- Packages: the `.deb`, the `.rpm` and the PKGBUILD ask for the versions
  of GTK (4.14) and WebKitGTK (2.48) the code needs; the description file
  names the launcher the packages install; `install.sh` copes with a
  folder whose name has spaces or signs and says when the sandbox's two
  programs are missing; the launcher's actions are in five languages
  ("Open with the Travail profile" gives way to "Choose a profile").
- Right-to-left sentences (Arabic, Hebrew...) read right in the interface:
  tab titles, favourites, the history, suggestions, the tab overview. The
  words came one after the other from left to right; a title too long is
  now cut where reading stops, its "…" on the left, as pages do. The
  Reader and the Light engine set such paragraphs against the right, line
  by line, with their links and numbers in place.
- Arabic (and the other scripts whose letters join or stack) keeps the
  system's font that knows how to shape it, whatever was shown before: a
  catch-all font found earlier for another script (GNU Unifont) drew it as
  loose letters.
- Titles and names in other alphabets (Arabic, Hebrew, Devanagari, Chinese,
  Cherokee...) show in the interface with whatever font the system has for
  them (asked to fontconfig when first needed); only a fixed list of Noto
  files in a fixed list of folders was looked at, so they often showed as
  empty boxes. Colour emoji are found the same way.

### Fixed
- History: a title that arrived after its page had loaded was never
  written to the file (the visit showed its bare address after a
  restart), and could add a second visit of the same page.
- Text painted by Sanaga that doesn't fit ends with "…" instead of being
  cut in the middle of a letter (tabs, favourites, lists, menus, the
  address bar).
- Site badges: the letter of the site's own name ("W" for
  fr.wikipedia.org, not "F").
- A band of the wrong colour across the last row of scrolling lists.
- An empty box in place of "⋮" in the welcome page, and in place of
  language names written in their own script on a computer without those
  fonts. A test now fails if a text uses a character the carried fonts
  cannot draw.
- The address bar no longer keeps the caret on Sanaga's own pages; a menu
  label no longer runs into its shortcut; the names under the new tab's
  tiles no longer run into one another.
- Settings: missing characters (▯) in help texts; a switch that is off was
  invisible in the dark look; the memory saver's note contradicted its
  row; the codecs card's buttons ran into its text; the sidebar cut its
  longest name in German; rows and switches said nothing to screen readers.
- Translation: choosing "an AI" with none set up was kept and failed at
  every later try; the question stayed over other pages and tabs.
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
