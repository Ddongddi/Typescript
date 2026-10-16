#!/usr/bin/env python3
"""Release score v2 (10000 = every metric at its target). Not part of any build.

    python lab/score.py metrics.json        # {"v0.8.7": {"acc.ALL": 99.78, ...}, ...} in release order

Each release is scored against the one before it (L회귀). Missing accuracy sets are left out
and the accuracy weights renormalised, so a release measured on fewer sets still scores.
"""
import json
import sys
from math import exp, log

SCALE = 10000
A = dict(acc=0.40, size=0.38, speed=0.08, mem=0.08)  # 영역 지수
KAPPA = 1.5

# 정확도 묶음: 묶음 가중치, {세트: 묶음 안 가중치}
ACC = {
    'everyday_sentence': (50, {'big.ko.sent.sentence': 1, 'big.ko.sent.token': .5, 'big.en.sent.sentence': 1,
                               'big.ko.title.none': 1, 'big.ko.title.after-en': 1}),
    'everyday_list': (20, {'acc.ALL': 1, 'acc.en.freq/none': 1, 'acc.en.freq/after-ko': 1,
                           'acc.ko.words/none': 1, 'acc.ko.vocab/none': 1}),
    'it_particle': (10, {'big.cs.particle.sentence': 1, 'big.cs.particle.token': .5,
                         'big.cs.bare.sentence': 1, 'big.cs.number.sentence': 1}),
    'heldout': (8, {'big.cs_heldout.particle.sentence': 1}),
    'scenario': (6, {'scenario.all.sentence': 1}),
    'learn': (6, {'big.learn.other_forms_before': 1, 'big.learn.other_forms_after': 1}),
}
ACC_T = 0.1  # 오류율 목표(%)
PCT = (.2, .2, .3, .3)  # avg, p50, p95, p99
LAT_T = 10  # µs
SIZE = {'size.zip': (.4, 3072), 'size.x64': (.2, 2048), 'size.x86': (.2, 512), 'size.packs': (.2, 512)}
MEM = {'heap.peak_kb': (.4, 4096), 'heap.loaded_kb': (.3, 16), 'load.ms': (.3, 1)}

BUDGET = {'keystroke.p99_us': 300, 'commit.p99_us': 300, 'size.zip': 6144, 'size.x64': 4096,
          'size.x86': 900, 'size.packs': 900, 'heap.peak_kb': 16384, 'load.ms': 10,
          'err:acc.ALL': 0.5, 'err:big.ko.sent.sentence': 5, 'err:big.en.sent.sentence': 1,
          'err:big.cs_heldout.particle.sentence': 10}
REG_ACC = ['acc.ALL', 'big.ko.sent.sentence', 'big.en.sent.sentence', 'big.cs.particle.sentence',
           'big.cs_heldout.particle.sentence', 'scenario.all.sentence']


def terms(m):
    """지표별 (x, T, 계수 c). 측정되지 않은 세트는 빼고 정확도 계수를 재정규화한다."""
    out = {}
    grp = {g: (w, {k: v for k, v in s.items() if k in m}) for g, (w, s) in ACC.items()}
    grp = {g: (w, s) for g, (w, s) in grp.items() if s}
    gsum = sum(w for w, _ in grp.values())
    for g, (w, s) in grp.items():
        ssum = sum(s.values())
        for k, v in s.items():
            out[k] = (100 - m[k], ACC_T, A['acc'] * w / gsum * v / ssum)
    for kind, wk in (('keystroke', .6), ('commit', .4)):
        for p, pw in zip(('avg', 'p50', 'p95', 'p99'), PCT):
            k = f'{kind}.{p}_us'
            out[k] = (m[k], LAT_T, A['speed'] * wk * pw)
    for k, (w, T) in SIZE.items():
        out[k] = (m[k], T, A['size'] * w)
    for k, (w, T) in MEM.items():
        out[k] = (m[k], T, A['mem'] * w)
    return out


def l_rate(m):
    return sum(c * log(max(x, T) / T) for x, T, c in terms(m).values())


def l_budget(m):
    s = 0
    for k, B in BUDGET.items():
        if k.startswith('err:'):
            if k[4:] not in m:
                continue
            x = 100 - m[k[4:]]
        else:
            x = m[k]
        s += log(max(1, x / B)) ** 2
    return s


def l_reg(m, ref):
    if ref is None:
        return 0
    # 기준에 없던 지표(r == 0, 예: 팩이 없던 시절의 팩 크기)는 나빠질 수 없으니 0
    lat = lambda x, r, tau, sig: max(0, log(x / r) - log(1 + tau)) / sig if r > 0 else 0
    z = {}
    ks = [k for k in REG_ACC if k in m and k in ref]
    z['acc'] = sum((max(0, (ref[k] - m[k]) - 0.05) / 0.5) ** 2 for k in ks) / len(ks)
    ls = [f'{a}.{p}_us' for a in ('keystroke', 'commit') for p in ('p95', 'p99')]
    z['speed'] = sum(lat(m[k], ref[k], .25, log(2)) ** 2 for k in ls) / 4
    z['size'] = sum(lat(m[k], ref[k], .02, log(1.5)) ** 2 for k in SIZE) / 4
    z['mem'] = (lat(m['load.ms'], ref['load.ms'], .25, log(2)) ** 2
                + lat(m['heap.loaded_kb'], ref['heap.loaded_kb'], .05, log(2)) ** 2
                + lat(m['heap.peak_kb'], ref['heap.peak_kb'], .05, log(2)) ** 2) / 3
    tot = sum(A.values())
    return KAPPA * sum(A[k] / tot * z[k] for k in z)


def parts(m, ref=None):
    return l_rate(m), l_budget(m), l_reg(m, ref)


def score(m, ref=None):
    return SCALE * exp(-sum(parts(m, ref)))


def main(path):
    releases = json.load(open(path))
    ref = None
    for tag, m in releases.items():
        lr, lb, lg = parts(m, ref)
        print(f'{tag}\t{lr:.4f}\t{lb:.4f}\t{lg:.4f}\t{SCALE * exp(-(lr + lb + lg)):.0f}')
        ref = m


if __name__ == '__main__':
    main(sys.argv[1])
