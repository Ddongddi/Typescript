# 영·한·일 성능 리포트

프로필: `enkoja` (영·한·일). [영·한 성능 리포트](performance.md)와 기준선·이력을 분리합니다.

영어·한국어·일본어를 모두 켜고 공통 단어·문장·시나리오·사용자 학습 평가를 실행합니다.
`acc.ALL`, `big.*`, `sc.*`는 영·한 공통 세트의 결과이므로, 일본어 활성화가 기존 입력에
미치는 영향을 볼 수 있습니다. 일본어 판별·가나→한자 변환은 `ja.*`로 따로 표시하며
서로 다른 세트의 정확도를 하나로 평균내지 않습니다. chat/dev/writing 시나리오는
영·한 시나리오이며 일본어 문장 평가는 자체 64건을 사용합니다. AJIMEE·Mozc 외부
평가와 한계는 [일본어 개선 보고서](japanese-improvement.md)를 참고합니다.

프로그램·설치 zip은 영·한과 같고, 설치하는 언어 팩에는 일본어가 추가됩니다.
`load.ms`, `heap.*`는 판별기 시작 및 공통 단어 평가 기준이며, `ja.load.ms`,
`ja.heap.loaded_kb`는 그 뒤 별도로 초기화하는 가나→한자 변환기의 시간과 힙 증가입니다.
이 값은 실제 앱의 전체 working set이나 키 입력부터 화면 반영까지의 지연이 아닙니다.

```sh
sh lab/report.sh --profile enkoja > section-enkoja.md
sh lab/report.sh --profile enkoja --selftest
cargo run --release --manifest-path lab/Cargo.toml -- --profile enkoja --save
```

출력을 아래 최신 버전 절로 넣습니다. 이전 영·한·일 측정이 있을 때만 같은 프로필의
직전 버전과 비교합니다. 기준선은 `lab/baseline-enkoja.tsv`이며, 영·한 기준선을 대신
사용하지 않습니다. 아래 수치는 동일 프로필 5회 실행의 중앙값입니다.

## 0.10.2

commit 53325c6 (작업 트리 변경 포함), 2026-10-18, rustc 1.93.1 (01f6ddf75 2026-02-11)

프로필: `enkoja` (영·한·일), lab 0.2.0.

공통 정확도·시나리오는 해당 언어가 켜진 판별기로 측정합니다. acc.ALL은 영·한 공통 세트이며 일본어 정답률과 합산하지 않습니다.

load.ms·heap.*는 판별기 시작/단어 평가 기준입니다. enkoja의 ja.load.ms·ja.heap.loaded_kb는 별도 가나→한자 변환기 초기화/힙 증가입니다. 설치 zip은 공통이며 팩 목록만 활성 언어에 따라 다릅니다.

### 크기 (KiB)

| 항목 | KiB |
| --- | ---: |
| `tacit_tip.dll` (x64) | 957 |
| `tacit_tip.dll` (x86) | 771 |
| `tacit-config.exe` | 3277 |
| `tacit-windows.zip` | 2386 |
| `tacit-pack-en-0.10.2.zip` | 446 |
| `tacit-pack-ko-0.10.2.zip` | 450 |
| `tacit-pack-ja-0.10.2.zip` | 14628 |

### 정확도 (%)

| 세트 | 정확도 |
| --- | ---: |
| `acc.ALL` | 99.52 |
| `acc.en.freq/none` | 98.85 |
| `acc.en.freq/after-ko` | 98.80 |
| `acc.ko.words/none` | 100.00 |
| `acc.ko.vocab/none` | 100.00 |
| `big.ko.sent.sentence` | 99.03 |
| `big.ko.sent.token` | 99.81 |
| `big.ko.phrase.sentence` | 99.37 |
| `big.ko.long.sentence` | 97.70 |
| `big.en.sent.sentence` | 99.90 |
| `big.cs.particle.sentence` | 99.06 |
| `big.cs.particle.token` | 99.85 |
| `big.cs_heldout.particle.sentence` | 98.95 |
| `big.cs.bare.sentence` | 96.94 |
| `big.cs.number.sentence` | 99.05 |
| `big.ko.title.none` | 99.87 |
| `big.ko.title.after-en` | 99.37 |
| `big.learn.other_forms_before` | 100.00 |
| `big.learn.other_forms_after` | 100.00 |
| `ja.convert.ALL.top1` | 85.94 |
| `ja.convert.ALL.top5` | 96.88 |
| `ja.detect.none.top1` | 100.00 |
| `ja.detect.after-ja.top1` | 100.00 |
| `ja.guard.english.top1` | 100.00 |

### 유형별 시나리오 (문장 정답률 %, token은 단어 단위, chain은 4문장 이어 입력)

| 유형 | 문장 | token | chain.token | µs/글자 |
| --- | ---: | ---: | ---: | ---: |
| chat | 96.51 | 98.78 | 99.17 | 4.64 |
| dev | 98.98 | 99.69 | 99.37 | 6.81 |
| writing | 98.78 | 99.77 | 99.75 | 6.89 |
| 전체 | 98.12 | 99.50 | | |

| 시나리오 | 문장 정답률 |
| --- | ---: |
| `chat.brand` | 100.00 |
| `chat.casual` | 100.00 |
| `chat.english` | 100.00 |
| `chat.loan` | 92.31 |
| `chat.number` | 100.00 |
| `chat.punct` | 100.00 |
| `chat.slang` | 88.24 |
| `dev.commit` | 100.00 |
| `dev.error` | 100.00 |
| `dev.identifier` | 100.00 |
| `dev.issue` | 100.00 |
| `dev.review` | 100.00 |
| `dev.terminal` | 96.00 |
| `dev.tool` | 100.00 |
| `writing.academic` | 92.86 |
| `writing.email` | 100.00 |
| `writing.english` | 100.00 |
| `writing.essay` | 100.00 |
| `writing.formal` | 100.00 |
| `writing.long` | 100.00 |
| `writing.numbers` | 100.00 |
| `writing.quote` | 100.00 |

### 속도·메모리 (중앙값, 5회)

| 항목 | 값 |
| --- | ---: |
| `commit.avg_us` | 55.90 |
| `commit.p50_us` | 47.70 |
| `commit.p95_us` | 128.80 |
| `commit.p99_us` | 199.80 |
| `keystroke.avg_us` | 26.87 |
| `keystroke.p50_us` | 16.50 |
| `keystroke.p95_us` | 81.60 |
| `keystroke.p99_us` | 140.10 |
| `ja.convert.avg_us` | 230.13 |
| `ja.convert.p50_us` | 200.60 |
| `ja.convert.p95_us` | 420.60 |
| `ja.convert.p99_us` | 498.90 |
| `load.ms` | 24.60 |
| `heap.loaded_kb` | 2468.13 |
| `heap.peak_kb` | 14047.55 |
| `ja.load.ms` | 3.34 |
| `ja.heap.loaded_kb` | 2998.60 |
