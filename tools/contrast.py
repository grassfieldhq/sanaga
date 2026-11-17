#!/usr/bin/env python3
"""Contrast of the tab bar's texts against what is really behind them.

WCAG 2 asks for 4.5:1 between a text and its background (AA, normal text).
Sanaga's bars are made of layers — the bar's colour, the tabs' track over
it, a hovered tab's lighter pill over that — some of them translucent: the
background of a title is what those layers give once put on one another.
This computes it the way the interface draws it (colours blended in sRGB,
as egui does) for the three looks (light, dark, private window), and says
which pairs are under 4.5:1.

    python3 tools/contrast.py            # the colours of src/theme.rs and src/app/tabs.rs
    python3 tools/contrast.py --before   # the colours before the contrast pass, to compare

Keep the tables below in step with src/theme.rs (`base_theme`) and
src/app/tabs.rs (`GROUP_COLORS`, `ink_on`, `group_hover`): they are copied by
hand. The same pairs are checked by `cargo test` from the real colours
(theme.rs `tab_titles_stand_out_from_the_tab_bar`, tabs.rs
`group_names_stand_out_from_their_colour`).
"""
import sys

BEFORE = "--before" in sys.argv


def rgb(h):
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))


def over(top, alpha, under):
    """`top` at `alpha` (0-255) drawn over the opaque colour `under`."""
    a = alpha / 255.0
    return tuple(top[i] * a + under[i] * (1 - a) for i in range(3))


def lum(c):
    def lin(v):
        v /= 255.0
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4

    r, g, b = (lin(v) for v in c)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def ratio(a, b):
    la, lb = lum(a), lum(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


def hexa(c):
    return "#%02X%02X%02X" % tuple(round(v) for v in c)


BLACK, WHITE = (0, 0, 0), (255, 255, 255)

# src/theme.rs, base_theme: (toolbar, strip, track, pill, text, sub, tab_text)
# track and pill are (colour, alpha); tab_text is the title of a tab that is not shown.
THEMES = {
    "light": dict(toolbar=rgb("F4F2ED"), strip=rgb("F4F2ED"), track=(BLACK, 26), pill=(rgb("FDFCF9"), 205), text=rgb("1D1B18"), sub=rgb("6B6862"), tab_text=rgb("5C5953")),
    "dark": dict(toolbar=rgb("2B2925"), strip=rgb("2B2925"), track=(WHITE, 30), pill=(WHITE, 36), text=rgb("F8F5F0"), sub=rgb("A8A59E"), tab_text=rgb("C8C5BE")),
    "private": dict(toolbar=rgb("22212A"), strip=rgb("22212A"), track=(WHITE, 30), pill=(WHITE, 36), text=rgb("F8F5F0"), sub=rgb("A8A59E"), tab_text=rgb("C8C5BE")),
}

# src/app/tabs.rs, GROUP_COLORS
GROUPS = [("Gris", "5F6368"), ("Bleu", "1A73E8"), ("Rouge", "D93025"), ("Jaune", "E37400"), ("Vert", "188038"), ("Rose", "D01884"), ("Violet", "A142F4"), ("Cyan", "007B83"), ("Orange", "FA903E")]

# src/app/tabs.rs, ink_on: the dark ink used on a light group colour
INK = rgb("1D1B18")


def ink_on(colour):
    """White or dark ink, whichever stands out more (src/app/tabs.rs)."""
    return WHITE if ratio(WHITE, colour) >= ratio(INK, colour) else INK


def group_hover(colour):
    """A group's colour under the pointer: a little further from its ink (src/app/tabs.rs)."""
    to = 0.0 if ink_on(colour) == WHITE else 255.0
    return tuple(round(v * 0.85 + to * 0.15) for v in colour)


rows = []


def row(look, what, text, bg):
    rows.append((look, what, hexa(text), hexa(bg), ratio(text, bg)))


for look, t in THEMES.items():
    title = t["sub"] if BEFORE else t["tab_text"]
    track = over(t["track"][0], t["track"][1], t["toolbar"])
    # a hovered tab: the pill at 45 % over the track
    hovered = over(t["pill"][0], t["pill"][1] * 0.45, track)
    row(look, "tab not shown: title on the track", title, track)
    row(look, "tab not shown, pointer over it: title", title, hovered)
    # the tab shown: its title on the pill over the track
    row(look, "tab shown: title on its pill", t["text"], over(t["pill"][0], t["pill"][1], track))
    for name, h in GROUPS:
        c = rgb(h)
        # tab bar: the group's name on a chip of its colour. Under the pointer the chip was
        # the colour at 85 % over the track (paler); it is now `group_hover`.
        ink = WHITE if BEFORE else ink_on(c)
        row(look, "group %s: name on its chip (tab bar)" % name, ink, c)
        row(look, "group %s: name on its chip, pointer over it" % name, ink, over(c, 255 * 0.85, track) if BEFORE else group_hover(c))
        # vertical tabs: the group's name on the tab list's background
        row(look, "group %s: name in the vertical tab list" % name, c if BEFORE else t["text"], t["strip"])

bad = 0
for look, what, text, bg, r in rows:
    mark = "" if r >= 4.5 else "   < 4.5"
    bad += r < 4.5
    print("%-8s %-52s %s on %s  %5.2f:1%s" % (look, what, text, bg, r, mark))
print("%d pairs, %d under 4.5:1 (%s)" % (len(rows), bad, "before" if BEFORE else "now"))
sys.exit(1 if bad and not BEFORE else 0)
