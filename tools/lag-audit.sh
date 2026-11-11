#!/bin/sh
# Looks for lag: starts the real window on a heavy profile (5 000 visits,
# 400 favourites, 80 tabs, 200 saved accounts), with the system's helper
# programs made slow (the keyring, xdg-settings… each take 2 s, as on a
# busy or half-configured system), walks through Sanaga's features, and
# lists every time the window stopped answering (SANAGA_DEBUG_STALLS).
#
#   tools/lag-audit.sh [path/to/sanaga] [frame budget in ms] [stall budget in ms]
#
# Two measures, two budgets:
#   frame  the time Sanaga's own code takes to make one frame (default
#          250 ms: a helper program waited for in the window's thread shows
#          as 2 000 ms and more; on a desktop frames take a few ms);
#   stall  the time the window did not answer, whatever held it — also the
#          drawing itself, which is slow without a graphics card (default
#          1 900 ms: just under one helper program waited for; use a small
#          virtual screen, 640x480).
#
# Fails when a budget is reached, when Sanaga panics, or when it does not
# finish the walk. Needs the debug build (the scripted actions are not in
# release builds) and a display (CI: xvfb-run).
# See docs/STABILITY-AUDIT-2026-11-11.md.
set -u
bin="${1:-target/debug/sanaga}"
frame_budget="${2:-250}"
stall_budget="${3:-1900}"
here="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
data="$work/data/sanaga"
mkdir -p "$data" "$work/slow" "$work/sync"

# --- a heavy profile
awk 'BEGIN { for (i = 0; i < 5000; i++) printf "%d\thttps://site%d.example/page/%d?q=%d\tPage %d of site %d — a fairly long title, as real ones are\n", 1760000000 + i * 60, i % 300, i, i, i, i % 300 }' > "$data/history.tsv"
awk 'BEGIN { for (i = 0; i < 400; i++) printf "https://favourite%d.example/\tFavourite number %d\n", i, i }' > "$data/bookmarks.tsv"
awk 'BEGIN { for (i = 0; i < 200; i++) printf "https://read%d.example/article\tArticle to read %d\n", i, i }' > "$data/reading.tsv"
awk 'BEGIN { for (i = 0; i < 200; i++) printf "https://login%d.example\tuser%d@example.com\n", i, i }' > "$data/logins.tsv"
awk -v page="file://$here/tests/smoke.html" 'BEGIN { for (i = 0; i < 80; i++) printf "%s?tab=%d\t%s\t%d\tTab number %d\t\t\t%s\t%d\t\n", page, i, (i % 4 == 0 ? "" : "Group " i % 4), (i < 3), i, (i == 79 ? "1" : ""), i % 4 }' > "$data/session.txt"
# (the full block lists are left out: fetching and compiling them needs the
# network and minutes of processor, which is not what is measured here)
printf 'language=en\nrestore_tabs=1\nblock_lists=0\nsync_dir=%s\n' "$work/sync" > "$data/settings.txt"
# a long page, for Find, the Reader and the source view
{
  echo '<!doctype html><html><head><meta charset="utf-8"><title>Long page</title></head><body><main><h1>A long page</h1>'
  awk 'BEGIN { for (i = 0; i < 4000; i++) printf "<p id=\"p%d\">Paragraph %d: the quick brown fox jumps over the lazy dog, <a href=\"#p%d\">again</a> and again.</p>\n", i, i, i + 1 }'
  echo '</main></body></html>'
} > "$work/long.html"

# --- Sanaga "installed" (its launcher and icons), so that making it the
# default browser and changing its icon really run
mkdir -p "$work/data/applications" "$work/data/icons/hicolor/scalable/apps" "$work/data/icons/hicolor/48x48/apps"
cp "$here/packaging/sanaga.desktop" "$work/data/applications/sanaga.desktop"
cp "$here/packaging/sanaga.svg" "$work/data/icons/hicolor/scalable/apps/sanaga.svg"
: > "$work/data/icons/hicolor/48x48/apps/sanaga.png"

