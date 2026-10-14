#!/usr/bin/env python3
"""Builds crates/core/data/kana_trigrams.bin from a kana trigram table (lines of three
characters, a TAB and a count — the Tatoeba-derived counts the model in lang/japanese.rs
reads; the script that counted them is not in this repository, and the text table this
converts is crates/core/data/kana_trigrams.txt in the git history before v0.6.6).

Packed layout (all little-endian); keep in step with lang/japanese.rs:
    [0]  magic "TSKANA01" (8 bytes)
    [8]  u16 n_chars
    [10] 2 bytes padding
    [12] n_chars x u16 code points (ascending)
    then one 5-byte record per trigram: [u8 a][u8 b][u8 c][u16 count], codes index the table.
A character is one byte and a count fits u16, so the file is ~40% of the text it came from.

Usage (from the repository root):
    python3 tools/build_kana_table.py --table old.txt --out crates/core/data/kana_trigrams.bin
"""

import argparse
import struct
import sys

MAGIC = b"TSKANA01"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--table", required=True, help='a "three characters TAB count" table')
    ap.add_argument("--out", default="crates/core/data/kana_trigrams.bin")
    args = ap.parse_args()

    records = []
    chars = set()
    with open(args.table, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            field, _, count = line.partition("\t")
            if len(field) != 3 or not count.isdigit():
                raise SystemExit(f"{args.table}: {line!r} is not three characters and a count")
            records.append((field, int(count)))
            chars.update(field)
    if not records:
        raise SystemExit(f"{args.table}: no records")
    table = sorted(chars)
    if len(table) > 256:
        raise SystemExit(f"{args.table}: {len(table)} characters, one byte fits 256")
    codes = {c: i for i, c in enumerate(table)}

    blob = bytearray()
    blob += MAGIC
    blob += struct.pack("<HH", len(table), 0)
    blob += b"".join(struct.pack("<H", ord(c)) for c in table)
    for field, count in records:
        if not 0 < count < 65536:
            raise SystemExit(f"{field}: count {count} does not fit u16")
        blob += bytes(codes[c] for c in field)
        blob += struct.pack("<H", count)
    with open(args.out, "wb") as f:
        f.write(blob)
    print(f"characters: {len(table)}, records: {len(records)}")
    print(f"wrote {args.out} ({len(blob)/1e6:.2f} MB)")


if __name__ == "__main__":
    sys.exit(main())
