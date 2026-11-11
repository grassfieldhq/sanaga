#!/bin/sh
# Install Sanaga for the current user, without admin rights:
#   program      ~/.local/bin/sanaga
#   its helper   ~/.local/lib/sanaga/libsanaga_page_helper.so (loaded by the page processes)
#   launcher     ~/.local/share/applications/sanaga.desktop
#   icons        ~/.local/share/icons/hicolor/<size>/apps/sanaga.png (+ scalable/apps/sanaga.svg)
#
#   packaging/install.sh              build (release) and install
#   packaging/install.sh --uninstall  remove those files (your data stays in ~/.local/share/sanaga)
#   PREFIX=/some/where packaging/install.sh   install somewhere else
set -eu
cd "$(dirname "$0")/.."
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/sanaga"
HELPER="$PREFIX/lib/sanaga/libsanaga_page_helper.so"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"
SIZES="48 64 128 256 512"

refresh() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$APPS" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN" "$HELPER" "$APPS/sanaga.desktop"
    rmdir "$PREFIX/lib/sanaga" 2>/dev/null || true
    for s in $SIZES; do rm -f "$ICONS/${s}x${s}/apps/sanaga.png"; done
    rm -f "$ICONS/scalable/apps/sanaga.svg"
    refresh
    echo "Sanaga removed (your data is still in ~/.local/share/sanaga)."
    exit 0
fi

if [ -f Cargo.toml ]; then
    echo "Building Sanaga (release)…"
    cargo build --release --locked
elif [ ! -x target/release/sanaga ]; then
    echo "The program is missing: target/release/sanaga" >&2
    exit 1
fi
# (from a downloaded archive the program is already built)

install -Dm755 target/release/sanaga "$BIN"
# (tells Sanaga which process each tab runs in: the Activity page; Sanaga
# works without it)
if [ -f target/release/libsanaga_page_helper.so ]; then
    install -Dm644 target/release/libsanaga_page_helper.so "$HELPER"
fi
for s in $SIZES; do
    mkdir -p "$ICONS/${s}x${s}/apps"
    "$BIN" --export-icon "$ICONS/${s}x${s}/apps/sanaga.png" "$s"
done
install -Dm644 packaging/sanaga.svg "$ICONS/scalable/apps/sanaga.svg"
mkdir -p "$APPS"
# the launcher calls the installed program by its full path, and shows the
# icon by its full path too (every launcher finds it, even without an icon
# theme cache)
sed -e "s|^Exec=sanaga|Exec=$BIN|" -e "s|^Icon=sanaga$|Icon=$ICONS/256x256/apps/sanaga.png|" packaging/sanaga.desktop > "$APPS/sanaga.desktop"
chmod 644 "$APPS/sanaga.desktop"
refresh

echo "Sanaga installed: $BIN"
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "Note: $PREFIX/bin is not in your PATH; the launcher still works." ;;
esac

# Video: WebKit plays through GStreamer. Without these decoders, YouTube and
# other sites say "your browser can't play this video".
if command -v gst-inspect-1.0 >/dev/null 2>&1; then
    missing=""
    gst-inspect-1.0 --exists avdec_h264 || gst-inspect-1.0 --exists openh264dec || missing="$missing H.264"
    gst-inspect-1.0 --exists avdec_aac || gst-inspect-1.0 --exists fdkaacdec || missing="$missing AAC"
    gst-inspect-1.0 --exists dav1ddec || gst-inspect-1.0 --exists av1dec || missing="$missing AV1"
    gst-inspect-1.0 --exists opusparse || missing="$missing Opus"
    if [ -n "$missing" ]; then
        echo
        echo "Video codecs missing:$missing — YouTube and many sites won't play videos."
        echo "Install them (once), e.g.:"
        echo "  Arch / Omarchy:  pacman -S gst-libav gst-plugins-bad gst-plugin-va   (as root)"
        echo "  Debian / Ubuntu: apt install gstreamer1.0-libav gstreamer1.0-plugins-bad gstreamer1.0-vaapi   (as root)"
        echo "  Fedora:          dnf install gstreamer1-plugin-libav gstreamer1-plugins-bad-free   (as root)"
        echo "Or in Sanaga: Settings › General › Video and music › Install."
    fi
fi
