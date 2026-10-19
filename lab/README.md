# 성능 측정 lab

## 한·영 후보 누락 진단

```powershell
cargo run --release --manifest-path lab/Cargo.toml -- --profile enko --audit-enko --fold 0/3 --corpus 3000 --misses 12
# 기존 탐색 생략 조건만 해제하는 실험: 기본 제품 동작에는 영향 없음
$env:LAB_ENKO_FULL_SEARCH = '1'
lab/target/release/tacit-lab.exe --profile enko --audit-enko --fold 0/3 --corpus 3000 --misses 12
lab/target/release/tacit-lab.exe --profile enko --fold 0/3 --corpus 3000 --misses 3 --steady
Remove-Item Env:LAB_ENKO_FULL_SEARCH
```

학습 모델을 지정하지 않은 `enko`에서 실행합니다. `python lab/fetch_data.py`로 준비한
영어·한국어 문장과 기존 합성 혼합 세트를 사용합니다. 일반 lab 측정에 진단 비용을
섞지 않도록 별도 실행합니다. [결과와 분류 정의](../docs/report/enko-candidate-audit.md).

`offline-exact`는 기존 오른쪽 문맥 보정을 포함한 정답 문장 수입니다.
`left-to-right-exact`는 그 보정 없이 실제 앞선 판별 결과의 언어를 전달한 정답 문장 수이며,
아래 원인별 수치는 이 경로의 **토큰 개수**입니다. 실시간 미리보기나 OS 세션 이벤트를
재생하는 검사는 아닙니다. `pure-selection`에는 점수와 정책에 의한 언어 선택 오류가
포함됩니다. 혼합 경로는 기존 간선·전이로 정답 출력을 구성할 수 있는지 검사하고,
`skip-gate`(생략만 없애면 선택)와 `skip-and-selection`(선택 점수도 바뀌어야 함)을 구분합니다.

## 한·영 학습 실험

기본 엔진은 그대로 두고 lab에서만 학습 모델을 비교합니다. 모델 파일은 제품 빌드나
설치 파일에 포함되지 않습니다. 실험 결과와 채택 여부는
[한·영 실험 기록](../docs/report/enko-experiments.md)에 남깁니다.

```powershell
# 기존 튜너의 원형 단어 기준 분할 감사: 같은 단어의 변형은 같은 fold에 배정
cargo run --release --manifest-path lab/Cargo.toml -- --audit-tune
# 사전·조사 목록 없이 키 n-gram과 언어 전이를 학습하는 linear-chain CRF
cargo run --release --manifest-path lab/Cargo.toml -- --fit-enko temp/model.bin
# 모델·학습 횟수는 같게 두고, 합성 혼합어의 한국어 부분을 문자 경계에서 잘라 학습
cargo run --release --manifest-path lab/Cargo.toml -- --fit-enko temp/model-spans.bin --augment-spans
$env:LAB_ENKO_MODEL = (Resolve-Path temp/model.bin).Path
lab/target/release/tacit-lab.exe --profile enko --fold 0/3 --corpus 3000 --misses 3 --steady
Remove-Item Env:LAB_ENKO_MODEL
```

먼저 `python lab/fetch_data.py`로 학습용 문장 자료를 준비합니다. 학습은 영어·한국어
문장과 한국어 댓글의 행 fold `1/3`을 사용합니다. 여기서 원형 단어의 안정적인 해시로
내부 검증 집합을 분리하고, 합성 혼합어는 학습 쪽 단어에서만 만듭니다. 문장 행 분할은
어휘·문서·출처의 완전한 독립성을 보장하지 않습니다. 개발 비교는 `0/3`, 최종 기본 엔진
확인은 `2/3`으로 구분하며, 내부 단어 정확도만으로 모델을 채택하지 않습니다.

`LAB_ENKO_MODEL`은 공통 단어·문장·시나리오·학습 평가에 모두 적용됩니다. `enkoja`와
함께 사용하면 실패합니다. 이 모델의 confidence는 최적 태그 경로의 조건부 확률이며,
보정된 정답 확률이 아닙니다. 기존 엔진과의 confidence 수치 직접 비교에는 쓰지 않습니다.

`--tune`의 검증 분할은 이전 행 순서 기준에서 원형 단어 기준으로 바뀌었습니다.
이전 튜너 점수와 직접 비교하지 않습니다. 일반 lab 평가 집합과 지표는 바뀌지 않았습니다.