# --- the system's helper programs, slow (each call is noted)
for tool in secret-tool xdg-settings xdg-mime gtk-update-icon-cache update-desktop-database hyprctl omarchy-cmd-default-browser; do
    printf '#!/bin/sh\necho "%s $1" >> "%s/calls.log"\nsleep 2\nexit 0\n' "$tool" "$work" > "$work/slow/$tool"
done
# (nothing is opened in other applications during the audit)
printf '#!/bin/sh\nexit 0\n' > "$work/slow/xdg-open"
chmod +x "$work/slow"/*

# --- the walk: one action every 1.5 s
actions="menu menu sidebar sidebar overview overview tabsearch type:tab esc new close next prev
history go:sanaga://bookmarks downloads settings section:0 section:1 section:2 section:3 section:4 section:5 section:6 section:7
settingsfind:cook go:sanaga://passwords go:sanaga://extensions go:sanaga://profiles go:sanaga://sites go:sanaga://import go:sanaga://clear
go:sanaga://about activity crashes go:file://$work/long.html find type:fox key:Enter key:Enter esc reader reader zin zout zreset source close
split split devices devices bookmark readinglist site esc aimenu aimenu sharemenu sharemenu profilemenu profilemenu overflow overflow
focus focus container esc makedefault appicon:1 appicon:0 syncnow launcher reload back fwd addr type:site1 key:ArrowDown esc
opentab:file://$here/tests/smoke.html close private close reopen"
spec=""
t=6
for a in $actions; do
    spec="$spec$t:$a,"
    t=$(awk -v t="$t" 'BEGIN { print t + 1.5 }')
done
spec="$spec$t:quit"
seconds=$(awk -v t="$t" 'BEGIN { print int(t) + 60 }')

out="$work/out.log"
# (LAG_AUDIT_STACKS=200: also print where the window's thread is during a
# stall of that many ms — slower, for looking into one)
PATH="$work/slow:$PATH" XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config" SANAGA_DEBUG_STALLS=30 SANAGA_DEBUG_STALL_STACKS="${LAG_AUDIT_STACKS:-1000000}" SANAGA_LIVE_TEST="$spec" \
    timeout "$seconds" "$bin" >"$out" 2>&1
code=$?

frames=$(grep '^frame ' "$out" | awk '{ if ($2 > m) m = $2 } END { print m + 0 }')
stalls=$(grep '^stall ' "$out" | awk '{ if ($2 > m) m = $2 } END { print m + 0 }')
echo "Sanaga's own work for one frame, the longest per action (budget: $frame_budget ms):"
grep '^frame ' "$out" | awk '{ k = $5; for (i = 6; i <= NF; i++) k = k " " $i; if ($2 > m[k]) m[k] = $2 } END { for (k in m) printf "  %5d ms  %s\n", m[k], k }' | sort -n -r | head -12
echo "The window not answering, the longest (budget: $stall_budget ms):"
grep '^stall ' "$out" | sort -k2,2 -n -r | head -8 | awk '{ print "  " $0 }'
echo "Helper programs called (each takes 2 s here): $(sort "$work/calls.log" 2>/dev/null | uniq -c | awk '{ printf "%s %s x%d, ", $2, $3, $1 }')"
status=0
if grep -q 'panicked at' "$out"; then echo "FAILED: Sanaga panicked:"; grep -A3 'panicked at' "$out"; status=1; fi
if [ "$code" != 0 ]; then echo "FAILED: Sanaga ended with code $code (124: it never finished the walk)"; tail -20 "$out"; status=1; fi
[ -d "$work/sync/Sanaga" ] || { echo "FAILED: the sync round did not run"; status=1; }
[ -s "$work/calls.log" ] || { echo "FAILED: no helper program was called (nothing was measured)"; status=1; }
[ "$frames" -lt "$frame_budget" ] || { echo "FAILED: a frame took $frames ms"; status=1; }
[ "$stalls" -lt "$stall_budget" ] || { echo "FAILED: the window did not answer for $stalls ms"; status=1; }
echo "Longest frame $frames ms, longest stall $stalls ms."
[ -n "${LAG_AUDIT_KEEP:-}" ] && echo "kept: $work" || rm -rf "$work"
exit "$status"
