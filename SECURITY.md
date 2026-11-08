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
