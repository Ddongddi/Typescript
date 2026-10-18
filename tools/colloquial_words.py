#!/usr/bin/env python3
"""Adds the one-syllable words that informal Korean says and Tatoeba does not (걍, 캬, 잭) to
crates/core/data/korean_words.txt, the list of words "seen in real text".

    python tools/colloquial_words.py

korean_words.txt comes from Tatoeba sentences, which are written, not said. A word missing from it
gets no floor in the decision (engine.rs, `korean_score`), so an English word with the same keys
wins: `rid` stays English though it is 걍. The words come from NSMC's *test* half (CC0, movie
reviews; fetched here, not kept). The lab (lab/fetch_data.py) tests on NSMC's train half, so no
sentence of the lab is a source of a word.

A word is added when it is said 5 times or more in the 50,000 reviews, is not in the list, and,
if its keys are also an English word, is said in Korean more often than that English word is,
scaled by LAMBDA: the share of English tokens that Korean text may carry. `do` (애) stays English:
a few Korean 애 do not outweigh the English `do`; `rid` (걍) turns Korean: nobody says `rid`.

Each word is added with count 1: only that the word exists matters (the floor); the counts the
scores of whole words were fitted to stay as they are. `WORDS_TOTAL` in korean.rs is the sum of the
counts and is updated with them.
"""

import collections
import re
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "crates" / "core" / "data"
KOREAN_RS = ROOT / "crates" / "core" / "src" / "lang" / "korean.rs"
NSMC = "https://raw.githubusercontent.com/e9t/nsmc/master/ratings_test.txt"
MIN_COUNT = 5
LAMBDA = 0.1

# Dubeolsik: initial, vowel and final of a syllable as the keys that type them.
INITIAL = "r R s e E f a q Q t T d w W c z x v g".split()
VOWEL = "k o i O j p u P h hk ho hl y n nj np nl b m ml l".split()
FINAL = ["", "r", "R", "rt", "s", "sw", "sg", "e", "f", "fr", "fa", "fq", "ft", "fx", "fv", "fg",
         "a", "q", "qt", "t", "T", "d", "w", "c", "z", "x", "v", "g"]


def keys(word: str) -> str:
    out = ""
    for ch in word:
        i = ord(ch) - 0xAC00
        out += INITIAL[i // 588] + VOWEL[i % 588 // 28] + FINAL[i % 28]
    return out


def main() -> None:
    request = urllib.request.Request(NSMC, headers={"User-Agent": "tacit-ime/0.0"})
    reviews = urllib.request.urlopen(request, timeout=120).read()
    said = collections.Counter()
    for line in reviews.decode("utf-8").splitlines()[1:]:
        fields = line.split("\t")
        if len(fields) >= 3:
            said.update(re.findall(r"[가-힣]+", fields[1]))
    tokens = sum(said.values())

    english, english_total = {}, 0
    for line in (DATA / "english_frequency.txt").read_text(encoding="utf-8").splitlines():
        fields = line.split()
        if len(fields) > 1 and fields[1].isdigit():
            english.setdefault(fields[0].lower(), int(fields[1]))
            english_total += int(fields[1])

    path = DATA / "korean_words.txt"
    text = path.read_text(encoding="utf-8")
    listed = {line.split("\t")[0] for line in text.splitlines() if line}
    added, kept_english = [], []
    for word, n in said.items():
        if len(word) != 1 or n < MIN_COUNT or word in listed:
            continue
        rival = english.get(keys(word).lower())
        if rival is not None and n / tokens <= LAMBDA * rival / english_total:
            kept_english.append(word)
        else:
            added.append(word)
    added.sort()

    if not text.endswith("\n"):
        text += "\n"
    path.write_text(text + "".join(f"{w}\t1\n" for w in added), encoding="utf-8", newline="\n")

    lines = path.read_text(encoding="utf-8").splitlines()
    total = sum(int(line.split("\t")[1]) for line in lines if line)
    rs = KOREAN_RS.read_text(encoding="utf-8")
    patched = re.sub(
        r"const WORDS_TOTAL: f64 = [\d_]+\.0;", f"const WORDS_TOTAL: f64 = {total:,}.0;".replace(",", "_"), rs
    )
    KOREAN_RS.write_text(patched, encoding="utf-8", newline="")
    print(f"added {len(added)} words, left to English: {''.join(sorted(kept_english))}; WORDS_TOTAL = {total}")


if __name__ == "__main__":
    main()
