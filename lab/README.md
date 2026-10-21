# 성능 측정 lab

## 공통 후보 선택기·실제 세션 전환 실험

```powershell
New-Item -ItemType Directory -Force temp/ranker | Out-Null
cargo run --release --manifest-path lab/Cargo.toml -- --fit-ranker temp/ranker/complete.bin
lab/target/release/tacit-lab.exe --fit-ranker temp/ranker/prefix.bin --prefix-training
lab/target/release/tacit-lab.exe --fit-ranker temp/ranker/context.bin --prefix-training --context-augmentation
lab/target/release/tacit-lab.exe --fit-ranker temp/ranker/dual.bin --prefix-training --separate-heads
# 기존 엔진의 실제 세션 기준선: 자동완성 없음, 오른쪽 문맥 보정 없음
lab/target/release/tacit-lab.exe --profile enko --switching --fold 0/3 --corpus 300
$env:LAB_ENKO_RANKER = (Resolve-Path temp/ranker/prefix.bin).Path
try {
    lab/target/release/tacit-lab.exe --profile enko --fold 0/3 --corpus 3000 --steady
    lab/target/release/tacit-lab.exe --profile enko --switching --fold 0/3 --corpus 300
    # 같은 모델을 입력 중에만 쓰는 구조 비교; 확정 판별은 기존 엔진
    $env:LAB_ENKO_RANKER_PREVIEW_ONLY = '1'
    lab/target/release/tacit-lab.exe --profile enko --switching --fold 0/3 --corpus 300
} finally {
    Remove-Item Env:LAB_ENKO_RANKER -ErrorAction SilentlyContinue
    Remove-Item Env:LAB_ENKO_RANKER_PREVIEW_ONLY -ErrorAction SilentlyContinue
}
```

선택기는 영어·한글·기존 혼합어 최상위 해석·기존 엔진의 결과를 같은 softmax로 비교한다.
72개 가중치를 학습하며 단어 식별자나 평가 정답을 특징으로 사용하지 않는다.
`--separate-heads`는 입력 중·완성 단어에 독립적인 144개 가중치를 학습한다.
기존 사전·혼합어 전이 제약·기존 엔진의 판단은 후보와 특징에 남아 있다.
혼합어는 N-best 전체가 아니므로 후보 도달률을 따로 보고한다.
모델 confidence는 후보 집합 안의 조건부 확률이며 정답 확률로 보정되지 않았다.
모델과 실행 경로는 lab 전용이고 제품에는 포함하지 않는다. `enko`만 허용하며,
CRF·전체 탐색 생략 해제 실험과 함께 쓰거나 제품 baseline을 `--save`할 수 없다.

학습은 기존 자료의 행 fold 1/3만 사용하고, 다른 fold와 정규화 후 같은 문장은 제외한다.
키열의 대소문자를 정규화한 해시로 내부 검증을 분리하며, 합성 혼합어와 접두어도
전체 키열·각 언어 구간이 같은 분할에 속할 때만 사용한다.
이는 문서·출처·형태소 원형의 완전한 독립성을 보장하지 않는다.
`--context-augmentation`은 모든 학습 사례에 이전 언어 없음·영어·한국어를 동일하게
적용하는 개발 실험이며, 실제 혼합 문맥 자료를 대체하지 않는다.

`--prefix-training`은 모든 키 접두어와 완성 철자의 미확정 상태도 학습한다(lab 0.2.10부터).
실제 세션에서 공백 전까지는 완성 철자에도 `partial=true`가 전달되기 때문이다.

혼합 문서 실험은 `python lab/fetch_mixed_docs.py`로 고정 자료와 출처/hash manifest를 만든다.
`--mixed-training lab/data/mixed-docs/train.txt`를 학습 명령에 추가하고,
`--switching --switching-input lab/data/mixed-docs/dev.txt --corpus 300`으로 별도 문서를 재생한다.
`final.txt`는 다른 출처의 최종 평가용이며 개발 중 사용하지 않는다. 원문·모델은 git에서 제외한다.
수집기는 `python lab/fetch_mixed_docs.py --selftest`로 검사할 수 있다.

`--switching`은 실제 InputSession에 키·구두점·확정 이벤트를 재생한다.
세 코퍼스와 기존 chat/dev/writing 시나리오를 사용하며 기존 정확도 지표는 바꾸지 않는다.
`switch.wrong_preview_pct`는 의도한 현재 단어의 조합과 표시가 다른 키 비율이다.
`switch.changes_per_1000_keys`에는 정상적인 혼합어 전환도 포함된다.
확정 문장 정확도와 함께 해석해야 하며, 낮은 전환 횟수만으로 개선을 주장하지 않는다.
FakeTextField 기반의 추가 입력 재생이므로 TSF 화면 반영·삭제·커서 이동은 측정하지 않는다.
`switch.errors.*`는 마지막 Trace와 정답·실제 표시를 비교한 원인별 키 수다.
`presentation`은 Trace 출력은 맞는데 표시가 다른 경우이며, 정책·재해석을 더 구분하지 않는다.
`split_or_stale_trace`는 Trace의 키열이 현재 평가 토큰과 다른 경우라 판별 오류로 단정하지 않는다.
`--misses N`으로 미리보기와 확정 오류 상세를 제한한다.

## 세션 미리보기·자동완성 지연

```powershell
cargo run --release --manifest-path lab/Cargo.toml -- --profile enko --session
lab/target/release/tacit-lab.exe --profile enkoja --session
```

`history.N.*`는 합성 사용자 기록 N개에서 접두어별 탐색 지연이고,
`session.N.*`는 키 입력의 `InputSession.handle()`과 `suggestion()`을 합친 지연입니다.
각각 워밍업 후 평균·p50·p95·p99를 µs로 출력합니다. 기록은 영어·한글을 섞어 생성하며
언어 판별 학습에는 사용하지 않습니다. 임시 파일은 초기 로드 후 삭제합니다.
단어 확정·디스크 저장·OS 화면 반영·일본어 실시간 한자 변환은 측정하지 않습니다.
기존 `keystroke.*`는 계속 판별 함수만 측정하므로 두 지표를 혼용하지 않습니다.
개선 전후 실행 파일을 번갈아 최소 3회 실행하고 각 지표의 중앙값을 비교합니다.
정확도는 별도로 기존 단어·문장·시나리오·일본어 평가와 코퍼스 fold를 확인합니다.

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

lab 0.2.11부터 `alloc.commit.*`, `alloc.keystroke.*`는 판별 한 번당 성공한 할당 요청 수와
요청 바이트의 평균이다. 판별 결과를 담는 보고서 배열의 할당은 제외하며, `bytes`는 해제된 임시
버퍼도 포함하는 누적 요청량으로 상주 메모리가 아니다. `heap.loaded_kb`와 `heap.peak_kb`도 함께 본다.
할당 계수의 atomic 비용이 있으므로 같은 lab 버전의 전후 실행 순서를 번갈아 반복 비교한다.

lab 0.2.12의 `--switching`은 `switch.alloc.calls_per_key`, `switch.alloc.bytes_per_key`로
`InputSession::handle`의 문자 키 처리 내부 할당을 센다. 정답 조합·자료 준비·보고서는 제외한다.
`--switching`은 현재 영·한 전용이며 일본어 기능은 일반 enkoja 평가와 세션 테스트로 검증한다.
짧은 시나리오의 지연은 표본이 작으므로 기존 문장 전체를 동일 횟수 반복한 파일을
`--switching-input PATH --corpus N`으로 재생할 수 있다. 반복 문장을 독립 정확도 표본으로 세지 않는다.
