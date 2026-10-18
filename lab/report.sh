#!/bin/sh
# Prints profile-specific report sections: a "## 한눈에: prev → cur"
# comparison with level bars, then the "## cur" detail section. Paste both at the
# top of performance.md, replacing the previous 한눈에 block:
#   cargo build --release -p tacit-tip -p tacit-config
#   cargo build --release -p tacit-tip --target i686-pc-windows-msvc
#   powershell -File scripts/package.ps1
#   sh lab/report.sh --profile enko > section-enko.md
#   sh lab/report.sh --profile enkoja > section-enkoja.md
# Check the checked-in 한눈에 block against the sections below (no lab run needed):
#   sh lab/report.sh --selftest
# Needs lab/data (python lab/fetch_data.py) for the big.* rows. Not part of any build.
#
# Note: the scenario rows compare released sections. The 0.8.6 → 0.8.7 doc table is
# a one-off bootstrap: 0.8.6 had no scenario measurement, so it reuses the same-set
# retroactive values from that section's 0.8.6 → 0.8.7 subsection (see its footnote).
set -e
cd "$(dirname "$0")/.."
RUNS=${RUNS:-5}
PROFILE=enko
SELFTEST=false
while [ "$#" -gt 0 ]; do
  case $1 in
    --profile)
      [ "$#" -ge 2 ] || { echo '--profile requires enko or enkoja' >&2; exit 2; }
      PROFILE=$2; shift 2 ;;
    --selftest) SELFTEST=true; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case $PROFILE in
  enko) REPORT=docs/report/performance.md; LABEL='영·한' ;;
  enkoja) REPORT=docs/report/performance-enkoja.md; LABEL='영·한·일' ;;
  *) echo '--profile requires enko or enkoja' >&2; exit 2 ;;
esac
GBW=12

ACC_METRICS="acc.ALL acc.en.freq/none acc.en.freq/after-ko acc.ko.words/none acc.ko.vocab/none big.ko.sent.sentence big.ko.sent.token big.ko.phrase.sentence big.ko.long.sentence big.en.sent.sentence big.cs.particle.sentence big.cs.particle.token big.cs_heldout.particle.sentence big.cs.bare.sentence big.cs.number.sentence big.ko.title.none big.ko.title.after-en big.learn.other_forms_before big.learn.other_forms_after"
LAT_METRICS="commit.avg_us commit.p50_us commit.p95_us commit.p99_us keystroke.avg_us keystroke.p50_us keystroke.p95_us keystroke.p99_us"
MEM_METRICS="load.ms heap.loaded_kb heap.peak_kb"
if [ "$PROFILE" = enkoja ]; then
  ACC_METRICS="$ACC_METRICS ja.convert.ALL.top1 ja.convert.ALL.top5 ja.detect.none.top1 ja.detect.after-ja.top1 ja.guard.english.top1"
  LAT_METRICS="$LAT_METRICS ja.convert.avg_us ja.convert.p50_us ja.convert.p95_us ja.convert.p99_us"
  MEM_METRICS="$MEM_METRICS ja.load.ms ja.heap.loaded_kb"
fi
SC_TYPES="chat dev writing"

rep() {
  _rep_ch=$1; _rep_n=$2; _rep_out=
  while [ "$_rep_n" -gt 0 ]; do _rep_out="$_rep_out$_rep_ch"; _rep_n=$((_rep_n - 1)); done
  printf '%s' "$_rep_out"
}

# Accuracy bar: fixed 80..100 zoom so point differences show (all rows share it).
accbar() {
  _acc_n=$(awk -v v="$1" -v w="$GBW" -v lo="${2:-80}" 'BEGIN { n = int((v - lo) / (100 - lo) * w + 0.5); if (n < 0) n = 0; if (n > w) n = w; print n }')
  rep '█' "$_acc_n"
  rep '░' "$((GBW - _acc_n))"
}

# Generic bar: value against an explicit max (group max or row max).
gbar() {
  _g_n=$(awk -v v="$1" -v m="$2" -v w="$GBW" 'BEGIN { if (!(m > 0)) m = 1; n = int(v / m * w + 0.5); if (n < 0) n = 0; if (n > w) n = w; if (v > 0 && n == 0) n = 1; print n }')
  rep '█' "$_g_n"
  rep '░' "$((GBW - _g_n))"
}

# First number of the value field of the first row containing key (fixed string).
lookup() {
  awk -F'|' -v k="$1" '{ sub(/\r$/, "") } index($0, k) { print $3; exit }' "$2" 2>/dev/null |
    sed 's/^ *//; s/ .*//' | grep -E '^[0-9]+(\.[0-9]+)?$' || true
}

