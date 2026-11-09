
## Install

| Your system | Download | Install |
|---|---|---|
| **Any distribution** (openSUSE, Debian 12, Solus… — video codecs included) | `sanaga-*.flatpak` | `flatpak install --user sanaga-*.flatpak` |
| Debian 13, Ubuntu 24.04+, Mint, Pop!_OS… | `sanaga_*_amd64.deb` | `sudo apt install ./sanaga_*_amd64.deb` |
| Fedora 43+ | `sanaga-*.x86_64.rpm` | `sudo dnf install ./sanaga-*.x86_64.rpm` |
| Arch, Manjaro, EndeavourOS, Omarchy… | `sanaga-*.pkg.tar.zst` | `sudo pacman -U sanaga-*.pkg.tar.zst` |
| Others (GTK 4 and WebKitGTK 6 installed) | `sanaga-*-x86_64-linux.tar.gz` | unpack, then `./install.sh` (no admin rights) |

The packages bring everything Sanaga needs, the video codecs for YouTube & co.
included. `SHA256SUMS` lists the files' checksums.
