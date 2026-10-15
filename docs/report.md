# 성능 리포트

릴리즈(부버전 이상)마다 크기·정확도·속도·메모리를 같은 방법으로 재서 위에서부터 쌓습니다. 새 릴리즈는 맨 위에 둡니다.

## 측정 방법

```sh
cargo build --release -p typescript-tip -p typescript-cli -p typescript-config
cargo build --release -p typescript-tip --target i686-pc-windows-msvc
powershell -File scripts/package.ps1          # dist/ 에 릴리즈와 같은 zip
python lab/fetch_data.py                      # 처음 한 번 (큰 세트용 데이터, git에 안 들어감)
sh lab/report.sh > section.md                 # 이 파일에 붙일 표
```

- 크기는 릴리즈 워크플로(`release.yml`)와 같은 빌드의 산출물입니다. x64 TIP이 큰 건 같이 빌드한 `typescript-cli`가 영어·한국어 팩을 내장시키기 때문이고(기능 통합), x86 TIP은 팩 없이 따로 빌드됩니다.
- 정확도는 `lab/`의 세트입니다. `acc.*`는 단어 단위(직접 고른 목록 포함), `big.*`는 실제 문장·제목에서 만든 큰 세트입니다. `sentence`는 문장 전체가 맞아야 맞고, `token`은 단어 하나씩 셉니다. 자세한 건 `lab/src/main.rs`와 `lab/src/corpus.rs`에 있습니다.
- 속도는 결정 한 번(`commit`)과 키 입력 한 번(`keystroke`)의 마이크로초이고, 5회 중앙값입니다. 같은 컴퓨터에서도 백그라운드 작업에 따라 10~20% 흔들리므로 릴리즈 사이의 작은 차이는 잡음으로 봅니다.
- 메모리는 팩을 올린 뒤 힙(`heap.loaded_kb`)과 측정 중 최대 힙(`heap.peak_kb`)입니다.

## 0.8.6

기준 커밋 d556020(+릴리즈 준비), 2026-10-16, rustc 1.93.1. 영어 팩의 쓰지 않던 빈도 열 제거, 한국어 음절 표 공유, 코어 엔진 `opt-level = "s"` 빌드, `cd` 영어 처리가 들어간 뒤의 첫 측정입니다.

### 크기 (KiB)

| 항목                            |  KiB |
| ------------------------------- | ---: |
| `typescript_tip.dll` (x64)      | 3435 |
| `typescript_tip.dll` (x86)      |  704 |
| `typescript.exe`                | 3081 |
| `typescript-config.exe`         | 5765 |
| `typescript-windows.zip` (설치) | 5036 |
| `typescript-pack-en-0.8.6.zip`  |  446 |
| `typescript-pack-ko-0.8.6.zip`  |  387 |
| `typescript-pack-ja-0.8.6.zip`  | 4763 |

### 정확도 (%)

| 세트                           | 정확도 |
| ------------------------------ | -----: |
| `acc.ALL` (직접 고른 전체)     |  99.81 |
| `acc.en.freq/none`             |  99.60 |
| `acc.en.freq/after-ko`         |  99.45 |
| `acc.ko.words/none`            |  99.95 |
| `acc.ko.vocab/none`            | 100.00 |
| `big.ko.sent.sentence`         |  97.10 |
| `big.ko.sent.token`            |  99.45 |
| `big.en.sent.sentence`         |  99.97 |
| `big.cs.particle.sentence`     |  86.82 |
| `big.cs.particle.token`        |  98.08 |
| `big.cs.bare.sentence`         |  95.66 |
| `big.cs.number.sentence`       |  96.95 |
| `big.ko.title.none`            |  99.87 |
| `big.ko.title.after-en`        |  98.57 |
| `big.learn.other_forms_before` |  87.50 |
| `big.learn.other_forms_after`  |  98.21 |

### 속도·메모리 (중앙값, 5회)

| 항목               |       값 |
| ------------------ | -------: |
| `commit.avg_us`    |    23.85 |
| `commit.p50_us`    |    20.10 |
| `commit.p95_us`    |    51.00 |
| `commit.p99_us`    |   101.70 |
| `keystroke.avg_us` |    13.90 |
| `keystroke.p50_us` |     8.90 |
| `keystroke.p95_us` |    34.10 |
| `keystroke.p99_us` |    63.40 |
| `load.ms`          |     1.90 |
| `heap.loaded_kb`   |    66.70 |
| `heap.peak_kb`     |  7995.82 |

### 이전과 비교 (이번 작업 전 `6c5ba4d`, 같은 lab 세트)

릴리즈 전에 따로 잰 값입니다. 다음 릴리즈부터는 위와 같은 표끼리 비교합니다.

| 항목                                    |    이전 |  0.8.6 |
| --------------------------------------- | ------: | -----: |
| `big.ko.sent.sentence` (%)              |   96.83 |  97.10 |
| `big.cs.particle.sentence` (%)          |   85.78 |  86.82 |
| `big.cs.bare.sentence` (%)              |   95.20 |  95.66 |
| `big.learn.other_forms_after` (%)       |   87.50 |  98.21 |
| 로드 직후 힙 (KiB)                      |    1006 |     67 |
| 최대 힙 (MiB)                           |     9.3 |    8.0 |
| 영어 팩 (MiB)                           |    1.58 |   1.44 |
| `commit` 평균 (µs, 같은 날 같은 조건)   |    ≈19 |    ≈20 |

- 정확도는 이미 97% 안팎이던 실데이터에서 소폭 올랐고, 직접 고른 목록(letter-noun 0 → 95%)에서 크게 올랐습니다. 학습 시뮬레이션은 한 형태를 가르친 뒤 다른 활용형이 87.5%에서 98.2%로 올랐습니다.
- 메모리와 크기는 줄었고, 결정 속도는 같거나 약간 느립니다(코어 `opt-level = "s"`로 약 3%).
- 알려진 약점: `graphql을`, `mongodb를`, `kubectl하고`처럼 사전에 없는 IT 이름 뒤에 조사가 붙는 문장(`big.cs.particle.sentence`의 나머지 13%).
