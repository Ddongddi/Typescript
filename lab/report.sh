#!/bin/sh
# Prints one report section (markdown) for docs/report.md: build sizes, pack sizes, and the lab
# metrics (accuracy; latency and memory as the median of RUNS runs). Run after packaging:
#   cargo build --release -p typescript-tip -p typescript-cli -p typescript-config
#   cargo build --release -p typescript-tip --target i686-pc-windows-msvc
#   powershell -File scripts/package.ps1
#   sh lab/report.sh > section.md
# Needs lab/data (python lab/fetch_data.py) for the big.* rows. Not part of any build.
set -e
cd "$(dirname "$0")/.."
RUNS=${RUNS:-5}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/core/Cargo.toml | head -1)
kb() { echo $(( ($(wc -c < "$1") + 512) / 1024 )); }

echo "## $version"
echo
echo "commit $(git rev-parse --short HEAD), $(date +%F), $(rustc --version)"
echo
echo "### 크기 (KiB)"
echo
echo "| 항목 | KiB |"
echo "| --- | ---: |"
for f in target/release/typescript_tip.dll target/i686-pc-windows-msvc/release/typescript_tip.dll \
  target/release/typescript.exe target/release/typescript-config.exe dist/typescript-windows.zip \
  dist/typescript-pack-en-$version.zip dist/typescript-pack-ko-$version.zip dist/typescript-pack-ja-$version.zip; do
  case $f in *i686*) arch=" (x86)" ;; *release/typescript_tip.dll) arch=" (x64)" ;; *) arch= ;; esac
  echo "| \`$(basename "$f")\`$arch | $(kb "$f") |"
done

tmp=$(mktemp -d)
i=1
while [ $i -le "$RUNS" ]; do
  cargo run -q --release --manifest-path lab/Cargo.toml -- --corpus 3000 > "$tmp/$i.txt" 2>&1
  i=$((i + 1))
done

# Median over the runs of one metric row.
metric() {
  for f in "$tmp"/*.txt; do awk -v m="$1" '$1 == m { print $2 }' "$f"; done | sort -n |
    awk '{ v[NR] = $1 } END { printf "%s", v[int((NR + 1) / 2)] }'
}

echo
echo "### 정확도 (%)"
echo
echo "| 세트 | 정확도 |"
echo "| --- | ---: |"
for m in acc.ALL acc.en.freq/none acc.en.freq/after-ko acc.ko.words/none acc.ko.vocab/none \
  big.ko.sent.sentence big.ko.sent.token big.en.sent.sentence big.cs.particle.sentence \
  big.cs.particle.token big.cs_heldout.particle.sentence big.cs.bare.sentence big.cs.number.sentence big.ko.title.none \
  big.ko.title.after-en big.learn.other_forms_before big.learn.other_forms_after; do
  echo "| \`$m\` | $(metric $m) |"
done

echo
echo "### 유형별 시나리오 (문장 정답률 %, token은 단어 단위, chain은 4문장 이어 입력)"
echo
echo "| 유형 | 문장 | token | chain.token | µs/글자 |"
echo "| --- | ---: | ---: | ---: | ---: |"
for t in chat dev writing; do
  echo "| $t | $(metric sc.$t.sentence) | $(metric sc.$t.token) | $(metric sc.$t.chain.token) | $(metric sc.$t.us_per_char) |"
done
echo "| 전체 | $(metric sc.ALL.sentence) | $(metric sc.ALL.token) | | |"
echo
echo "| 시나리오 | 문장 정답률 |"
echo "| --- | ---: |"
for f in $(awk '$1 ~ /^sc\.[a-z]+\.[a-z]+$/ && $1 !~ /\.(sentence|token)$/ { print $1 }' "$tmp/1.txt"); do
  echo "| \`${f#sc.}\` | $(metric "$f") |"
done

echo
echo "### 속도·메모리 (중앙값, $RUNS회)"
echo
echo "| 항목 | 값 |"
echo "| --- | ---: |"
for m in commit.avg_us commit.p50_us commit.p95_us commit.p99_us keystroke.avg_us keystroke.p50_us \
  keystroke.p95_us keystroke.p99_us load.ms heap.loaded_kb heap.peak_kb; do
  echo "| \`$m\` | $(metric $m) |"
done
rm -rf "$tmp"
