#!/usr/bin/env python3
"""Compare full-list and live-preview search on pinned external readings and every prefix.

python lab/compare_japanese_preview.py --binary lab/target/release/tacit-lab.exe --out temp/preview.json
Uses the same pinned, hash-checked sources as evaluate_japanese.py; no training or user data.
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess

from evaluate_japanese import DATA, cases, fetch


def run(binary, path, best):
    command = [str(Path(binary).resolve()), '--japanese', '--readings', str(path.resolve())]
    if best:
        command.append('--best')
    process = subprocess.run(command, check=True, capture_output=True, text=True,
                             encoding='utf-8', timeout=180)
    results = {}
    for line in process.stdout.splitlines():
        kana, micros, *candidates = line.split('\t')
        assert kana not in results and candidates, f'invalid output: {line}'
        results[kana] = (float(micros), candidates)
    return results


def latency(results, readings):
    values = sorted(results[kana][0] for kana in readings)
    return {'mean': statistics.mean(values), 'p95': values[min(len(values) - 1, int(len(values) * .95))]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--runs', type=int, default=7)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1:
        parser.error('--runs must be positive')
    sources = fetch()
    whole = list(dict.fromkeys(item[1] for item in cases()))
    prefixes = list(dict.fromkeys(kana[:end] for kana in whole for end in range(1, len(kana) + 1)))
    path = DATA / 'preview-readings.txt'
    path.write_text('\n'.join(prefixes) + '\n', encoding='utf-8')
    samples = {'whole': {'full': [], 'best': []}, 'prefixes': {'full': [], 'best': []}}
    # Warm both paths; alternate order to reduce temperature and scheduling bias.
    for best in (False, True):
        run(args.binary, path, best)
    for index in range(args.runs):
        pair = {}
        for best in ((False, True) if index % 2 == 0 else (True, False)):
            pair[best] = run(args.binary, path, best)
        assert set(pair[False]) == set(prefixes) == set(pair[True]), 'missing or extra readings'
        for kana in prefixes:
            assert pair[True][kana][1] == pair[False][kana][1][:1], f'top-1 changed: {kana}'
        for name, readings in [('whole', whole), ('prefixes', prefixes)]:
            for best, label in [(False, 'full'), (True, 'best')]:
                samples[name][label].append(latency(pair[best], readings))
        print(f'run {index + 1}/{args.runs}: {len(prefixes)} identical top-1 results', flush=True)
    summary = {}
    for name, modes in samples.items():
        summary[name] = {'n': len(whole if name == 'whole' else prefixes)}
        for label, runs in modes.items():
            summary[name][label + '_us'] = {
                key: statistics.median(run[key] for run in runs) for key in ('mean', 'p95')}
        summary[name]['reduction_percent'] = {
            key: 100 * (1 - summary[name]['best_us'][key] / summary[name]['full_us'][key])
            for key in ('mean', 'p95')}
    report = {'runs': args.runs, 'summary': summary, 'samples_us': samples, 'sources': sources}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
