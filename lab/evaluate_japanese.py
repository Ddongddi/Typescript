#!/usr/bin/env python3
"""Fetch pinned Japanese evaluation corpora and score a lab executable (no user learning).

python lab/evaluate_japanese.py --binary lab/target/release/tacit-lab.exe --out temp/ja.json
AJIMEE data: CC-BY-SA 3.0, azooKey/AJIMEE-Bench (Wikipedia JWTD test subset).
Mozc regression data: upstream BSD-3-Clause notices, google/mozc.
Raw sources and hashes stay in ignored lab/data/japanese; never training or release input.
Context-bearing AJIMEE cases are reported separately WITHOUT context, not as a full-context score.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import urllib.request

DATA = Path(__file__).parent / 'data' / 'japanese'
SOURCES = {
    'ajimee.json': ('azooKey/AJIMEE-Bench', '401666cd56d1a570c2021798b64b6da4396bfd45', 'JWTD_v2/v1/evaluation_items.json'),
    'mozc.tsv': ('google/mozc', '0136c35782536cb43d710575c311cd4185982738', 'src/data/test/quality_regression_test/oss.tsv'),
}
HASHES = {
    'ajimee.json': 'e9eb668fd6aa14b1e26436f429b5550108af0a1dfd443b8cea0bcb3ab3028fca',
    'mozc.tsv': '4ea1ad4b54cfd6ca2e4d62a0d75c922e18144afc36f1e0ceee3e09670264d5c7',
}


def fetch():
    DATA.mkdir(parents=True, exist_ok=True)
    provenance = {}
    for name, (repo, rev, path) in SOURCES.items():
        url = f'https://raw.githubusercontent.com/{repo}/{rev}/{path}'
        target = DATA / name
        if not target.exists():
            with urllib.request.urlopen(url, timeout=120) as response:
                target.write_bytes(response.read())
        digest = hashlib.sha256(target.read_bytes()).hexdigest()
        assert digest == HASHES[name], f'corrupt or unexpected pinned dataset: {target}'
        provenance[name] = {'url': url, 'sha256': digest}
    (DATA / 'sources.json').write_text(json.dumps(provenance, indent=2) + '\n', encoding='utf-8')
    return provenance


def hiragana(text):
    return ''.join(chr(ord(c) - 0x60) if '\u30a1' <= c <= '\u30f6' else c for c in text)


def distance(a, b):
    row = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        nxt = [i]
        for j, cb in enumerate(b, 1):
            nxt.append(min(nxt[-1] + 1, row[j] + 1, row[j - 1] + (ca != cb)))
        row = nxt
    return row[-1]


def expected_rank(candidates, expected):
    return next((i for i, text in enumerate(candidates, 1) if text in expected), None)


def cases():
    out = []
    for item in json.loads((DATA / 'ajimee.json').read_text(encoding='utf-8')):
        group = 'ajimee.context_omitted' if item['context_text'] else 'ajimee.no_context'
        out.append((group, hiragana(item['input']), item['expected_output'], 'exact'))
    for line in (DATA / 'mozc.tsv').read_text(encoding='utf-8').splitlines():
        if not line or line.startswith('#'):
            continue
        fields = line.split('\t')
        if len(fields) < 4:
            continue
        _, kana, expected, command = fields[:4]
        # Exact top1 and full-string regexes retain their upstream meanings.
        # Mozc's segmented N-best rank differs from this engine's whole-string rank.
        if command == 'Conversion Expected' and (len(fields) == 4 or fields[4] == '0'):
            out.append(('mozc.exact', kana, [expected], 'exact'))
        elif command in ('Conversion Match', 'Conversion Not Match'):
            out.append(('mozc.regex', kana, [expected], command))
    return out


def evaluate(binary, items):
    readings = list(dict.fromkeys(item[1] for item in items))
    path = DATA / 'readings.txt'
    path.write_text('\n'.join(readings) + '\n', encoding='utf-8')
    run = subprocess.run([str(Path(binary).resolve()), '--japanese', '--readings', str(path.resolve())],
                         text=True, encoding='utf-8', capture_output=True, check=True, timeout=180)
    got = {}
    times = []
    for line in run.stdout.splitlines():
        kana, micros, *candidates = line.split('\t')
        assert kana not in got and candidates, f'invalid result: {line}'
        got[kana] = candidates
        times.append(float(micros))
    assert set(got) == set(readings), 'missing or extra conversion results'
    groups = {}
    misses = []
    for group, kana, expected, command in items:
        candidates = got[kana]
        if command == 'exact':
            rank = expected_rank(candidates, expected)
            first = rank == 1
            top5 = rank is not None and rank <= 5
            cer = min(distance(e, candidates[0]) / max(1, len(e)) for e in expected)
        else:
            matched = re.fullmatch(expected[0], candidates[0]) is not None
            first = matched if command == 'Conversion Match' else not matched
            top5, cer = None, None
        bucket = groups.setdefault(group, {'n': 0, 'top1': 0, 'top5': 0, 'top_available': 0, 'cer': 0})
        bucket['n'] += 1
        bucket['top1'] += first
        if top5 is not None:
            bucket['top5'] += top5
            bucket['top_available'] += rank is not None
            bucket['cer'] += cer
        if not first:
            miss = {'group': group, 'reading': kana, 'expected': expected, 'actual': candidates[:5]}
            if command == 'exact':
                miss['expected_rank'] = rank
                miss['failure'] = 'ranking' if rank is not None else 'not_returned'
            misses.append(miss)
    for name, bucket in groups.items():
        for key in ['top1', 'top5', 'top_available', 'cer']:
            bucket[key] = round(100 * bucket[key] / bucket['n'], 4)
        if name == 'mozc.regex':
            del bucket['top5'], bucket['top_available'], bucket['cer']
    times.sort()
    return {'groups': groups, 'latency_us': {'n': len(times), 'mean': sum(times) / len(times),
            'p50': times[len(times) // 2], 'p95': times[min(len(times) - 1, len(times) * 95 // 100)],
            'p99': times[min(len(times) - 1, len(times) * 99 // 100)]}, 'misses': misses}


def selftest():
    assert hiragana('カタカナー') == '\u304b\u305f\u304b\u306a\u30fc'
    assert distance('abc', 'axc') == 1
    assert distance('', 'abc') == 3
    assert distance('abc', 'abc') == 0
    assert expected_rank(['a', 'b', 'c'], ['c', 'b']) == 2
    assert expected_rank(['a', 'b', 'c'], ['d']) is None
    assert expected_rank([], ['a']) is None
    print('Japanese evaluation selftest passed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', default='lab/target/release/tacit-lab.exe')
    parser.add_argument('--out', default='temp/japanese-evaluation.json')
    parser.add_argument('--selftest', action='store_true')
    parser.add_argument('--check', action='store_true', help='enforce Japanese quality floors')
    args = parser.parse_args()
    if args.selftest:
        selftest()
        return
    provenance = fetch()
    result = evaluate(args.binary, cases())
    result['sources'] = provenance
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k != 'misses'}, ensure_ascii=False, indent=2))
    if args.check:
        group = result['groups']['ajimee.no_context']
        assert group['n'] == 100 and group['top1'] >= 50 and group['top5'] >= 70 and group['cer'] <= 8, group
        group = result['groups']['mozc.exact']
        assert group['n'] == 220 and group['top1'] >= 65 and group['top5'] >= 90, group
        assert result['groups']['mozc.regex']['top1'] >= 75


if __name__ == '__main__':
    main()
