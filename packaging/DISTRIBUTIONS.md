# Packaging Sanaga for a distribution

`packaging/install.sh` installs Sanaga for one user. A distribution package
(for example Grassfield's `grassfield-desktop`, which makes Sanaga the default
browser) builds it as below.

## Build

```sh
SANAGA_DISTRO=grassfield cargo build --release --locked
```

- `--locked`: the versions in `Cargo.lock` only.
- `--release`: a release build. The test aids driven by environment variables
  (`SANAGA_LIVE_TEST`, `SANAGA_TEST_POPUPS`, `SANAGA_COOKIES`…) exist only in
  debug builds; never ship a debug build.
- `SANAGA_DISTRO=<name>`: the distribution ships the video codecs with
  Sanaga's package, so Sanaga **never offers to install them** (no button, no
  command; its bar only says they come with the system). Any non-empty value
  works; Grassfield uses `SANAGA_DISTRO=grassfield`. The same variable set in
  the environment at run time has the same effect, but the build-time value
  is the one to rely on.

## Runtime dependencies

| Why | Arch package (Grassfield) | Debian / Ubuntu | Fedora |
|---|---|---|---|
| The window | `gtk4` | `libgtk-4-1` | `gtk4` |
| The web engine | `webkitgtk-6.0` | `libwebkitgtk-6.0-4` | `webkitgtk6.0` |
| The web engine's sandbox (each page in its own bubblewrap sandbox, with a filtered D-Bus) | `bubblewrap`, `xdg-dbus-proxy` | `bubblewrap`, `xdg-dbus-proxy` | `bubblewrap`, `xdg-dbus-proxy` |
| Video and audio (H.264, AAC, AV1, Opus; VP9) | `gst-libav`, `gst-plugins-bad`, `gst-plugin-va`, `gst-plugins-good` | `gstreamer1.0-libav`, `gstreamer1.0-plugins-bad`, `gstreamer1.0-vaapi`, `gstreamer1.0-plugins-good` | `gstreamer1-plugin-libav`, `gstreamer1-plugins-bad-free`, `gstreamer1-plugins-good` |
| Passwords in the system keyring | `libsecret` (`secret-tool`) | `libsecret-tools` | `libsecret` |
| Default browser, opening files | `xdg-utils` | `xdg-utils` | `xdg-utils` |
| *(optional)* video recording of the simulator | `ffmpeg` | `ffmpeg` | `ffmpeg` |

Without `bubblewrap` and `xdg-dbus-proxy`, WebKitGTK runs pages without its
sandbox: a distribution that makes Sanaga its default browser must depend on
both.

## Files

| File | Installed as |
|---|---|
| `target/release/sanaga` | `/usr/bin/sanaga` |
| `target/release/libsanaga_page_helper.so` | `/usr/lib/sanaga/libsanaga_page_helper.so` (Sanaga looks in `../lib/sanaga` and `../lib64/sanaga` from its program; loaded by WebKit's page processes, it tells Sanaga which process serves which tab — without it the Activity page lists processes without tab names) |
| `packaging/sanaga.desktop` | `/usr/share/applications/sanaga.desktop` |
| `packaging/sanaga.svg` | `/usr/share/icons/hicolor/scalable/apps/sanaga.svg` |
| `sanaga --export-icon FILE SIZE` (48, 64, 128, 256, 512) | `/usr/share/icons/hicolor/<size>x<size>/apps/sanaga.png` |

## Before making it the default browser

The security audit of 8 November 2026 and its fixes are in
[docs/SECURITY-AUDIT-2026-11-08.md](../docs/SECURITY-AUDIT-2026-11-08.md).
Before switching the default, on a machine with WebKitGTK 6: `cargo build`,
`cargo test`, and the live checks listed at the end of that file, with their
results recorded.

## Publishing in the distributions' own channels

The release workflow builds and signs every package; putting them where
each distribution looks for software needs an account there, so it is done
by a maintainer — what is ready, and what is left to do each time:

- **AUR** (Arch, Manjaro, EndeavourOS): the release's files include a
  `PKGBUILD` with the checksum of the tag's source in place of `SKIP`
  (`updpkgsums` does the same by hand). Copy it into the AUR repository
  `sanaga`, run `makepkg --printsrcinfo > .SRCINFO`, commit and push.
- **Flathub**: Flathub builds from its own repository
  (`flathub/site.grassfield.Sanaga`). Its manifest is this one with two
  changes: the `sanaga` module's source is the tag (`type: git`, `tag:` and
  `commit:`) instead of `type: dir`, and `cargo-sources.json` (made by
  `flatpak-cargo-generator.py` from `Cargo.lock`, as the release workflow
  does) is committed next to it. The description file needs a `<release>`
  entry for each version. Submit with a pull request to `flathub/flathub`.
- **Debian / Ubuntu and Fedora**: the `.deb` and `.rpm` of a release are
  complete packages (their dependencies and the oldest versions of GTK and
  WebKitGTK they need are declared). A repository that `apt` or `dnf`
  follows (a PPA, a COPR, or a folder served with its signing key) needs a
  key that only the maintainers hold: none is set up here.

