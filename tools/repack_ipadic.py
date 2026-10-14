#!/usr/bin/env python3
"""Converts an older ipadic_compact.bin (the TSIPDIC2 layout) to the packed TSIPDIC3 layout
that tools/build_ipadic.py writes. Fields are re-encoded, never rebuilt, so the candidates an
engine reads out of the file cannot change; only the bytes the file takes.

TSIPDIC2 (what this reads): a 32-byte header, then u32 offset tables x3 (readings, entries,
entry bytes), the UTF-8 readings blob, the entries blob ([u16 lc][u16 rc][i16 cost][u8 slen]
[surface bytes] per entry) and the connection matrix.

Usage (from the repository root):
    python3 tools/repack_ipadic.py --old old.bin --out crates/core/data/ipadic_compact.bin
"""

import argparse
import os
import struct
import sys

from build_ipadic import write_compact


def load_old(path: str):
    data = open(path, "rb").read()
    if data[:8] != b"TSIPDIC2":
        raise SystemExit(f"{path}: magic {data[:8]!r}, expected TSIPDIC2")
    n_readings, n_entries = struct.unpack_from("<II", data, 8)
    lsize, rsize = struct.unpack_from("<HH", data, 20)
    readings_len, entries_len = struct.unpack_from("<II", data, 24)
    tables = 32
    table_len = (n_readings + 1) * 4
    at_readings = tables + 3 * table_len
    at_entries = at_readings + readings_len
    at_matrix = at_entries + entries_len

    def u32_at(at: int) -> int:
        return struct.unpack_from("<I", data, at)[0]

    def off(table: int, i: int) -> int:
        return u32_at(tables + table * table_len + 4 * i)

    readings_blob = data[at_readings:at_entries]
    entries_blob = data[at_entries:at_matrix]
    readings = [readings_blob[off(0, i):off(0, i + 1)].decode() for i in range(n_readings)]
    by_reading = {}
    for i, r in enumerate(readings):
        start, end = off(1, i), off(1, i + 1)
        at = off(2, i)
        rows = []
        for _ in range(start, end):
            lc, rc, cost, slen = struct.unpack_from("<HHhB", entries_blob, at)
            rows.append((entries_blob[at + 7:at + 7 + slen].decode(), cost, lc, rc))
            at += 7 + slen
        by_reading[r] = rows
    if at_matrix + lsize * rsize * 2 > len(data):
        raise SystemExit(f"{path}: truncated")
    matrix = struct.unpack_from(f"<{lsize * rsize}h", data, at_matrix)
    print(f"read {path}: {n_readings} readings, {n_entries} entries, matrix {lsize}x{rsize}")
    return readings, by_reading, lsize, rsize, matrix


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--old", required=True, help="an ipadic_compact.bin in the TSIPDIC2 layout")
    ap.add_argument("--out", default="crates/core/data/ipadic_compact.bin")
    args = ap.parse_args()
    readings, by_reading, lsize, rsize, matrix = load_old(args.old)
    n_entries, n_top, n_rest = write_compact(args.out, readings, by_reading, lsize, rsize, matrix)
    size = os.path.getsize(args.out)
    print(f"readings: {len(readings)}, entries: {n_entries}, surface characters: {n_top}+{n_rest}")
    print(f"wrote {args.out} ({size/1e6:.1f} MB)")


if __name__ == "__main__":
    sys.exit(main())
