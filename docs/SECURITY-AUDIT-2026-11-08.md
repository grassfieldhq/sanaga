# Security audit of 8 November 2026 — findings and fixes

Sanaga is to become the default browser of the Grassfield desktop once a
security audit passes. The first audit (8 November 2026, against commit
`44247bd`) **failed** with 15 findings. This page lists each finding, what was
changed, and the commit that changed it, for the re-audit.

Status legend: **Fixed** · **Fixed, with a deviation** (explained) ·
**Partly fixed** (what is left is said).

> **Verification status.** The fixes were written on a machine without
> WebKitGTK 6 (see "Build and test status" below). The whole crate, tests
> included, passes `cargo clippy --all-targets -- -D warnings` with the WebKit
> pkg-config lookups stubbed (type-checking only, nothing linked); the pure
> rules (`src/guard.rs`, `src/codecs.rs`, commit `ef63152`) were built and
> their 18 tests run on their own (`tools/guard-tests`). The browser itself
> was **not built or run**: every behaviour below must still be checked live
> with WebKitGTK 6 before release (checklist at the end).

## Blockers

### 1. Downloads could land outside the Downloads folder — **Fixed**

*Was:* the name was the last URL segment after percent-decoding
(`src/net.rs:68-71`), so an encoded `/` let a page choose the folder
(autostart, applications); downloads started with no prompt
(`page.rs:389-393`).

*Now* (`1b03d7c`, `8350c77`):
- WebKit's suggested file name (`decide-destination`: Content-Disposition,
  else the address) is used; `guard::safe_download_name` percent-decodes it
  and keeps only the part after the last `/` or `\`, drops control and
  direction-changing characters, trims leading/trailing dots and spaces (no
  hidden file, no `..`), caps the length, and makes `.desktop`, `.service` and
  the other systemd unit types inert by adding `.download`.
- A clash gets ` (2)`, ` (3)`…; anything already there counts (even a
  dangling link), and names picked for downloads in progress are reserved.
- WebKit is told never to overwrite (`set_allow_overwrite(false)`: it creates
  the file exclusively and fails rather than replace one). Downloads Sanaga
  writes itself (light engine) use `O_CREAT|O_EXCL|O_NOFOLLOW`.
- Safari's model: a download goes straight to Downloads, without a dialog,
  when the person just acted on the page (a click that navigates, a new
  window opened by a click, an address opened from Sanaga's interface) and no
  other download used that action. Otherwise (no user gesture, or several
  downloads at once) the download waits (asynchronous `decide-destination`,
  WebKit 2.40+) behind a bar *"Do you want to allow downloads on <site>?"*;
  allowed, the waiting downloads go on and the site may download until Sanaga
  quits; refused, they are cancelled and not listed.
- Unit tests: `guard::tests::download_names_*`, `downloads_never_replace_a_file`,
  `downloads_follow_safaris_rule`, `download_question_names_the_site`.

### 2. No download mark — **Fixed**

*Now* (`3605ac5`): when a download finishes, `guard::mark_download` opens the
file (`O_NOFOLLOW`, regular files only) and sets with `fsetxattr`
`user.xdg.origin.url` (the address after redirections, user name, password and
`#fragment` removed; `data:` keeps only its type; capped at 2 KiB) and
`user.xdg.referrer.url` (the page that started it). `libc` (already in the
build) is now a direct dependency. Test: `downloaded_files_carry_their_origin`
(reads the attributes back), `download_marks_hide_passwords`.

## High

### 3. The page-to-browser channel trusted everyone — **Fixed, with a deviation**

*Was:* the script message handler and all of Sanaga's scripts ran in the
page's own world (`page.rs:283-307`, `web.rs:193-208`).

*Now* (`23c488a`, `aab0589`):
- Sanaga's scripts are `UserScript::for_world(…, "sanaga")`, top frame only;
  the `ipc` handler is registered for that world only
  (`register_script_message_handler("ipc", Some("sanaga"))`). The person's
  extensions stay in the page world, without the channel. Sanaga's own
  evaluations (tools, summary, translation, passwords, find) run in that world
  (`PageView::evaluate_own`).
- Every message carries the sender's `location.href`; the Rust side drops it
  unless the view still shows that page (`guard::message_body`), so a page that
  has navigated since is ignored.
- Commands that change state need an action in Sanaga's interface:
  - forwarded shortcuts (quit, new window, summary…) must follow a real key
    press that GTK saw on the view within 1.5 s, and the page script ignores
    untrusted key events;
  - page messages can only answer what the person started: the element
    remover's selector (`zap-done`, armed per tab from Sanaga's menu, checked
    by `guard::plausible_selector` before it reaches the site's CSS), the
    floating-video info (armed per tab), one summary text per request, a
    translation batch while a translation the person started runs,
    view-source of http(s) links on Sanaga's own source page;
  - the other tools come only from Sanaga's menus (WebKit context-menu
    actions created by Sanaga, never a page message), and a page's answer
    never switches tabs;
  - the codec bar may appear on a media error (only when decoders really are
    missing) but the install starts only from Sanaga's button (finding 7);
    AI features start only from Sanaga's interface (finding 6).
