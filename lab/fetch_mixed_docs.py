"""Freeze mixed Korean/English prose from primary documentation, without running its code.

python lab/fetch_mixed_docs.py
python lab/fetch_mixed_docs.py --selftest

Sources retain their own licenses; raw pages and extracted prose stay in ignored lab/data/.
Train/dev are different Kubernetes documents. Python documents are a sealed final set.
The manifest records URLs and hashes. Cached HTML is reused, not silently refreshed.
"""

import hashlib
import json
import re
import sys
from html.parser import HTMLParser

from fetch_data import DATA, clean, get

KUBE = "https://kubernetes.io/ko/docs/concepts/"
SOURCES = {
    "train": [KUBE + path + "/" for path in (
        "workloads/pods", "workloads/pods/init-containers", "workloads/pods/pod-lifecycle",
        "workloads/controllers/deployment", "services-networking/service",
        "configuration/configmap", "configuration/secret", "storage/persistent-volumes",
    )],
    "dev": [KUBE + path + "/" for path in (
        "workloads/controllers/statefulset", "workloads/controllers/daemonset",
    )],
    "final": ["https://docs.python.org/ko/3.12/tutorial/" + path + ".html"
              for path in ("classes", "controlflow")],
}


class Prose(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack = []
        self.root = None
        self.paragraph = None
        self.parts = []
        self.items = []

    def handle_starttag(self, tag, attrs):
        if tag in {"br", "hr", "img", "input", "meta", "link", "source", "wbr"}:
            if tag == "br" and self.paragraph is not None:
                self.parts.append(" ")
            return
        self.stack.append(tag)
        attrs = dict(attrs)
        if self.root is None and (tag in {"main", "article"} or attrs.get("role") == "main"):
            self.root = len(self.stack)
        if tag == "p" and self.root is not None and not any(t in self.stack for t in ("pre", "nav", "script", "style")):
            self.paragraph = len(self.stack)
            self.parts = []

    def handle_endtag(self, tag):
        if tag not in self.stack:
            return
        depth = len(self.stack) - self.stack[::-1].index(tag)
        if self.paragraph == depth:
            text = clean("".join(self.parts))
            if re.search(r"[가-힣]", text) and re.search(r"[A-Za-z]", text):
                self.items.append(text)
            self.paragraph = None
        if self.root == depth:
            self.root = None
        del self.stack[depth - 1:]

    def handle_data(self, data):
        if self.paragraph is not None and not any(t in self.stack for t in ("script", "style")):
            self.parts.append(data)


def extract(html):
    parser = Prose()
    parser.feed(html)
    return parser.items


def main():
    target = DATA / "mixed-docs"
    target.mkdir(parents=True, exist_ok=True)
    manifest = []
    seen = set()
    sets = {}
    for split, urls in SOURCES.items():
        lines = []
        for i, url in enumerate(urls):
            cache = target / f"{split}-{i:02}.html"
            if not cache.exists():
                cache.write_bytes(get(url))
            raw = cache.read_bytes()
            paragraphs = extract(raw.decode("utf-8"))
            if not paragraphs:
                raise ValueError(f"no mixed prose in {url}")
            kept = []
            for text in paragraphs:
                identity = text.casefold()
                if identity not in seen:
                    seen.add(identity)
                    kept.append(text)
            lines.extend(kept)
            manifest.append({"split": split, "url": url, "html_sha256": hashlib.sha256(raw).hexdigest(),
                             "paragraphs": len(kept), "text_sha256": hashlib.sha256("\n".join(kept).encode()).hexdigest()})
        (target / f"{split}.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
        sets[split] = {line.casefold() for line in lines}
        print(split, len(lines), "mixed paragraphs")
    assert not (sets["train"] & sets["dev"] or sets["train"] & sets["final"] or sets["dev"] & sets["final"])
    (target / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")


def selftest():
    html = '<nav><p>바깥 outside</p></nav><main><p>파드 <code>Pod</code>는 <a>API</a>를 사용한다.</p><pre><p>코드 code</p></pre><p>한글만</p></main><footer><p>꼬리 footer</p></footer>'
    assert extract(html) == ["파드 Pod는 API를 사용한다."]
    assert extract('<div role="main"><p>한글 <em>English</em><br>문장</p></div>') == ["한글 English 문장"]
    print("mixed-docs extraction selftest passed")


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        selftest()
    elif sys.argv[1:]:
        raise SystemExit("usage: fetch_mixed_docs.py [--selftest]")
    else:
        main()
