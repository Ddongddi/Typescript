"""Downloads the lab's text corpora into lab/data/ (git-ignored, never part of an installer).

    python lab/fetch_data.py

Sources (fetched from their own sites, not redistributed by this repo; check each license):
  NSMC            https://github.com/e9t/nsmc                  CC0, informal movie-review sentences
  Chatbot_data    https://github.com/songys/Chatbot_data        everyday question/answer sentences
  Tatoeba         https://tatoeba.org                           CC BY 2.0 FR, ko + en sentences
  kowiki titles   https://dumps.wikimedia.org/kowiki            CC BY-SA, noun-like words

Output (one item per line): ko_sentences.txt, en_sentences.txt, ko_titles.txt,
  ko_phrases.txt (multi-word kowiki titles), ko_long.txt (40+ char Korean sentences)
"""

import bz2
import csv
import gzip
import io
import random
import re
import urllib.request
from pathlib import Path

DATA = Path(__file__).parent / "data"
SEED = 7

HANGUL = re.compile(r"^[가-힣 ]+$")
# Hangul + ASCII letters/digits and the punctuation people type; nothing exotic.
KO_SENTENCE = re.compile(r"^[가-힣A-Za-z0-9 ,.?!'~]+$")
EN_SENTENCE = re.compile(r"^[A-Za-z0-9 ,.?!']+$")


def get(url: str) -> bytes:
    print("fetch", url)
    request = urllib.request.Request(url, headers={"User-Agent": "typescript-ime-lab/0.0 (corpus fetch)"})
    with urllib.request.urlopen(request, timeout=120) as r:
        return r.read()


def clean(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def ko_ok(s: str) -> bool:
    return 6 <= len(s) <= 70 and bool(KO_SENTENCE.match(s)) and bool(re.search(r"[가-힣]", s))


def nsmc() -> list[str]:
    raw = get("https://raw.githubusercontent.com/e9t/nsmc/master/ratings_train.txt")
    rows = raw.decode("utf-8").splitlines()[1:]
    return [clean(r.split("\t")[1]) for r in rows if r.count("\t") >= 2]


def chatbot() -> list[str]:
    raw = get("https://raw.githubusercontent.com/songys/Chatbot_data/master/ChatbotData.csv")
    out = []
    for row in csv.reader(io.StringIO(raw.decode("utf-8"))):
        out += [clean(c) for c in row[:2]]
    return out[2:]


def tatoeba(lang: str) -> list[str]:
    url = f"https://downloads.tatoeba.org/exports/per_language/{lang}/{lang}_sentences.tsv.bz2"
    rows = bz2.decompress(get(url)).decode("utf-8").splitlines()
    return [clean(r.split("\t")[2]) for r in rows if r.count("\t") >= 2]


_TITLES_CACHE: list[str] | None = None


def titles_raw() -> list[str]:
    global _TITLES_CACHE
    if _TITLES_CACHE is None:
        url = "https://dumps.wikimedia.org/kowiki/latest/kowiki-latest-all-titles-in-ns0.gz"
        rows = gzip.decompress(get(url)).decode("utf-8").splitlines()
        _TITLES_CACHE = [r.replace("_", " ") for r in rows]
    return _TITLES_CACHE


def titles() -> list[str]:
    words = set(titles_raw())
    return sorted(w for w in words if HANGUL.match(w) and " " not in w and 2 <= len(w) <= 6)


def phrases() -> list[str]:
    out = set()
    for w in titles_raw():
        if len(w) > 30 or not HANGUL.match(w):
            continue
        parts = w.split(" ")
        if 2 <= len(parts) <= 4 and all(1 <= len(p) <= 8 for p in parts):
            out.add(w)
    return sorted(out)


def write(name: str, items: list[str], limit: int) -> None:
    rng = random.Random(SEED)
    items = sorted(set(items))
    rng.shuffle(items)
    (DATA / name).write_text("\n".join(items[:limit]) + "\n", encoding="utf-8")
    print(f"{name}: {min(len(items), limit)} of {len(items)}")


def main() -> None:
    DATA.mkdir(exist_ok=True)
    ko = [s for s in nsmc() + chatbot() + tatoeba("kor") if ko_ok(s)]
    en = [s for s in tatoeba("eng") if 8 <= len(s) <= 70 and EN_SENTENCE.match(s)]
    write("ko_sentences.txt", ko, 100000)
    write("en_sentences.txt", en, 100000)
    write("ko_titles.txt", titles(), 100000)
    write("ko_phrases.txt", phrases(), 30000)
    write("ko_long.txt", [s for s in ko if len(s) >= 40], 10000)


if __name__ == "__main__":
    main()
