# Packaging Sanaga — and why users don't install codecs themselves

Sanaga plays video through WebKitGTK, which uses GStreamer. The decoders
(H.264, AAC, AV1, Opus, VP9) are system libraries: they can't be put inside
Sanaga's program file, and H.264 / AAC are covered by patents, which is why
distributions ship them in separate packages. A **package** of Sanaga pulls
them in by itself, so the person installing Sanaga has nothing else to do:

| How Sanaga is installed | Codecs | Recipe |
|---|---|---|
| **Flatpak** (Flathub, every distribution) | bundled: GNOME runtime (WebKitGTK 6, GStreamer) + `org.freedesktop.Platform.ffmpeg-full` | `packaging/flatpak/site.grassfield.Sanaga.yml` |
| **Arch, Manjaro, EndeavourOS, Omarchy** (AUR) | dependencies: `gst-libav`, `gst-plugins-bad`, `gst-plugins-good` | `packaging/arch/PKGBUILD` |
| **Debian, Ubuntu** (`.deb`) | dependencies: `gstreamer1.0-libav`, `gstreamer1.0-plugins-bad`, `gstreamer1.0-plugins-good` | `[package.metadata.deb]` in `Cargo.toml` (`cargo deb`) |
| Built from source (`cargo build`, `packaging/install.sh`) | the person's system; `install.sh` says what's missing | — |

Whatever the way, Sanaga checks the decoders when it starts. If some are
missing (a build from source, a system without them), it shows a bar on video
sites with the exact install command, which it can open in a terminal (the
package manager asks for confirmation and the password there) — Settings ›
General › Video and music. A package that brings the codecs is built with
`SANAGA_DISTRO=<name>` (the Arch package: `arch`; the Fedora one: `fedora`;
Grassfield: `grassfield`) and then never offers to install anything; inside
the Flatpak it never asks either. Every package also depends on `bubblewrap`
and `xdg-dbus-proxy`, WebKit's page sandbox. See also
[packaging/DISTRIBUTIONS.md](../packaging/DISTRIBUTIONS.md).

**Netflix, Disney+, Prime Video, Spotify web** need Widevine, Google's DRM
module, which is only licensed to Chrome-based browsers and Firefox: no
package of a WebKitGTK browser can include it. Sanaga opens these sites in the
Spotify app or in an installed browser that has Widevine.

## Building the packages

GitHub builds all of them at every push and publishes them in Releases — see
[RELEASING.md](RELEASING.md). By hand:

```sh
# Arch (from packaging/arch/)
makepkg -si

# Debian / Ubuntu
cargo install cargo-deb && cargo deb

# Flatpak (generate the offline Rust sources first)
python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
flatpak-builder --user --install build packaging/flatpak/site.grassfield.Sanaga.yml
```

The Arch recipe was checked by building the package from this repository
(`makepkg`, without installing it): the package's dependency list includes the
codecs.

## Inside the Flatpak

Things that use desktop tools behave differently in the sandbox: Hyprland
window placement (keep on top, floating video corner) and opening links in
other installed browsers need host access (`flatpak-spawn --host`) that the
manifest does not grant yet.