# Nth (1-based) "## X.Y.Z" version heading of the report, skipping "## 한눈에".
ver_at() {
  awk -v n="$1" '{ sub(/\r$/, "") } $1 == "##" && $2 ~ /^[0-9][0-9.]*$/ { c++; if (c == n) { print $2; exit } }' "$2"
}

# Body of the "## <version>" section (up to the next "## " heading).
section() {
  awk -v ver="$1" '{ sub(/\r$/, "") } /^## / { if (seen) exit; if ($2 == ver) { seen = 1; next } } seen { print }' "$2"
}

acc_delta() {
  awk -v p="$1" -v c="$2" 'BEGIN { d = c - p; s = (d >= 0 ? "+" : ""); a = (d > 0.05 ? "▲" : (d < -0.05 ? "▼" : "=")); printf "%s%.2f %s", s, d, a }'
}

lat_delta() {
  awk -v p="$1" -v c="$2" 'BEGIN { d = c - p; pct = (p != 0 ? d / p * 100 : 0); a = (d < 0 ? "▲" : "▼"); ap = (pct < 0 ? -pct : pct); if (ap < 25) a = "="; printf "%+.2f (%+.1f%%) %s", d, pct, a }'
}

size_delta() {
  awk -v p="$1" -v c="$2" 'BEGIN { d = c - p; if (d == 0) { printf "0 ="; exit } pct = (p != 0 ? d / p * 100 : 0); a = (d < 0 ? "▲" : "▼"); printf "%+.0f (%+.1f%%) %s", d, pct, a }'
}

mem_delta() {
  awk -v p="$1" -v c="$2" -v nz="$3" 'BEGIN { d = c - p; pct = (p != 0 ? d / p * 100 : 0); a = (d < 0 ? "▲" : "▼"); ap = (pct < 0 ? -pct : pct); if (ap < nz) a = "="; printf "%+.2f (%+.1f%%) %s", d, pct, a }'
}

acc_row() {
  case $1 in ja.*) _amin=0 ;; *) _amin=80 ;; esac
  if [ -z "$3" ]; then printf '| `%s` | %s | - | - |\n' "$1" "$2"; return; fi
  if [ -z "$2" ]; then printf '| `%s` | - | %s `%s` | 신규 |\n' "$1" "$3" "$(accbar "$3" "$_amin")"; return; fi
  printf '| `%s` | %s `%s` | %s `%s` | %s |\n' "$1" "$2" "$(accbar "$2" "$_amin")" "$3" "$(accbar "$3" "$_amin")" "$(acc_delta "$2" "$3")"
}

sc_row() {
  if [ -z "$3" ]; then printf '| %s | %s | - | - |\n' "$1" "$2"; return; fi
  if [ -z "$2" ]; then printf '| %s | - | %s `%s` | 신규 |\n' "$1" "$3" "$(accbar "$3")"; return; fi
  printf '| %s | %s `%s` | %s `%s` | %s |\n' "$1" "$2" "$(accbar "$2")" "$3" "$(accbar "$3")" "$(acc_delta "$2" "$3")"
}

size_crow() {
  if [ -z "$3" ]; then printf '| %s | %s | - | - |\n' "$1" "$2"; return; fi
  if [ -z "$2" ]; then printf '| %s | - | %s `%s` | 신규 |\n' "$1" "$3" "$(gbar "$3" "$4")"; return; fi
  printf '| %s | %s `%s` | %s `%s` | %s |\n' "$1" "$2" "$(gbar "$2" "$4")" "$3" "$(gbar "$3" "$4")" "$(size_delta "$2" "$3")"
}

lat_row() {
  if [ -z "$3" ]; then printf '| `%s` | %s | - | - |\n' "$1" "$2"; return; fi
  if [ -z "$2" ]; then printf '| `%s` | - | %s | 신규 |\n' "$1" "$3"; return; fi
  printf '| `%s` | %s `%s` | %s `%s` | %s |\n' "$1" "$2" "$(gbar "$2" "$4")" "$3" "$(gbar "$3" "$4")" "$(lat_delta "$2" "$3")"
}

mem_row() {
  if [ -z "$3" ]; then printf '| `%s` | %s | - | - |\n' "$1" "$2"; return; fi
  if [ -z "$2" ]; then printf '| `%s` | - | %s | 신규 |\n' "$1" "$3"; return; fi
  case $1 in *.ms) _nz=25 ;; *) _nz=1 ;; esac
  printf '| `%s` | %s `%s` | %s `%s` | %s |\n' "$1" "$2" "$(gbar "$2" "$4")" "$3" "$(gbar "$3" "$4")" "$(mem_delta "$2" "$3" "$_nz")"
}