- *Deviation:* WebKitGTK's `script-message-received` gives no frame
  information, so the frame cannot be read on the Rust side. The frame is
  guaranteed instead by construction: the channel exists only in Sanaga's
  world, and that world's scripts run only in the top frame (no page or
  iframe code can enter it); `IPC_JS` also refuses to install itself outside
  the top frame. The URI check above is done on the Rust side for every
  message.
- Tests: `messages_from_a_page_that_navigated_are_dropped`,
  `shortcut_messages_are_recognised`, `element_remover_selectors_stay_selectors`.

### 4. Passwords filled with no click — **Fixed**

*Now* (`d00c702`):
- Nothing is filled on page load. When sign-in fields appear, the page script
  (Sanaga's world) only tells Sanaga, which offers the saved accounts in its
  own bar; the fill happens when the person clicks *"Fill: <account>"* there.
- The in-page account list (usernames in the page's DOM) and the page's
  `pw:fill` request are gone.
- The fill runs in Sanaga's world; Rust first checks the tab still shows that
  origin, and the script compares `location.origin` with the expected origin
  at the moment of filling and refuses outside the top frame. Cross-origin
  iframes are never filled (the script never enters frames).

### 5. Pages could read the clipboard silently — **Fixed**

*Now* (`a153e47`): `javascript-can-access-clipboard` is no longer set; WebKit's
default applies (a read needs a user gesture).

### 6. AI translation — **Fixed**

*Now* (`744e1f2`):
- Page text (translation and summaries) only goes to models with no tools and
  no file access: Claude (API), Ollama, and Claude Code started with its
  documented switches: `--tools ""` (no built-in tool), `--disallowedTools "*"`
  (every tool, MCP ones included), `--strict-mcp-config` (no MCP server),
  `--safe-mode` (no CLAUDE.md, hooks, plugins or skills),
  `--disable-slash-commands`, `--no-session-persistence`, in an empty private
  folder. Codex (`-s read-only`) and Gemini CLI (`--approval-mode default`)
  can still read local files and have no documented tool-less mode: they are
  refused for page content, and Settings and the error message say so (they
  may still write an extension from the person's own description, which is
  reviewed before use: finding 9).
- The answer goes into the page as text only (a text node's `data`, never
  HTML), from Sanaga's world.
- Batches are checked (at most 80 texts, 6000 characters) and capped at 40 per
  page.
- Translation is reset when the tab navigates; answers for a page the tab has
  left are dropped (a generation number).
- Tests: `page_batches_are_bounded`, `page_text_goes_only_to_tool_less_models`,
  `only_tool_less_assistants_read_pages`.

## Medium

### 7. Codec install — **Fixed**

*Now* (`fb0ae70`, `79509a2`):
- `SANAGA_DISTRO` (at build time, or in the environment;
  `SANAGA_DISTRO=grassfield` for the Grassfield package) turns the install off
  entirely: no button, the bar only says the decoders come with the system.
- Elsewhere, the command shown is the command run (same argv, no
  `--noconfirm`/`-y`/`--non-interactive`); *"Install in a Terminal"* opens it in
  the desktop's terminal (`xdg-terminal-exec`), where the package manager asks
  for confirmation and the password; the words of the command reach the
  terminal's shell as arguments (`"$@"`), never as text to re-read. The
  privileged helper Sanaga used to run without a terminal is gone.
- Only Sanaga's own buttons start it.
- Tests: `the_command_shown_is_the_command_run`,
  `a_distribution_that_ships_codecs_turns_the_install_off`.

### 8. Favicons fetched outside WebKit — **Fixed**

*Now* (`060a7af`): Sanaga makes no request of its own for icons. WebKit loads
a page's icon with the page, through that page's network session (favicons
enabled on every session: profile, containers, private), and hands it over
(`notify::favicon`). So: no shared cookie jar (each tab's own session), no
fetch to LAN/localhost beyond what the page itself loads, no on-disk cache for
private tabs (their icons stay in memory), no contact with the default
favourites at first start (a site shows its letter badge until visited).

### 9. AI-written extensions switched on before review — **Fixed**

*Now* (`b2ac276`): the extension is saved switched off; the page shows where
it would act (`@match`/`@include`, or all sites), what that allows, and its
code (opened); it is switched on only by the person's click on *"Turn On
Extension"* (or in Extensions).

### 10. Predictable /tmp paths — **Fixed**

*Now* (`cc315de`): AI work folders and browser-import copies use
`guard::PrivateDir`: a new folder with a random name created atomically with
mode 0700 (mkdtemp semantics) in `$XDG_RUNTIME_DIR` when it is the user's own
closed folder, else in the temp folder; removed when done.

### 11. Page URL unescaped in a .desktop Exec line — **Fixed**

*Now* (`c62148e`): `guard::exec_arg` quotes each argument, escapes `"`,
backtick, `$` and `\`, doubles `%`, drops control characters, then applies the
key-file string escaping; `Name`, `Comment` and `Icon` go through
`guard::desktop_value`. Web apps are made for http(s) pages only; the profile
launcher uses the same helpers. Test `hostile_addresses_stay_one_exec_argument`
reads a hostile address back, as a desktop would, as one argument.

## Low

### 12. Single-instance socket — **Fixed, with a deviation**

*Now* (`a955179`): the socket lives in `$XDG_RUNTIME_DIR` only (checked: owned
by the user, mode 0700; without it each launch runs on its own), mode 0600; the
peer must be the same user (`SO_PEERCRED`; `getpeereid` on the BSDs); messages
are at most 64 KiB (2 s read timeout) and 64 lines, at most 20 per 10 s.
`guard::accept_forwarded` lets through http(s) addresses, text without a
scheme (it becomes an https address or a search), and Sanaga's own flags;
`javascript:`, `data:`, `sanaga://`, `view-source:` and the rest are dropped.
*Deviation:* `file:` addresses are also accepted, so that local pages and PDFs
opened from the file manager reach the running default browser; the peer is
already verified to be the same user, who can open those files anyway.
Tests: `other_launches_hand_over_web_addresses_only`,
`other_launches_are_rate_limited`.

### 13. Test hooks in the release build — **Fixed**

*Now* (`ed220c3`): `SANAGA_LIVE_TEST`, `SANAGA_BENCH_CYCLE`,
`SANAGA_TEST_POPUPS`, `SANAGA_COOKIES` and `SANAGA_HWACCEL` (and the page
`probe` tool) are compiled only under `cfg(debug_assertions)`. The
`SANAGA_DEBUG_*` switches only print diagnostics and stay.

### 14. Default file permissions — **Fixed**

*Now* (`ab86632`): Sanaga's data folder, each profile folder and each WebKit
data folder (cookies, site data, containers) are created 0700 and tightened if
open; `write_atomic` writes a new 0600 copy (`O_EXCL`) before swapping it in;
`history.tsv` and `crash.log` are created 0600; files left readable by earlier
versions are closed when a profile or WebKit folder opens. Test:
`data_files_and_folders_are_closed_to_others`.

### 15. Built-in certificate list — **Fixed**

*Now* (`8221a55`): every ureq agent gets a rustls configuration built from the
system bundle (`$SSL_CERT_FILE`, then the usual distribution paths); with no
bundle those requests fail rather than fall back. The favicon fetch is gone
(finding 8). `rustls` is a direct dependency with the same version and
features ureq already builds (no new crate). Test:
`requests_trust_the_systems_certificates`.

## Build and test status

| Check | Result |
|---|---|
| `cargo fetch --locked` (Cargo.lock consistent with Cargo.toml) | passes |
| `cargo clippy --locked --all-targets -- -D warnings` (debug, WebKit pkg-config stubbed: `SYSTEM_DEPS_WEBKITGTK_6_0_NO_PKG_CONFIG=1`, same for `JAVASCRIPTCOREGTK_6_0`) | passes |
| `cargo clippy --locked --bin sanaga -- -D warnings` with `debug-assertions` off (the release `cfg` paths: test hooks compiled out) | passes |
| `cargo test --manifest-path tools/guard-tests/Cargo.toml` | 18 passed |
| `cargo build`, `cargo test` of the whole crate | **not run**: WebKitGTK 6 is not installed on the machine used; the other new tests (translation batches, tool-less AI, file permissions, system certificates, extension matches) were type-checked but not executed |
| `python3 tools/i18n.py check` | all texts translated |

## Not touched

The mobile simulator's device names, frames and browser identity are a
separate question for the owner and were left as they are.

## To check live before release (WebKitGTK 6)

1. A link with `download`, a `Content-Disposition: attachment; filename="../x.desktop"`
   response, a `%2F` name: file lands in Downloads as `x.desktop.download`,
   with `getfattr -d` showing the two `user.xdg.*` attributes.
2. A page calling `a.click()` on a download link from a timer, or starting two
   downloads at once: the bar asks; refusing cancels.
3. A page calling `window.webkit.messageHandlers.ipc.postMessage(…)` or
   defining `window.__sanagaFill`: nothing happens.
4. A sign-in page with one saved account: nothing filled until "Fill" in the
   bar; an iframe sign-in form is never filled.
5. `navigator.clipboard.readText()` without a click: refused.
6. Translation with Claude Code (works), with Codex (refused with the message);
   navigating mid-translation stops it.
7. Codec bar with `SANAGA_DISTRO=grassfield`: no install button.
8. Favicons in normal, private and container tabs; no request to the default
   favourites at first start.
9. "Create an extension": written switched off.
10. A second `sanaga https://example.org` while Sanaga runs: a new tab;
    `sanaga 'javascript:alert(1)'`: nothing.
11. `stat` on `~/.local/share/sanaga` (0700) and its files (0600).
