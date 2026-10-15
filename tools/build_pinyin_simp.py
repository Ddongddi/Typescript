#!/usr/bin/env python3
"""Stage 1 of the simplified-Pinyin dictionary pipeline (zh).

Reads rime-pinyin-simp's pinyin_simp.dict.yaml and writes a normalized TSV:
    word <TAB> syllables <TAB> weight
sorted by syllables, then descending weight (deterministic, byte-stable input for
the stage-2 pack builder, which will compile it to a compact little-endian table in
crates/core/data/ — the same two-stage pattern as build_ipadic.py).

The script ships no data of its own: --fetch downloads the source, or pass --src.

Source:  https://github.com/rime/rime-pinyin-simp  (pinyin_simp.dict.yaml)
License: Apache-2.0 (the repo's LICENSE). The dictionary is derived from the Android
         Open Source Project's PinyinIME; when the compiled data ships, add both
         notices to THIRD_PARTY_NOTICES.txt.

Usage:
    python3 tools/build_pinyin_simp.py --fetch --out /tmp/pinyin_simp.tsv
    python3 tools/build_pinyin_simp.py --src pinyin_simp.dict.yaml --out out.tsv
"""

import argparse
import urllib.request

SOURCE_URL = ("https://raw.githubusercontent.com/rime/rime-pinyin-simp"
              "/master/pinyin_simp.dict.yaml")


def fetch(url=SOURCE_URL):
    with urllib.request.urlopen(url) as response:
        return response.read().decode("utf-8")


def parse(text):
    """Yields (word, syllables, weight). Skips the YAML header; counts bad rows."""
    entries = []
    errors = 0
    in_body = False
    for line in text.splitlines():
        if line.rstrip() == "...":
            in_body = True
            continue
        if not in_body or not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 3:
            errors += 1
            continue
        word, pinyin, weight = parts
        syllables = " ".join(pinyin.split()).lower()
        try:
            w = int(weight)
        except ValueError:
            errors += 1
            continue
        if not word or not syllables or not all(
                c.isascii() and c.isalpha() for t in syllables.split() for c in t):
            errors += 1
            continue
        entries.append((word, syllables, w))
    return entries, errors


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--fetch", action="store_true", help="download from SOURCE_URL")
    src.add_argument("--src", help="path to an existing pinyin_simp.dict.yaml")
    ap.add_argument("--out", required=True, help="output TSV path")
    args = ap.parse_args()

    text = fetch() if args.fetch else open(args.src, encoding="utf-8").read()
    entries, errors = parse(text)
    entries.sort(key=lambda e: (e[1], -e[2], e[0]))

    seen = set()
    dup = 0
    for word, syllables, _ in entries:
        key = (word, syllables)
        dup += key in seen
        seen.add(key)

    syllables = sorted({t for _, s, _ in entries for t in s.split()})
    phrases = sum(1 for w, _, _ in entries if len(w) > 1)
    print(f"entries: {len(entries)} (phrases {phrases}, singles {len(entries) - phrases})")
    print(f"distinct syllables: {len(syllables)} ({syllables[0]}..{syllables[-1]})")
    print(f"max weight: {max((w for _, _, w in entries), default=0)}")
    print(f"duplicate (word, syllables) pairs: {dup}; bad rows skipped: {errors}")

    with open(args.out, "w", encoding="utf-8") as f:
        for word, s, w in entries:
            f.write(f"{word}\t{s}\t{w}\n")
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