제품 빌드와 분리된 Rust 실행 파일입니다. 입력 판별·변환의 정확도, 지연,
힙 사용량과 학습 효과를 측정합니다. 아래 명령은 저장소 루트에서 실행합니다.

## 영·한 / 영·한·일

```sh
cargo run --release --manifest-path lab/Cargo.toml -- --profile enko
cargo run --release --manifest-path lab/Cargo.toml -- --profile enkoja
```

| 프로필 | 활성 언어 | 평가 | 기준선 | 릴리즈 보고서 |
| --- | --- | --- | --- | --- |
| `enko` (기본값) | 영어·한국어 | 공통 단어·문장·시나리오·학습 | `baseline-enko.tsv` | [영·한](../docs/report/performance.md) |
| `enkoja` | 영어·한국어·일본어 | 같은 공통 평가 + 일본어 판별·변환 | `baseline-enkoja.tsv` | [영·한·일](../docs/report/performance-enkoja.md) |

공통 평가와 학습 실험도 실제 활성 언어로 실행합니다. 따라서 `enkoja`의 공통
정확도는 일본어를 켰을 때 영·한 입력이 얼마나 유지되는지를 보여 줍니다.
일본어 지표는 `ja.*`로 분리하며 `acc.ALL`과 합산하지 않습니다.

## 데이터와 기준선

```sh
python lab/fetch_data.py  # 큰 문장·제목 세트 준비, lab/data/에 저장
cargo run --release --manifest-path lab/Cargo.toml -- --profile enko --save
cargo run --release --manifest-path lab/Cargo.toml -- --profile enkoja --save
```

`--save`는 해당 프로필의 기준선만 덮어씁니다. 예전 `baseline.tsv`는
`baseline-enko.tsv`로 옮겼습니다. 다른 프로필의 기준선을 대신 사용하지 않습니다.
일반 실행은 기준선과 차이·고친 사례·새로 틀린 사례를 출력합니다. lab 버전이
다르면 경고하므로, 릴리즈 비교는 같은 lab·데이터·옵션으로 다시 측정합니다.

큰 데이터가 없으면 일반 lab 실행은 `big.*` 평가를 건너뜁니다. 릴리즈 report는
필수 지표가 빠지면 실패합니다. 단어·시나리오와 자체 일본어 사례는 저장소에 있습니다.

```sh
# 큰 세트를 제외하고 빠르게 확인
cargo run --release --manifest-path lab/Cargo.toml -- --profile enkoja --no-corpus --misses 5
# 큰 세트 표본 수 / 개발·검증 분할: K/N은 0부터 세며 서로 다른 fold는 겹치지 않음
cargo run --release --manifest-path lab/Cargo.toml -- --profile enko --corpus 3000 --fold 0/3
```

## 릴리즈 보고서 생성

Windows에서는 먼저 PowerShell에서 제품 산출물을 만들고 패키징합니다.
CLI를 함께 빌드하면 Cargo의 기능 통합으로 팩이 DLL에 내장될 수 있으므로 아래처럼 빌드합니다.

```powershell
cargo build --release -p tacit-tip -p tacit-config
cargo build --release -p tacit-tip --target i686-pc-windows-msvc
./scripts/package.ps1
```

그다음 Git Bash 등 `sh`·`awk`가 있는 셸에서 각각 실행합니다.

```sh
sh lab/report.sh --profile enko > section-enko.md
sh lab/report.sh --profile enkoja > section-enkoja.md
# 기본 5회 실행의 중앙값. 개발 중 빠른 확인에만 RUNS=1 사용
RUNS=1 sh lab/report.sh --profile enkoja > preview-enkoja.md
```

출력을 해당 프로필 보고서에 넣습니다. 기존 `## 한눈에`와 같은 버전 절은
교체하고 이전 버전 이력은 남깁니다. 비교 대상은 같은 프로필의 이전 버전이며,
첫 영·한·일 보고서에는 영·한 수치를 이전 값으로 가져오지 않습니다.
이전 열은 과거 기록이므로, 지연 개선을 판단하려면 같은 lab·데이터로 두 버전을
교차 재측정합니다. 커밋하지 않은 코드가 포함되면 보고서의 커밋 표기에 표시됩니다.

