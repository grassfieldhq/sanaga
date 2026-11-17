#!/usr/bin/env python3
"""What a screen reader is given by Sanaga, read through AT-SPI (the bus
Orca uses): the tree of its interface, one line per thing — role, name,
state. Run it while Sanaga is open and a screen reader (or anything that
sets org.a11y.Status ScreenReaderEnabled) is on:

    tools/a11y-tree.py              the tree of Sanaga's interface
    tools/a11y-tree.py --all        every tree named sanaga (the page's too)

Needs python3-gi and gir1.2-atspi-2.0. See docs/TESTING.md.
"""
import sys

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

STATES = (
    ("focused", Atspi.StateType.FOCUSED),
    ("selected", Atspi.StateType.SELECTED),
    ("checked", Atspi.StateType.CHECKED),
    ("expanded", Atspi.StateType.EXPANDED),
)


def walk(node, depth, out, limit=40):
    """One line per thing under `node`: its role, its name, its states."""
    if node is None or depth > limit:
        return
    try:
        role = node.get_role_name()
        name = node.get_name() or ""
        states = node.get_state_set()
        marks = [word for word, flag in STATES if states.contains(flag)]
        children = node.get_child_count()
    except Exception as error:  # (a thing that went away meanwhile)
        out.append("  " * depth + f"(gone: {error})")
        return
    out.append("  " * depth + f'{role} "{name}"' + (" [" + ",".join(marks) + "]" if marks else ""))
    for i in range(min(children, 400)):
        walk(node.get_child_at_index(i), depth + 1, out, limit)


if __name__ == "__main__":
    every = "--all" in sys.argv
    desktop = Atspi.get_desktop(0)
    shown = 0
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if not app or not (app.get_name() or "").lower().startswith("sanaga"):
            continue
        lines = []
        walk(app, 0, lines)
        print("\n".join(lines))
        shown += 1
        if not every:
            break
    if not shown:
        sys.exit("no application named sanaga on the accessibility bus (is a screen reader on?)")
