# Releasing Sanaga

GitHub builds the packages itself (`.github/workflows/release.yml`); nothing
has to be built on your computer.

| You do | GitHub builds | Published as |
|---|---|---|
| push to `main` | every package | the release named after the version in `Cargo.toml`, e.g. **v0.1.0** (made the first time, its packages replaced at the next pushes) |
| push a tag `v1.2.3` | every package | the release **v1.2.3** |
| Actions › Release › *Run workflow* | every package | same as a push to `main` |

A new version number in `Cargo.toml` therefore gives a new release; the
earlier ones stay as they are.

The packages:

| File | For | Built on |
|---|---|---|
| `sanaga-X.flatpak` | every distribution (codecs inside) | GNOME 50 runtime + ffmpeg-full |
| `sanaga_X_amd64.deb` | Debian 13, Ubuntu 24.04+ and their family | Ubuntu 24.04 (`cargo deb`) |
| `sanaga-X.x86_64.rpm` | Fedora 43+ | Fedora 43 (`cargo generate-rpm`) |
| `sanaga-X-1-x86_64.pkg.tar.zst` | Arch and its family | Arch (`makepkg`, `packaging/arch/PKGBUILD`) |
| `sanaga-X-x86_64-linux.tar.gz` | the others (program + `install.sh`) | Ubuntu 24.04 |
| `SHA256SUMS` | checking the downloads | |

A build that fails (tests included) publishes nothing: the previous release
stays as it was.

## Making a release

1. Put the version in `Cargo.toml` (`version = "0.2.0"`) and in
   `packaging/arch/PKGBUILD` (`pkgver=0.2.0`); run `cargo build` once so
   `Cargo.lock` follows.
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [0.2.0] — 2026-11-09`
   and start a new empty `## [Unreleased]` above it. That part of the
   changelog becomes the release's notes.
3. Commit, then tag and push:

   ```sh
   git commit -am "Sanaga 0.2.0"
   git tag v0.2.0
   git push origin main --tags
   ```

4. Follow the build in the repository's **Actions** tab (about 20–30
   minutes, the Flatpak is the longest). The release appears in **Releases**.

The tag must match the version in `Cargo.toml` (`v0.2.0` ↔ `0.2.0`), otherwise
the build stops at once and says so.

## Later

- **Flathub**: the Flatpak manifest is ready for a submission to Flathub
  (users then install from their software centre and get updates).
- **AUR**: `packaging/arch/PKGBUILD` builds from the release tag; publish it
  as the `sanaga` AUR package after the first tagged release.
- **ARM (aarch64)**: the same jobs on GitHub's `ubuntu-24.04-arm` machines.