```sh
sh lab/report.sh --profile enko --selftest
sh lab/report.sh --profile enkoja --selftest
```

`--selftest`는 lab을 재실행하지 않고 중앙값 계산과 문서의 지표·비교 표를 확인합니다.
짝수 횟수로 측정하면 가운데 두 값의 평균을 사용합니다. CI에서도 두 프로필의
자체 검사와 lab 전체 테스트를 실행합니다.
보고서의 프로그램·설치 zip은 공통이며, 팩 크기 목록만 프로필별로 다릅니다.

## 지표의 범위

| 지표 | 의미 |
| --- | --- |
| `acc.*`, `chars.sentences` | 공통 세트의 정답률·문자 정확도. `acc.ALL`은 일본어 변환을 포함하지 않음 |
| `big.*` | 실제 문장·제목과 합성 혼합 입력, 학습 일반화 실험 |
| `sc.*` | 영·한 chat/dev/writing 시나리오. `chain`은 4문장 연속 입력 |
| `preview.flicker.*` | 영어 단어 입력 중 두 번째 키부터 영어 외 판별로 바뀐 비율 |
| `commit.*`, `keystroke.*` | 판별 한 번 / 입력 중 접두어 판별의 지연(µs) |
| `load.ms`, `heap.*` | 판별기 시작 및 공통 단어 평가의 시간·힙. 큰 코퍼스·일본어 변환기 메모리는 제외 |
| `ja.convert.*` | 자체 일본어 64건의 Top-1·Top-5와 변환 지연 |
| `ja.detect.*`, `ja.guard.english.*` | 로마자 일본어 판별과 영어 보존 확인 |
| `ja.load.ms`, `ja.heap.loaded_kb` | 별도 가나→한자 변환기 초기화 시간·힙 증가 |

힙 수치는 프로세스 전체 메모리나 mmap 사전 페이지의 working set이 아닙니다.
지연은 엔진 호출 시간이며 실제 앱 화면 반영까지의 시간은 아닙니다. 작은 지연
차이는 시스템 부하에 흔들리므로, 다른 빌드·테스트와 동시에 측정하지 않습니다.

## 일본어 외부 평가와 검증

```sh
cargo build --release --manifest-path lab/Cargo.toml
python lab/evaluate_japanese.py --check --out temp/japanese-evaluation.json
python lab/evaluate_japanese.py --selftest
cargo test --manifest-path lab/Cargo.toml
python lab/compare_japanese_preview.py --binary lab/target/release/tacit-lab.exe --out temp/preview.json
```

외부 평가는 Windows 실행 파일 경로가 기본값입니다. Linux/macOS에서는
`--binary lab/target/release/tacit-lab`을 추가합니다. AJIMEE·Mozc의 고정 자료를
`lab/data/japanese/`에 캐시하고 SHA-256을 확인합니다. 평가 자료는 학습이나
배포 사전 생성에 사용하지 않습니다. 문맥이 있는 AJIMEE 사례도 현재 평가는
왼쪽 문맥을 생략한 결과임을 따로 표시합니다.

`top_available`은 반환 후보 안에 정답이 있는 비율이고, 오답의 `expected_rank`와
`failure`는 순위 문제(`ranking`)와 반환 목록에 없는 경우(`not_returned`)를 구분합니다.
후보에 없다는 사실만으로 사전 누락이라고 판단하지 않습니다.

`compare_japanese_preview.py`는 같은 고정 자료의 읽기와 모든 접두어에 대해
전체 후보 목록 / 라이브 Top-1 경로를 순서를 번갈아 7회 비교합니다. 첫 후보가
다르면 실패하며, 실행별 평균·p95의 중앙값과 원시 측정치를 JSON으로 저장합니다.
Linux/macOS에서는 `--binary lab/target/release/tacit-lab`을 사용합니다. CI는
`--runs 1`로 출력 일치를 검사하며 성능 수치로 통과 여부를 결정하지 않습니다.
별도 읽기 파일에는 `--japanese --readings PATH --best`로 Top-1만 출력합니다.

lab 자체 버전 변경 규칙과 이력은 [CHANGELOG.md](CHANGELOG.md)에 있습니다.
`score.py`의 종합 점수 규격은 [기존 영·한 평가](../docs/report/score.md)용입니다.
