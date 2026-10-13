#!/usr/bin/env python3
"""Builds crates/core/data/ipadic_compact.bin from MeCab IPADIC sources.

The engine (crates/core/src/decide/kanji.rs) reads this file by include_bytes!.
Rebuild it only when changing the source data or the format below.

Source (fetch once):
    https://github.com/taku910/mecab -> mecab-ipadic/  (*.csv, matrix.def)
License: NAIST (see THIRD_PARTY_NOTICES.txt); redistribution requires the notice.

Word costs are adjusted by word frequency (wordfreq's zipf, CC BY-SA 4.0 — see
THIRD_PARTY_NOTICES.txt): common spellings get a lower cost, which fixes orderings IPADIC's
2007 newspaper costs get wrong (hiragana てんき before 天気, 返還 before 変換, ホン before 本
in phrases is left as-is). The JSON {form: zipf} was made once with wordfreq 3.1.1:

    from wordfreq import zipf_frequency
    zipf_frequency(word, "ja")   # wordfreq's MeCab tokenizer was not needed:
                                 # single words went through simple_tokenize instead

covering every surface and lemma in the CSVs (325,871 forms).

Usage:
    python3 tools/build_ipadic.py --ipadic-dir /path/to/mecab-ipadic \
        [--wordfreq-json wordfreq_ja.json] --out crates/core/data/ipadic_compact.bin

Compact layout (all little-endian); keep in step with kanji.rs:
    [0]  magic "TSIPDIC1" (8 bytes)
    [8]  u32 n_readings           readings (hiragana), sorted
    [12] u32 n_entries
    [16] u8  max_reading_chars
    [17] 3 bytes padding
    [20] u16 lsize, u16 rsize     connection matrix dimensions (MeCab ids)
    [24] u32 readings_len         blob bytes
    [28] u32 entries_len          blob bytes
    [32] u32 reading_offsets[n_readings+1]   byte offsets into the readings blob
         u32 entry_offsets[n_readings+1]     entry index ranges per reading
         u32 entry_byte_offsets[n_readings+1] byte offsets of the first entry per reading
         readings blob (UTF-8 hiragana, concatenated in sorted order)
         entries blob:  per entry [u16 lc][u16 rc][i16 cost][u8 slen][surface bytes]
         matrix: i16 lsize*rsize, indexed [l + lsize * r] as MeCab stores it
                (l = right-id of the left word, r = left-id of the right word)
"""

import argparse
import glob
import json
import os
import struct
import sys
from collections import defaultdict

# (zipf - 4) * SCALE is added to the word cost (common: lower cost); clamped. Single
# characters keep IPADIC's costs: a bonus there carves words into 多+さ+か instead of 大阪
# and lifts odd single-kanji splits into the candidate list (言いてん気). The costs of a
# few words that stay wrong for kana input (ホン before 本) are fixed by cycling or learning.
ZIPF_SCALE = 1500.0
ZIPF_CLAMP = 2600
ZIPF_MIN_CHARS = 2

# Hiragana readings only: they are what the input method looks up.
KANA_LO, KANA_HI = 0x3041, 0x3096  # ぁ .. ゖ
CHOON = 0x30FC  # ー is part of words (らーめん)


def kata_to_hira(s: str) -> str:
    return "".join(
        chr(ord(c) - 0x60) if 0x30A1 <= ord(c) <= 0x30F6 else c for c in s
    )


def reading_ok(r: str) -> bool:
    return len(r) > 0 and all(
        (KANA_LO <= ord(c) <= KANA_HI) or ord(c) == CHOON for c in r
    )


