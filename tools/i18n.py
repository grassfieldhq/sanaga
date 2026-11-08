#!/usr/bin/env python3
"""Sanaga translations helper.

The interface texts are written in French in the code (`tr("…")`, `tf("…")`)
and translated in `assets/i18n/translations.txt`, one text per line:

    français ⟂ English ⟂ Español ⟂ Português ⟂ Deutsch

Usage (from the project folder):

    python3 tools/i18n.py check   # list texts used in the code but not translated
    python3 tools/i18n.py build   # write assets/i18n/{en,es,pt,de}.tsv

`build` also fails when a line does not have exactly five parts.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TABLE = ROOT / "assets" / "i18n" / "translations.txt"
LANGS = ["en", "es", "pt", "de"]


def rows():
    out = []
    for line in TABLE.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or "⟂" not in line:
            continue
        parts = line.split("⟂")
        if len(parts) != 5:
            raise SystemExit(f"bad line (needs 5 parts): {parts[0]}")
        out.append(parts)
    return out


def build():
    table = rows()
    for k, lang in enumerate(LANGS, start=1):
        lines = [f"# Sanaga — {lang} (généré depuis translations.txt)"]
        lines += [f"{r[0]}\t{r[k]}" for r in table]
        (ROOT / "assets" / "i18n" / f"{lang}.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(table)} texts written for {', '.join(LANGS)}")


def check():
    known = {r[0] for r in rows()}
    pattern = re.compile(r'\b(?:tr|tf)\(\s*"((?:[^"\\]|\\.)*)"')
    missing = []
    for f in sorted((ROOT / "src").rglob("*.rs")):
        for m in pattern.finditer(f.read_text(encoding="utf-8")):
            text = m.group(1).replace('\\"', '"')
            if text not in known and text not in missing:
                missing.append(text)
    for text in missing:
        print(text)
    if missing:
        raise SystemExit(f"{len(missing)} text(s) without translation")
    print("all texts are translated")


if __name__ == "__main__":
    {"build": build, "check": check}.get(sys.argv[1] if len(sys.argv) > 1 else "", lambda: print(__doc__))()
