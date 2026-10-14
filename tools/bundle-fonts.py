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
user agent, so no fonttools run is needed. Re-run to refresh.
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


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as res:
        return res.read()


def main():
    css = get(CSS2).decode("utf-8")
    blocks = re.findall(r"/\*\s*(\S+)\s*\*/\s*@font-face\s*{([^}]*)}", css)
    found = {}
    for subset, body in blocks:
        if subset != "latin":
            continue
        family = re.search(r"font-family:\s*'([^']+)'", body).group(1)
        weight = re.search(r"font-weight:\s*(\d+)", body).group(1)
        url = re.search(r"url\((https://[^)]+\.woff2)\)", body).group(1)
        if (family, weight) in WANT:
            found[(family, weight)] = url
    missing = WANT - set(found)
    if missing:
        sys.exit(f"latin subset not served for: {sorted(missing)}")

    parts = [
        "/* Dashboard fonts, bundled (see tools/bundle-fonts.py).",
        "   Inter Tight 400/500/600, JetBrains Mono 400/500, Instrument Serif 400,",
        "   latin subsets as data URIs: no request leaves this machine. */",
    ]
    total = 0
    for family, weight in sorted(found):
        data = get(found[(family, weight)])
        total += len(data)
        b64 = base64.b64encode(data).decode("ascii")
        parts.append(
            f"@font-face{{font-family:'{family}';font-style:normal;font-weight:{weight};"
            f"font-display:swap;src:url(data:font/woff2;base64,{b64}) format('woff2');}}"
        )
    OUT.write_text("\n".join(parts) + "\n", encoding="utf-8")
    print(f"wrote {OUT} ({total / 1024:.0f} KiB of woff2)")


if __name__ == "__main__":
    main()
