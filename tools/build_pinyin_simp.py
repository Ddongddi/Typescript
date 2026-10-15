#!/usr/bin/env python3
"""The simplified-Pinyin dictionary pipeline (zh).

Stage 1: read rime-pinyin-simp's pinyin_simp.dict.yaml, write a normalized TSV
    word <TAB> syllables <TAB> weight
sorted by syllables, then descending weight, then word (deterministic).

Stage 2 (--bin): compile those entries into the compact little-endian table the
engine reads, crates/core/data/pinyin_compact.bin. Format TSZHPIN1 (keep in
step with crates/core/src/lang/chinese.rs):

    [0]  "TSZHPIN1" (8 bytes)
    [8]  u32 n_readings
    [12] u32 readings_len          blob bytes
    [16] u32 entries_len           blob bytes
    [20] u32 n_entries
    [24] u16 max_reading_bytes
    [26] u16 reserved (0)
    [28] u16 reading_lens[n_readings]
    ... u16 entry_counts[n_readings]
    ... u32 entry_byte_lens[n_readings]
    ... readings blob: the readings, syllables joined with one space, utf-8,
        sorted bytewise (that equals syllable-sequence order: ' ' sorts below
        every letter, and a prefix sorts before its continuations).
    ... entries blob: per reading in order, its entries sorted by weight
        descending: u8 word_len, the word utf-8, u32 weight.

The script ships no data of its own: --fetch downloads the source, or pass --src.
Source:  https://github.com/rime/rime-pinyin-simp  (pinyin_simp.dict.yaml)
License: Apache-2.0 (the repo's LICENSE). The dictionary derives from the
         Android Open Source Project's PinyinIME; when the compiled data ships,
         both notices belong in THIRD_PARTY_NOTICES.txt.

Usage:
    python3 tools/build_pinyin_simp.py --fetch --bin crates/core/data/pinyin_compact.bin
    python3 tools/build_pinyin_simp.py --src pinyin_simp.dict.yaml --out out.tsv --bin out.bin
"""

import argparse
import struct
import urllib.request

SOURCE_URL = ("https://raw.githubusercontent.com/rime/rime-pinyin-simp"
              "/master/pinyin_simp.dict.yaml")


def fetch(url=SOURCE_URL):
    with urllib.request.urlopen(url) as response:
        return response.read().decode("utf-8")


def parse(text):
    """(word, syllables, weight) rows; skips the YAML header, counts bad rows."""
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


def group(entries):
    """[(reading, [(word, weight), ...]), ...] sorted by reading, entries by weight desc."""
    groups = {}
    for word, syllables, weight in entries:
        groups.setdefault(syllables, []).append((word, weight))
    return [(reading, sorted(items, key=lambda e: (-e[1], e[0])))
            for reading, items in sorted(groups.items())]


def write_bin(entries, path):
    readings = group(entries)
    reading_lens, entry_counts, entry_lens = [], [], []
    reading_blob, entry_blob = bytearray(), bytearray()
    for reading, items in readings:
        rb = reading.encode("utf-8")
        assert 0 < len(rb) < 65536, reading
        reading_lens.append(len(rb))
        reading_blob += rb
        eb = bytearray()
        for word, weight in items:
            wb = word.encode("utf-8")
            assert 0 < len(wb) < 256, word
            eb += bytes([len(wb)]) + wb + struct.pack("<I", weight)
        assert len(items) < 65536, reading
        entry_counts.append(len(items))
        entry_lens.append(len(eb))
        entry_blob += eb
    n = len(readings)
    header = b"TSZHPIN1" + struct.pack(
        "<IIIIHH", n, len(reading_blob), len(entry_blob),
        sum(entry_counts), max(reading_lens), 0)
    arrays = (struct.pack(f"<{n}H", *reading_lens)
              + struct.pack(f"<{n}H", *entry_counts)
              + struct.pack(f"<{n}I", *entry_lens))
    with open(path, "wb") as f:
        f.write(header + arrays + reading_blob + entry_blob)
    return n


def selfcheck(path):
    """Read the file back the way the Rust reader will and spot-check lookups."""
    data = open(path, "rb").read()
    assert data[:8] == b"TSZHPIN1", "magic"
    n, readings_len, entries_len, n_entries, max_rb, _ = struct.unpack_from("<IIIIHH", data, 8)
    at = 28
    reading_lens = struct.unpack_from(f"<{n}H", data, at)
    entry_counts = struct.unpack_from(f"<{n}H", data, at + 2 * n)
    entry_lens = struct.unpack_from(f"<{n}I", data, at + 4 * n)
    blob = at + 8 * n
    readings = []
    o = 0
    for ln in reading_lens:
        readings.append(data[blob + o: blob + o + ln].decode("utf-8"))
        o += ln
    assert o == readings_len, "readings length"
    assert readings == sorted(readings), "readings sorted"
    assert sum(entry_counts) == n_entries
    assert max(reading_lens) == max_rb

    def lookup(key):
        import bisect
        i = bisect.bisect_left(readings, key)
        if i >= n or readings[i] != key:
            return None
        eo = sum(entry_lens[:i])          # fine for a spot check
        out = []
        eb = blob + readings_len + eo
        end = eb + entry_lens[i]
        while eb < end:
            wl = data[eb]
            same = data[eb + 1: eb + 1 + wl].decode("utf-8")
            weight = struct.unpack_from("<I", data, eb + 1 + wl)[0]
            out.append((same, weight))
            eb += 1 + wl + 4
        assert eb == end, "entry blob walk"
        return out

    top = lookup("ni hao")
    assert top and top[0][0] == "你好", f"nihao: {top[:2]}"
    assert lookup("zzz") is None
    assert lookup("ni") and any(w == "你" for w, _ in lookup("ni"))
    return n, n_entries, readings_len, entries_len


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--fetch", action="store_true", help="download from SOURCE_URL")
    src.add_argument("--src", help="path to an existing pinyin_simp.dict.yaml")
    ap.add_argument("--out", help="normalized TSV path")
    ap.add_argument("--bin", help="compiled TSZHPIN1 path (crates/core/data/zh/pinyin_compact.bin)")
    args = ap.parse_args()
    if not args.out and not args.bin:
        ap.error("pass --out, --bin, or both")

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

    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            for word, s, w in entries:
                f.write(f"{word}\t{s}\t{w}\n")
        print(f"wrote {args.out}")
    if args.bin:
        n = write_bin(entries, args.bin)
        n2, n_entries, rlen, elen = selfcheck(args.bin)
        import os
        print(f"wrote {args.bin}: {os.path.getsize(args.bin)} bytes, "
              f"{n} readings, {n_entries} entries "
              f"(readings {rlen}B, entries {elen}B) - selfcheck ok")


if __name__ == "__main__":
    main()