# Pack zips carry the version in the name, so fall back to a version-free prefix.
size_prev() {
  _sp=$(lookup "$1" "$2")
  if [ -z "$_sp" ]; then
    _k=$(printf '%s' "$1" | sed 's/-[0-9][0-9.]*\.zip`$/-/')
    _sp=$(lookup "$_k" "$2")
  fi
  printf '%s' "$_sp"
}

bigger() {
  awk -v m="$1" -v a="$2" -v b="$3" 'BEGIN { if (a + 0 > m) m = a + 0; if (b + 0 > m) m = b + 0; if (!(m > 0)) m = 1; print m }'
}

print_compare() {
  echo "## 한눈에: $1 → $2"
  echo
  echo "▲ 좋아짐 · ▼ 나빠짐 · = 동점/잡음(정확도 0.05%p, 지연 25% 안은 잡음으로 봄). 정확도는 높을수록, 크기·지연·메모리는 낮을수록 좋음."
  echo
  echo "이전 열은 같은 프로필의 과거 보고서 기록입니다. 같은 lab·데이터로 교차 재측정하지 않은 지연 차이는 코드 개선의 증거로 해석하지 않습니다."
  echo
  echo "### 정확도 핵심 (%)"
  echo
  echo "막대 범위: 영·한 80~100%, 일본어(ja.*) 0~100%."
  echo
  echo "| 지표 | $1 | $2 | 변화 |"
  echo "| --- | ---: | ---: | ---: |"
  for m in $ACC_METRICS; do
    acc_row "$m" "$(lookup "\`$m\`" "$3")" "$(lookup "\`$m\`" "$4")"
  done
  echo
  echo "### 유형별 시나리오 문장 정답률 (%, 높을수록 좋음, 막대 80~100 구간)"
  echo
  echo "| 유형 | $1 | $2 | 변화 |"
  echo "| --- | ---: | ---: | ---: |"
  for t in $SC_TYPES 전체; do
    sc_row "$t" "$(lookup "| $t |" "$3")" "$(lookup "| $t |" "$4")"
  done
  echo
  echo "### 크기 비교 (KiB, 낮을수록 좋음, 막대 표 안 최대 기준)"
  echo
  echo "| 항목 | $1 | $2 | 변화 |"
  echo "| --- | ---: | ---: | ---: |"
  _smax=1
  while IFS='|' read -r _disp _cur _rest; do
    if [ -n "$_disp" ]; then
      _smax=$(bigger "$_smax" "$_cur" "$(size_prev "$_disp" "$3")")
    fi
  done < "$5"
  while IFS='|' read -r _disp _cur _rest; do
    if [ -n "$_disp" ]; then
      size_crow "$_disp" "$(size_prev "$_disp" "$3")" "$_cur" "$_smax"
    fi
  done < "$5"
  echo
  echo "### 지연 비교 (µs 중앙값, 낮을수록 좋음, 막대 표 안 최대 기준)"
  echo
  echo "| 항목 | $1 | $2 | 변화 |"
  echo "| --- | ---: | ---: | ---: |"
  _lmax=1
  for m in $LAT_METRICS; do
    _lmax=$(bigger "$_lmax" "$(lookup "\`$m\`" "$3")" "$(lookup "\`$m\`" "$4")")
  done
  for m in $LAT_METRICS; do
    lat_row "$m" "$(lookup "\`$m\`" "$3")" "$(lookup "\`$m\`" "$4")" "$_lmax"
  done
  echo
  echo "### 시작·메모리 비교 (낮을수록 좋음, 막대 행별 비교)"
  echo
  echo "| 항목 | $1 | $2 | 변화 |"
  echo "| --- | ---: | ---: | ---: |"
  for m in $MEM_METRICS; do
    _p=$(lookup "\`$m\`" "$3"); _c=$(lookup "\`$m\`" "$4")
    mem_row "$m" "$_p" "$_c" "$(bigger 1 "$_p" "$_c")"
  done
}

if [ "$SELFTEST" = true ]; then
  [ -f "$REPORT" ] || { echo "selftest: no $REPORT"; exit 1; }
  grep -F -q "프로필: \`$PROFILE\`" "$REPORT" || { echo "selftest: wrong profile in $REPORT"; exit 1; }
  _fail=0
  _cur=$(ver_at 1 "$REPORT")
  _prev=$(ver_at 2 "$REPORT")
  if [ -z "$_cur" ]; then echo "selftest: need a version section"; exit 1; fi
  _st=$(mktemp -d)
  section "$_cur" "$REPORT" > "$_st/cur.md"
  section "$_prev" "$REPORT" > "$_st/prev.md"
  if [ -z "$_prev" ]; then
    for m in $ACC_METRICS $LAT_METRICS $MEM_METRICS; do
      [ -n "$(lookup "\`$m\`" "$_st/cur.md")" ] || { echo "selftest: missing $m"; exit 1; }
    done
    rm -f "$_st/cur.md" "$_st/prev.md"
    rmdir "$_st"
    echo "selftest passed: $PROFILE first version $_cur (no previous profile baseline)"
    exit 0
  fi
  if grep -F -x -q -- "## 한눈에: $_prev → $_cur" "$REPORT"; then
    echo "ok head: $_prev → $_cur"
  else
    echo "FAIL head, want: ## 한눈에: $_prev → $_cur"; _fail=1
  fi
  _chk() {
    if [ "$2" = "$3" ]; then echo "ok $1"; else echo "FAIL $1: got [$2] want [$3]"; _fail=1; fi
  }
  _need() {
    if grep -F -x -q -- "$1" "$REPORT"; then :; else echo "FAIL row missing: $1"; _fail=1; fi
  }
  _chk accbar-86.82 "$(accbar 86.82)" '████░░░░░░░░'
  _chk accbar-97.31 "$(accbar 97.31)" '██████████░░'
  _chk accbar-100 "$(accbar 100)" '████████████'
  _chk accbar-ja-50 "$(accbar 50 0)" '██████░░░░░░'
  _chk gbar-70.8 "$(gbar 70.8 101.7)" '████████░░░░'
  _chk acc-delta "$(acc_delta 86.82 97.31)" '+10.49 ▲'
  _chk lat-delta "$(lat_delta 23.85 17.46)" '-6.39 (-26.8%) ▲'
  _chk size-delta "$(size_delta 3435 3460)" '+25 (+0.7%) ▼'
  for m in $ACC_METRICS; do
    _need "$(acc_row "$m" "$(lookup "\`$m\`" "$_st/prev.md")" "$(lookup "\`$m\`" "$_st/cur.md")")"
  done
  _lmax=1
  for m in $LAT_METRICS; do
    _lmax=$(bigger "$_lmax" "$(lookup "\`$m\`" "$_st/prev.md")" "$(lookup "\`$m\`" "$_st/cur.md")")
  done
  for m in $LAT_METRICS; do
    _need "$(lat_row "$m" "$(lookup "\`$m\`" "$_st/prev.md")" "$(lookup "\`$m\`" "$_st/cur.md")" "$_lmax")"
  done
  for m in $MEM_METRICS; do
    _p=$(lookup "\`$m\`" "$_st/prev.md"); _c=$(lookup "\`$m\`" "$_st/cur.md")
    _need "$(mem_row "$m" "$_p" "$_c" "$(bigger 1 "$_p" "$_c")")"
  done
  sed -n '/^### 크기/,/^### /p' "$_st/cur.md" | grep '^| `' | sed 's/^| //; s/ | [0-9].*//' > "$_st/disps.txt"
  _smax=1
  while IFS= read -r _d; do
    [ -n "$_d" ] || continue
    _smax=$(bigger "$_smax" "$(lookup "$_d" "$_st/cur.md")" "$(size_prev "$_d" "$_st/prev.md")")
  done < "$_st/disps.txt"
  while IFS= read -r _d; do
    [ -n "$_d" ] || continue
    _need "$(size_crow "$_d" "$(size_prev "$_d" "$_st/prev.md")" "$(lookup "$_d" "$_st/cur.md")" "$_smax")"
  done < "$_st/disps.txt"
  for t in $SC_TYPES 전체; do
    if [ -n "$(lookup "| $t |" "$_st/cur.md")" ]; then
      echo "ok sc.$t present"
    else
      echo "FAIL sc.$t missing in $_cur"; _fail=1
    fi
  done
  rm -f "$_st/cur.md" "$_st/prev.md" "$_st/disps.txt"
  rmdir "$_st"
  if [ "$_fail" -ne 0 ]; then echo "selftest FAILED"; exit 1; fi
  echo "selftest passed: $_prev → $_cur"
  exit 0
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/core/Cargo.toml | head -1)
kb() { echo $(( ($(wc -c < "$1") + 512) / 1024 )); }

