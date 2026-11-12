# Security Policy

Sanaga is a web browser: it shows pages from anywhere on the internet, keeps
passwords (in the system keyring) and site data. Security reports are taken
seriously.

## Supported versions

Only the latest release receives security fixes during the 0.x series.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report them privately through GitHub's private vulnerability reporting
(*Security → Report a vulnerability* in the repository), or contact a
maintainer directly. Please include:

- what an attacker can do (for example: read another site's data, run code,
  read saved passwords, escape the page);
- the steps or a page to reproduce it;
- your Sanaga version, distribution and WebKitGTK version.

We aim to acknowledge reports within a week and to keep you informed until the
issue is fixed. We are happy to credit you in the release notes if you wish.

## Scope

Sanaga relies on **WebKitGTK** to load and run web pages. Problems inside the
web engine itself should also be reported to the WebKitGTK project
(<https://webkitgtk.org/security.html>); keep your distribution's WebKitGTK
package up to date, as it brings the engine's security fixes.

Things that are in scope for Sanaga itself include: how Sanaga stores settings,
history and sessions; the password and AutoFill features; data imports from
other browsers; extensions handling; the scripts Sanaga adds to pages; and the
`sanaga://` internal pages.

## How Sanaga protects you (summary)

The last security audit and its fixes are in
[docs/SECURITY-AUDIT-2026-11-08.md](docs/SECURITY-AUDIT-2026-11-08.md).

- **Pages can't drive the browser.** Sanaga's own page scripts and its
  page-to-browser message channel live in an isolated JavaScript world, in the
  top frame only; a page or an iframe can neither see nor call them. Messages
  from a page that has navigated since are dropped, a forwarded keyboard
  shortcut must follow a real key press, and actions that change something
  (saving site settings, the floating video, the codec install, AI features)
  start only from Sanaga's own interface.
- **Passwords** stay in the system keyring and are filled only after you pick
  an account in Sanaga's bar, after the origin of the top-level page is checked
  again; frames are never filled.
- **Downloads** go to the Downloads folder under a safe name (a site can't
  choose another folder, never replaces a file, `.desktop`/`.service` files are
  made inert), are marked with where they came from (`user.xdg.origin.url`,
  `user.xdg.referrer.url`), and a site that starts a download by itself or
  several at once must be allowed first.
- **Crash reports** stay on the computer (`crashes/`, closed to other
  users) until the person sends one themselves; they hold where Sanaga
  stopped and the system's version, never a page's address, and the home
  folder's name is taken out.
- **Test software** can drive Sanaga only when it is started with
  `--automation`, in a window that keeps nothing and shares nothing with
  the profiles ([docs/AUTOMATION.md](docs/AUTOMATION.md)).
- **Page text and AI**: the text of a page is only sent to models run with no
  tools and no access to your files; their answer goes back into the page as
  plain text.
- **Your data**: Sanaga's folders are private (0700), its files 0600; temporary
  files go to private folders; the single-instance socket accepts only your
  own user and web addresses.
- **Network**: favicons come from WebKit with the page; Sanaga's own requests
  use the system's certificate authorities.
