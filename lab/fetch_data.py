"""Downloads the lab's text corpora into lab/data/ (git-ignored, never part of an installer).

    python lab/fetch_data.py

Sources (fetched from their own sites, not redistributed by this repo; check each license):
  NSMC            https://github.com/e9t/nsmc                  CC0, informal movie-review sentences
  Chatbot_data    https://github.com/songys/Chatbot_data        everyday question/answer sentences
  Tatoeba         https://tatoeba.org                           CC BY 2.0 FR, ko + en sentences
  kowiki titles   https://dumps.wikimedia.org/kowiki            CC BY-SA, noun-like words

Output (one item per line): ko_sentences.txt, en_sentences.txt, ko_titles.txt
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


def titles() -> list[str]:
    url = "https://dumps.wikimedia.org/kowiki/latest/kowiki-latest-all-titles-in-ns0.gz"
    rows = gzip.decompress(get(url)).decode("utf-8").splitlines()
    words = {r.replace("_", " ") for r in rows}
    return sorted(w for w in words if HANGUL.match(w) and " " not in w and 2 <= len(w) <= 6)


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
    write("ko_sentences.txt", ko, 60000)
    write("en_sentences.txt", en, 30000)
    write("ko_titles.txt", titles(), 60000)


if __name__ == "__main__":
    main()
