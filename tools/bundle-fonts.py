#!/usr/bin/env python3
"""Fetch the latin-subset woff2 files for the dashboard's fonts and emit them
as data-URI @font-face rules in crates/config/src/web/fonts.css.

The dashboard is a single self-contained HTML document (see web.rs) served
from 127.0.0.1, so the fonts ride inside the page instead of being fetched
from Google Fonts at every open (offline-safe, no third-party request).

Sources (all SIL Open Font License 1.1):
    Inter Tight 400/500/600, JetBrains Mono 400/500, Instrument Serif 400
    https://fonts.google.com/specimen/Inter+Tight
    https://fonts.google.com/specimen/JetBrains+Mono
    https://fonts.google.com/specimen/Instrument+Serif

Usage:
    python tools/bundle-fonts.py
Reads the already-subset 'latin' files Google serves for css2 with a woff2
user agent, so no fonttools run is needed. Re-run to refresh. Google serves a
variable font once for several weights, so each file is downloaded and
embedded once, with a weight range (font-weight: 400 600).

Status: nothing reads fonts.css yet. Before its output is committed the page
has to be pointed at it:
    - web.rs: add include_str!("web/fonts.css") first in STYLES and
      WELCOME_STYLES;
    - base.css: remove the @import of fonts.googleapis.com;
    - http.rs: the Content-Security-Policy allows no data: fonts and still lists
      the Google hosts: use font-src data: and drop both hosts (and update the
      policy test next to it);
    - THIRD_PARTY_NOTICES.txt: add the copyright and license text of the three
      fonts (the OFL asks for it when they are redistributed).
"""
import base64
import re
import sys
import urllib.request
from pathlib import Path

CSS2 = (
    "https://fonts.googleapis.com/css2"
    "?family=Inter+Tight:wght@400;500;600"
    "&family=JetBrains+Mono:wght@400;500"
    "&family=Instrument+Serif"
    "&display=swap"
)
UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
)
WANT = {
    ("Inter Tight", "400"),
    ("Inter Tight", "500"),
    ("Inter Tight", "600"),
    ("JetBrains Mono", "400"),
    ("JetBrains Mono", "500"),
    ("Instrument Serif", "400"),
}
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "config" / "src" / "web" / "fonts.css"


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as res:
        return res.read()


def field(pattern: str, body: str, what: str) -> str:
    """The first group of `pattern` in `body`; a clear exit if Google's CSS changed."""
    found = re.search(pattern, body)
    if found is None:
        sys.exit(f"unexpected css2 format: no {what} in a @font-face block:\n{body.strip()}")
    return found.group(1)


def latin_faces(css: str) -> dict[str, tuple[str, set[int]]]:
    """url -> (family, weights) for the wanted fonts' latin subset. One url can
    carry several weights (a variable font)."""
    faces: dict[str, tuple[str, set[int]]] = {}
    for subset, body in re.findall(r"/\*\s*(\S+)\s*\*/\s*@font-face\s*{([^}]*)}", css):
        if subset != "latin":
            continue
        family = field(r"font-family:\s*'([^']+)'", body, "font-family")
        weight = field(r"font-weight:\s*(\d+)", body, "font-weight")
        url = field(r"url\((https://[^)]+\.woff2)\)", body, "woff2 url")
        if (family, weight) in WANT:
            faces.setdefault(url, (family, set()))[1].add(int(weight))
    return faces


def main() -> None:
    faces = latin_faces(get(CSS2).decode("utf-8"))
    covered = {(family, str(w)) for family, weights in faces.values() for w in weights}
    missing = WANT - covered
    if missing:
        sys.exit(f"latin subset not served for: {sorted(missing)}")

    parts = [
        "/* Dashboard fonts, bundled (see tools/bundle-fonts.py).",
        "   Inter Tight 400-600, JetBrains Mono 400-500, Instrument Serif 400,",
        "   latin subsets as data URIs: no request leaves this machine. */",
    ]
    total = 0
    for url, (family, weights) in sorted(faces.items(), key=lambda kv: (kv[1][0], min(kv[1][1]))):
        data = get(url)
        total += len(data)
        low, high = min(weights), max(weights)
        span = str(low) if low == high else f"{low} {high}"
        b64 = base64.b64encode(data).decode("ascii")
        parts.append(
            f"@font-face{{font-family:'{family}';font-style:normal;font-weight:{span};"
            f"font-display:swap;src:url(data:font/woff2;base64,{b64}) format('woff2');}}"
        )
    # "\n" on every system: the file must not differ by who generated it.
    with OUT.open("w", encoding="utf-8", newline="\n") as out:
        out.write("\n".join(parts) + "\n")
    print(f"wrote {OUT} ({len(faces)} files, {total / 1024:.0f} KiB of woff2)")


if __name__ == "__main__":
    main()