def load_entries(ipadic_dir: str, zipf: dict):
    entries = []
    skipped = 0
    for path in sorted(glob.glob(os.path.join(ipadic_dir, "*.csv"))):
        with open(path, encoding="euc-jp", errors="replace") as f:
            for line in f:
                parts = line.rstrip("\n").split(",")
                if len(parts) < 13:
                    skipped += 1
                    continue
                surface = parts[0]
                lc, rc, cost = int(parts[1]), int(parts[2]), int(parts[3])
                feats = parts[4:]
                reading = feats[7] if len(feats) > 7 else surface
                if not reading or reading == "*":
                    reading = surface
                lemma = feats[6] if len(feats) > 6 and feats[6] != "*" else surface
                z = max(zipf.get(surface, 0.0), zipf.get(lemma, 0.0))
                if z > 0 and len(surface) >= ZIPF_MIN_CHARS:
                    adjust = max(-ZIPF_CLAMP, min(ZIPF_CLAMP, (4.0 - z) * ZIPF_SCALE))
                    cost = int(round(cost + adjust))
                reading = kata_to_hira(reading)
                if not reading_ok(reading):
                    skipped += 1
                    continue
                if len(surface.encode()) > 255 or len(reading) > 255:
                    skipped += 1
                    continue
                entries.append((reading, surface, max(-32768, min(32767, cost)),
                                max(0, min(65535, lc)), max(0, min(65535, rc))))
    # A directory can hold the same entry twice (merged exports); keep each one once.
    entries = sorted(set(entries))
    return entries, skipped


def load_matrix(ipadic_dir: str):
    path = os.path.join(ipadic_dir, "matrix.def")
    with open(path) as f:
        lsize, rsize = (int(x) for x in f.readline().split())
        matrix = [0] * (lsize * rsize)
        for line in f:
            l, r, c = (int(x) for x in line.split())
            matrix[l + lsize * r] = max(-32768, min(32767, c))
    if len(matrix) != lsize * rsize:
        raise SystemExit("matrix.def: size mismatch")
    return lsize, rsize, matrix


def build(ipadic_dir: str, out_path: str, zipf: dict) -> None:
    entries, skipped = load_entries(ipadic_dir, zipf)
    by_reading = defaultdict(list)
    for reading, surface, cost, lc, rc in entries:
        by_reading[reading].append((surface, cost, lc, rc))
    readings = sorted(by_reading)
    for r in readings:
        by_reading[r].sort(key=lambda e: e[1])  # cheapest first

    reading_offsets = [0]
    readings_blob = bytearray()
    entry_offsets = [0]
    entry_byte_offsets = [0]
    entries_blob = bytearray()
    for r in readings:
        readings_blob += r.encode()
        reading_offsets.append(len(readings_blob))
        for surface, cost, lc, rc in by_reading[r]:
            s = surface.encode()
            entries_blob += struct.pack("<HHhB", lc, rc, cost, len(s)) + s
        entry_offsets.append(entry_offsets[-1] + len(by_reading[r]))
        entry_byte_offsets.append(len(entries_blob))

    lsize, rsize, matrix = load_matrix(ipadic_dir)
    max_reading_chars = max(len(r) for r in readings)

    header = struct.pack(
        "<8sIIB3xHHII",
        b"TSIPDIC2",
        len(readings),
        entry_offsets[-1],
        max_reading_chars,
        lsize,
        rsize,
        len(readings_blob),
        len(entries_blob),
    )
    assert len(header) == 32
    with open(out_path, "wb") as f:
        f.write(header)
        f.write(struct.pack(f"<{len(reading_offsets)}I", *reading_offsets))
        f.write(struct.pack(f"<{len(entry_offsets)}I", *entry_offsets))
        f.write(struct.pack(f"<{len(entry_byte_offsets)}I", *entry_byte_offsets))
        f.write(readings_blob)
        f.write(entries_blob)
        f.write(struct.pack(f"<{len(matrix)}h", *matrix))
    size = os.path.getsize(out_path)
    print(f"readings: {len(readings)}, entries: {entry_offsets[-1]}, "
          f"max reading chars: {max_reading_chars}, matrix: {lsize}x{rsize}, "
          f"skipped: {skipped}")
    print(f"wrote {out_path} ({size/1e6:.1f} MB)")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--ipadic-dir", required=True,
                    help="directory with MeCab IPADIC *.csv and matrix.def")
    ap.add_argument("--wordfreq-json",
                    help="JSON {word: zipf} from wordfreq (see the docstring); "
                         "without it, IPADIC's raw costs are kept")
    ap.add_argument("--out", default="crates/core/data/ipadic_compact.bin")
    args = ap.parse_args()
    zipf = {}
    if args.wordfreq_json:
        with open(args.wordfreq_json) as f:
            zipf = json.load(f)
        print(f"wordfreq entries: {len(zipf)}")
    build(args.ipadic_dir, args.out, zipf)


if __name__ == "__main__":
    sys.exit(main())