size_row() {
  case $1 in *i686*) _arch=" (x86)" ;; *release/tacit_tip.dll) _arch=" (x64)" ;; *) _arch= ;; esac
  _disp="\`$(basename "$1")\`$_arch"
  _kb=$(kb "$1")
  printf '| %s | %s |\n' "$_disp" "$_kb"
  printf '%s|%s\n' "$_disp" "$_kb" >> "$tmp/sizes.txt"
}

case $RUNS in ''|*[!0-9]*) echo 'RUNS must be a positive integer' >&2; exit 2 ;; esac
[ "$RUNS" -gt 0 ] || { echo 'RUNS must be a positive integer' >&2; exit 2; }
tmp=$(mktemp -d)
i=1
while [ $i -le "$RUNS" ]; do
  if ! cargo run -q --release --manifest-path lab/Cargo.toml -- --profile "$PROFILE" --corpus 3000 > "$tmp/$i.txt" 2>&1; then
    cat "$tmp/$i.txt" >&2; exit 1
  fi
  awk -v metrics="$ACC_METRICS $LAT_METRICS $MEM_METRICS" '
    { seen[$1] = 1 }
    END { n = split(metrics, required, " "); for (j = 1; j <= n; j++) if (!seen[required[j]]) {
      print "missing metric: " required[j] > "/dev/stderr"; failed = 1
    } exit failed }
  ' "$tmp/$i.txt"
  i=$((i + 1))
