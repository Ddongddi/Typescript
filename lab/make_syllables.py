"""Counts the Hangul syllables of lab/data/ko_sentences.txt into crates/core/data/korean_syllable_counts.txt.

    python lab/make_syllables.py

The count prices a Korean syllable that is in no word (the language model of the code-switch
search): a rare syllable is a rare syllable, not an unknown one. Run lab/fetch_data.py first.

The sentences the lab judges (every HELD_OUT_STEP-th of the list, see lab/src/corpus.rs `spread`)
are left out, so the count never saw them. Output: "syllable<TAB>count", most common first, after one
line "~<TAB>n" with how many syllables were seen exactly once. They are left out, and the mass they
stand for is what a syllable that was never seen shares.
"""

from collections import Counter
from pathlib import Path

HERE = Path(__file__).parent
SOURCE = HERE / "data" / "ko_sentences.txt"
TARGET = HERE.parent / "crates" / "core" / "data" / "korean_syllable_counts.txt"
# 100000 sentences, 3000 judged: the lab takes every 33rd.
HELD_OUT_STEP = 33
# A syllable seen fewer times than this is noise in a corpus of this size.
MIN_COUNT = 2


def main() -> None:
    counts: Counter[str] = Counter()
    for i, line in enumerate(SOURCE.read_text(encoding="utf-8").splitlines()):
        if i % HELD_OUT_STEP == 0:
            continue
        counts.update(c for c in line if "가" <= c <= "힣")
    rows = [(s, n) for s, n in counts.most_common() if n >= MIN_COUNT]
    once = sum(1 for n in counts.values() if n == 1)
    text = f"~\t{once}\n" + "".join(f"{s}\t{n}\n" for s, n in rows)
    TARGET.write_text(text, encoding="utf-8", newline="\n")
    print(f"{TARGET.name}: {len(rows)} syllables, {sum(n for _, n in rows)} seen, {once} seen once")


if __name__ == "__main__":
    main()
