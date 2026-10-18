#!/usr/bin/env python3
"""Absorb Mozc OSS lexicon + IME connection costs into the native Rust lattice.

python tools/build_mozc.py --source temp/japanese-research/mozc/src/data/dictionary_oss
No C++/Bazel/runtime dependency. Source must be the pinned google/mozc revision below.
TSIPDIC4 extends TSIPDIC3: 12-bit IDs in the same 3 bytes, u16 per-reading entry counts,
u32 Unicode scalar charset; remaining tables are unchanged. Keep ALL contexts and homophones.
Mozc matrix [previous rid][next lid] is transposed to the reader's [next lid][previous rid].
Data licenses/notices: THIRD_PARTY_NOTICES.txt and crates/core/data/mozc/.
"""
import argparse
from array import array
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from build_ipadic import kata_to_hira, reading_ok, write_compact

REVISION = '0136c35782536cb43d710575c311cd4185982738'


def build(source, target):
    revision = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    assert revision == REVISION, f'expected Mozc {REVISION}, got {revision}'
    subprocess.run(['git', '-C', str(source), 'diff', '--exit-code', 'HEAD', '--', '.'], check=True)
    entries = set()
    sources = {}
    skipped = 0
    base = sorted(source.glob('dictionary[0-9][0-9].txt'))
    assert len(base) == 10, 'incomplete Mozc dictionary'
    src = source.parent.parent
    manual = [src / 'data/dictionary_manual' / name for name in ['places.tsv', 'words.tsv']]
    extra = [source / name for name in ['aux_dictionary.tsv', 'dictionary_filter.tsv', 'id.def']]
    scripts = [src / 'dictionary' / name for name in ['gen_aux_dictionary.py', 'gen_filtered_dictionary.py']]
    for path in base + extra + manual + scripts:
        sources[path.relative_to(src).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix='tacit-mozc-') as work:
        aux, filtered = Path(work) / 'aux.txt', Path(work) / 'filtered.txt'
        # Use the pinned upstream preprocessing, including manual fixes and cost inheritance.
        subprocess.run([sys.executable, str(scripts[0]), '--strict', '--output', str(aux),
                        '--aux_tsv', str(extra[0]), '--id_def', str(extra[2]),
                        '--words_tsv', *map(str, manual), '--dictionary_txts', *map(str, base)], check=True)
        subprocess.run([sys.executable, str(scripts[1]), '--output', str(filtered),
                        '--filter_tsv', str(extra[1]), '--dictionary_txts', *map(str, base)], check=True)
        for path in [filtered, aux]:
            for line in path.read_text(encoding='utf-8').splitlines():
                fields = line.split('\t')
                if not line or line.startswith('#'):
                    continue
                assert len(fields) >= 5, (path, line)
                reading, left, right, cost, surface = fields[:5]
                reading = kata_to_hira(reading)
                if not reading_ok(reading):
                    skipped += 1
                    continue
                assert 0 <= int(left) < 4096 and 0 <= int(right) < 4096
                assert -32768 <= int(cost) <= 32767
                entries.add((reading, surface, int(cost), int(left), int(right)))
    assert len(entries) > 1_000_000, 'incomplete Mozc dictionary'
    by_reading = defaultdict(list)
    for reading, surface, cost, left, right in sorted(entries):
        by_reading[reading].append((surface, cost, left, right))
    for values in by_reading.values():
        values.sort(key=lambda e: (e[1], e[0], e[2], e[3]))
    path = source / 'connection_single_column.txt'
    sources[path.relative_to(src).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    with path.open(encoding='utf-8') as stream:
        size = int(next(stream))
        original = array('h', (int(line) for line in stream))
    assert len(original) == size * size and 0 < size <= 4096
    matrix = array('h', [0]) * len(original)
    for right in range(size):
        for left in range(size):
            matrix[right + size * left] = original[right * size + left]
    readings = sorted(by_reading)
    count, _, _ = write_compact(str(target), readings, by_reading, size, size, matrix, wide=True)
    provenance = {'repository': 'https://github.com/google/mozc', 'revision': REVISION,
                  'source_sha256': sources, 'entries': count, 'readings': len(readings),
                  'skipped_non_kana_readings': skipped, 'matrix_size': size,
                  'output_sha256': hashlib.sha256(target.read_bytes()).hexdigest()}
    target.with_suffix('.sources.json').write_text(json.dumps(provenance, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(provenance, indent=2))
    print(f'{target}: {target.stat().st_size:,} bytes')


def selftest():
    import tempfile
    import struct
    with tempfile.TemporaryDirectory() as tmp:
        target = Path(tmp) / 'test.bin'
        # More than 255 entries, context IDs above 2047, and a supplementary-plane kanji.
        values = [('\U00020bb7', n, 2671, 2671) for n in range(296)]
        write_compact(str(target), ['\u3088\u3057'], {'\u3088\u3057': values}, 2672, 2672,
                      array('h', [0]) * (2672 * 2672), wide=True)
        raw = target.read_bytes()
        assert raw[:8] == b'TSIPDIC4'
        assert struct.unpack_from('<H', raw, 37)[0] == 296
        assert struct.unpack_from('<I', raw, 41)[0] == 0x20bb7
    print('Mozc pack selftest passed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path)
    parser.add_argument('--out', type=Path, default=Path('crates/core/data/ipadic_compact.bin'))
    parser.add_argument('--selftest', action='store_true')
    args = parser.parse_args()
    if args.selftest:
        selftest()
    else:
        if args.source is None:
            parser.error('--source is required')
        build(args.source, args.out)