done

# Median over the runs of one metric row.
metric() {
  for f in "$tmp"/*.txt; do awk -v m="$1" '$1 == m { print $2 }' "$f"; done | sort -n |
    awk '{ v[NR] = $1 } END { printf "%s", v[int((NR + 1) / 2)] }'
}

SIZE_FILES="target/release/tacit_tip.dll target/i686-pc-windows-msvc/release/tacit_tip.dll target/release/tacit-config.exe dist/tacit-windows.zip dist/tacit-pack-en-$version.zip dist/tacit-pack-ko-$version.zip"
if [ "$PROFILE" = enkoja ]; then SIZE_FILES="$SIZE_FILES dist/tacit-pack-ja-$version.zip"; fi

{
echo "## $version"
echo
revision=$(git rev-parse --short HEAD)
if ! git diff --quiet -- crates/core lab; then revision="$revision (작업 트리 변경 포함)"; fi
echo "commit $revision, $(date +%F), $(rustc --version)"
echo
echo "프로필: \`$PROFILE\` ($LABEL), lab $(sed -n 's/^version = "\(.*\)"/\1/p' lab/Cargo.toml | head -1)."
echo
echo '공통 정확도·시나리오는 해당 언어가 켜진 판별기로 측정합니다. acc.ALL은 영·한 공통 세트이며 일본어 정답률과 합산하지 않습니다.'
echo
echo 'load.ms·heap.*는 판별기 시작/단어 평가 기준입니다. enkoja의 ja.load.ms·ja.heap.loaded_kb는 별도 가나→한자 변환기 초기화/힙 증가입니다. 설치 zip은 공통이며 팩 목록만 활성 언어에 따라 다릅니다.'
echo
echo "### 크기 (KiB)"
echo
echo "| 항목 | KiB |"
echo "| --- | ---: |"
for f in $SIZE_FILES; do
  size_row "$f"
done

echo
echo "### 정확도 (%)"
echo
echo "| 세트 | 정확도 |"
echo "| --- | ---: |"
for m in $ACC_METRICS; do
  echo "| \`$m\` | $(metric $m) |"
done

echo
echo "### 유형별 시나리오 (문장 정답률 %, token은 단어 단위, chain은 4문장 이어 입력)"
echo
echo "| 유형 | 문장 | token | chain.token | µs/글자 |"
echo "| --- | ---: | ---: | ---: | ---: |"
for t in $SC_TYPES; do
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
for m in $LAT_METRICS $MEM_METRICS; do
  echo "| \`$m\` | $(metric $m) |"
done
} > "$tmp/cur.md"

prev=
if [ -f "$REPORT" ]; then prev=$(ver_at 1 "$REPORT"); fi
if [ "$prev" = "$version" ]; then prev=$(ver_at 2 "$REPORT"); fi
if [ -n "$prev" ] && [ "$prev" != "$version" ]; then
  section "$prev" "$REPORT" > "$tmp/prev.md"
  print_compare "$prev" "$version" "$tmp/prev.md" "$tmp/cur.md" "$tmp/sizes.txt"
  echo
fi
cat "$tmp/cur.md"
rm -f "$tmp"/*.txt "$tmp/cur.md" "$tmp/prev.md"
rmdir "$tmp"
