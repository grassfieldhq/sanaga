#!/bin/sh
# Install Sanaga for the current user, without admin rights:
#   program      ~/.local/bin/sanaga
#   launcher     ~/.local/share/applications/sanaga.desktop
#   icons        ~/.local/share/icons/hicolor/<size>/apps/sanaga.png
#
#   packaging/install.sh              build (release) and install
#   packaging/install.sh --uninstall  remove those files (your data stays in ~/.local/share/sanaga)
#   PREFIX=/some/where packaging/install.sh   install somewhere else
set -eu
cd "$(dirname "$0")/.."
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/sanaga"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"
SIZES="48 64 128 256 512"

refresh() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$APPS" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN" "$APPS/sanaga.desktop"
    for s in $SIZES; do rm -f "$ICONS/${s}x${s}/apps/sanaga.png"; done
    refresh
    echo "Sanaga removed (your data is still in ~/.local/share/sanaga)."
    exit 0
fi

echo "Building Sanaga (release)…"
cargo build --release --locked

install -Dm755 target/release/sanaga "$BIN"
for s in $SIZES; do
    mkdir -p "$ICONS/${s}x${s}/apps"
    "$BIN" --export-icon "$ICONS/${s}x${s}/apps/sanaga.png" "$s"
done
mkdir -p "$APPS"
# the launcher calls the installed program by its full path
sed "s|^Exec=sanaga|Exec=$BIN|" packaging/sanaga.desktop > "$APPS/sanaga.desktop"
chmod 644 "$APPS/sanaga.desktop"
refresh

echo "Sanaga installed: $BIN"
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "Note: $PREFIX/bin is not in your PATH; the launcher still works." ;;
esac
