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

Packed layout (all little-endian); keep in step with kanji.rs:
    [0]  magic "TSIPDIC3" (8 bytes)
    [8]  u32 n_readings           readings (hiragana), sorted
    [12] u32 n_entries
    [16] u8  max_reading_chars
    [17] 3 bytes padding
    [20] u16 lsize, u16 rsize     connection matrix dimensions (MeCab ids, <= 2048)
    [24] u32 readings_len         blob bytes
    [28] u32 entries_len          blob bytes
    [32] u16 n_top                surface characters stored in one byte
    [34] u16 n_rest               surface characters stored in two
    [36] u8  reading_lens[n_readings]      encoded reading bytes (one per character)
    [36] + n_readings            u8  entry_counts[n_readings]
    [36] + 2*n_readings          u16 entry_byte_lens[n_readings]  entry blob bytes per reading
    [36] + 4*n_readings          u16 charset: the n_top code points (most frequent first),
                                 then the n_rest (ascending)
    ...                          readings blob: ぁ..ゖ (0x3041..0x3096) -> 0..85, ー (0x30fc) -> 86
    ...                          entries blob: per entry
                                     u24 packed (lc | rc << 11), i16 cost, u8 nbytes,
                                     nbytes x code: below 0x80 charset[code], else
                                     (code - 0x80) << 8 | next indexes charset[n_top + code]
    ...                          matrix: i16 lsize*rsize, indexed [l + lsize * r] as MeCab stores it
                                 (l = right-id of the left word, r = left-id of the right word)

Every table sits at its natural width: readings are one byte per character, lc/rc are 11 bits,
the per-reading lengths fit u8/u16, and surfaces index a ~5,300 code-point table in one or two
bytes. The reader rebuilds the u32 prefix sums from the lengths at load, so lookups stay O(1)
and the candidates are exactly what the old UTF-8 layout produced.
"""

import argparse
import glob
import json
import os
import struct
import sys
from collections import Counter, defaultdict

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

MAGIC = b"TSIPDIC3"
TOP_CODES = 128  # surface characters stored in one byte; the rest take two


def reading_code(r: str) -> bytes:
    """The reading as one byte per character (ぁ..ゖ -> 0..85, ー -> 86)."""
    out = bytearray()
    for c in r:
        v = ord(c)
        if KANA_LO <= v <= KANA_HI:
            out.append(v - KANA_LO)
        else:
            assert v == CHOON, f"reading {r!r} is not hiragana"
            out.append(86)
    return bytes(out)


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
                # 記号 whose reading is kana (δ = デルタ, Ｎ = エヌ, ρ = ロー) exist so MeCab can
                # read symbols; from kana input they are noise that outranks the word itself
                # (よーそろー → よーソΡ, のーと → ノー＆). Keep self-reading symbols only.
                if feats[0] == "記号" and kata_to_hira(reading) != surface:
                    skipped += 1
                    continue
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


def write_compact(out_path: str, readings, by_reading, lsize: int, rsize: int, matrix) -> tuple:
    """Writes the packed table (the layout in the module docstring) for the readings and their
    entries; returns (n_entries, n_top, n_rest)."""
    assert lsize <= 2048 and rsize <= 2048, "lc/rc are 11-bit fields"
    counts = Counter()
    for r in readings:
        for surface, _cost, _lc, _rc in by_reading[r]:
            counts.update(surface)
    # Surface characters: the top 128 by count get one-byte codes; the rest, ascending, two.
    top = [c for c, _ in sorted(counts.items(), key=lambda kv: (-kv[1], ord(kv[0])))[:TOP_CODES]]
    top_at = {c: i for i, c in enumerate(top)}
    rest = sorted((c for c in counts if c not in top_at), key=ord)
    assert len(rest) < 0x8000, "too many surface characters for a two-byte code"
    rest_at = {c: i for i, c in enumerate(rest)}

    def surface_code(surface: str) -> bytes:
        out = bytearray()
        for c in surface:
            if c in top_at:
                out.append(top_at[c])
            else:
                i = rest_at[c]
                out.append(0x80 + (i >> 8))
                out.append(i & 0xFF)
        return bytes(out)

    reading_lens = bytearray()
    readings_blob = bytearray()
    for r in readings:
        code = reading_code(r)
        assert len(code) <= 255, f"reading {r!r} is too long"
        reading_lens.append(len(code))
        readings_blob += code

    entry_counts = bytearray()
    entry_byte_lens = bytearray()
    entries_blob = bytearray()
    n_entries = 0
    for r in readings:
        start = len(entries_blob)
        for surface, cost, lc, rc in by_reading[r]:
            code = surface_code(surface)
            assert len(code) <= 255, f"surface {surface!r} is too long"
            assert 0 <= lc < 2048 and 0 <= rc < 2048, f"ids of {surface!r} do not fit 11 bits"
            entries_blob += (lc | (rc << 11)).to_bytes(3, "little")
            entries_blob += struct.pack("<hB", cost, len(code))
            entries_blob += code
            n_entries += 1
        entry_counts.append(len(by_reading[r]))
        delta = len(entries_blob) - start
        assert delta <= 65535, f"reading {r!r} has too many entry bytes"
        entry_byte_lens += struct.pack("<H", delta)

    charset = b"".join(struct.pack("<H", ord(c)) for c in top + rest)
    header = struct.pack(
        "<8sIIB3xHHIIHH",
        MAGIC,
        len(readings),
        n_entries,
        max(len(r) for r in readings),
        lsize,
        rsize,
        len(readings_blob),
        len(entries_blob),
        len(top),
        len(rest),
    )
    assert len(header) == 36
    with open(out_path, "wb") as f:
        f.write(header)
        f.write(reading_lens)
        f.write(entry_counts)
        f.write(entry_byte_lens)
        f.write(charset)
        f.write(readings_blob)
        f.write(entries_blob)
        f.write(struct.pack(f"<{lsize * rsize}h", *matrix))
    return n_entries, len(top), len(rest)


def build(ipadic_dir: str, out_path: str, zipf: dict) -> None:
    entries, skipped = load_entries(ipadic_dir, zipf)
    by_reading = defaultdict(list)
    for reading, surface, cost, lc, rc in entries:
        by_reading[reading].append((surface, cost, lc, rc))
    readings = sorted(by_reading)
    for r in readings:
        by_reading[r].sort(key=lambda e: e[1])  # cheapest first

    lsize, rsize, matrix = load_matrix(ipadic_dir)
    n_entries, n_top, n_rest = write_compact(out_path, readings, by_reading, lsize, rsize, matrix)
    size = os.path.getsize(out_path)
    print(f"readings: {len(readings)}, entries: {n_entries}, "
          f"max reading chars: {max(len(r) for r in readings)}, matrix: {lsize}x{rsize}, "
          f"surface characters: {n_top}+{n_rest}, skipped: {skipped}")
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
