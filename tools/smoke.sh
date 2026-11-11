#!/bin/sh
# Starts the real window (the one given, default: the debug build), waits for
# the interface, loads a page and asks it its title through Sanaga's own
# channel. Used by CI under a virtual display; works on a desktop too.
#   tools/smoke.sh [path/to/sanaga]
set -u
bin="${1:-target/debug/sanaga}"
here="$(cd "$(dirname "$0")/.." && pwd)"

# One run: the window is asked where it is, then the page its title, at the
# seconds given (a slow machine needs more).
run() {
    data="$(mktemp -d)"
    out="$data/out.log"
    XDG_DATA_HOME="$data" SANAGA_LIVE_TEST="$1:info,$2:probe:'title='+document.title,$3:quit" \
      timeout 120 "$bin" "file://$here/tests/smoke.html" >"$out" 2>&1
    status=0
    if grep -q "^live info" "$out"; then echo "interface: ok"; else echo "interface: did not start"; status=1; fi
    if grep -q "^live probe title=smoke 2" "$out"; then echo "page: ok"; else echo "page: did not load or did not answer"; status=1; fi
    [ "$status" = 0 ] || tail -40 "$out"
    rm -rf "$data"
    return "$status"
}

# (a second, slower try: a loaded machine can take longer to show the first
# page than the first try allows — a real failure fails both)
run 4 8 10 || { echo "again, more slowly"; run 10 25 30; }
