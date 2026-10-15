# Performance

측정일: 2026-10-15 / Windows / 코드 0.8.4 / 릴리스 팩 0.4.0
빌드 프로파일: release (`Cargo.toml` — LTO, `strip`, core는 opt-level 3)

> `dist/` 산출물은 팩 0.4.0 기준이라 현재 코드(0.8.4) 빌드와 수치가 다를 수 있다.
> 특히 `typescript-config.exe`는 dist 2.92MB → 현재 6.26MB로 커졌다.

## 1. 설치 크기

### 배포 파일 (`dist/`)

| 파일 | zip | 압축 풀면 |
|---|---|---|
| `typescript-windows.zip` | 4.23MB | 10.86MB |
| `typescript-pack-en-0.4.0.zip` | 0.44MB | 1.58MB |
| `typescript-pack-ko-0.4.0.zip` | 0.38MB | 1.05MB |
| `typescript-pack-ja-0.4.0.zip` | 5.93MB | 16.54MB |
| `install.ps1` / `packs.json` | 17KB / 877B | — |

`typescript-windows.zip` 안: `typescript_tip.dll` 3.68MB, `typescript.exe` 3.37MB,
`typescript-config.exe` 3.06MB, `typescript_tip_x86.dll` 0.73MB, 기타(ico, 라이선스, 설치 스크립트).

팩 압축 풀면: en = `english_levels.txt` 1.58MB + `trigrams.bin` 80KB,
ko = `korean_vocab_sorted.txt` 812KB + `korean_words_sorted.txt` 276KB + `korean_syllables.txt` 10KB,
ja = `ipadic_compact.bin` 16.12MB + `kana_trigrams.txt` 1.21MB.

### 설치 후 디스크 (추정)

| 구성 | 크기 |
|---|---|
| 기본(프로그램) | ~10.4MB |
| 기본 + en + ko (기본 언어) | ~13.0MB |
| 전부 (en + ko + ja) | ~29.5MB |

설정·학습 JSON은 수 KB. 팩 위치: `%LOCALAPPDATA%\Typescript\packs`.

## 2. 실행 파일 크기

현재 소스 release 빌드 (`target/release`):

| 바이너리 | 크기 |
|---|---|
| `typescript.exe` | 3.23MB (CLI, en+ko 내장) |
| `typescript-config.exe` | 6.26MB (대시보드 웹UI 내장) |
| `typescript_tip.dll` (x64) | 3.60MB |
| `typescript_tip.dll` (x86) | 0.76MB |

참고: debug 빌드는 exe 4.2MB / config 8.3MB / tip 5.0MB.
Android: `typescript.apk` 1.21MB, `.so` arm64 3.45MB / x86_64 3.46MB.

주의: `dist/`의 팩은 구 형식(0.4.0)이라 일본어 팩이 크다. 현행 형식(0.8.0~)은 다운로드 4.9MB·설치 10.1MB.
본 측정 후 README·문서의 구 수치 서술(DLL 1MB·일본어 팩 6MB)은 위 실측에 맞춰 고쳤다.

### 실행 메모리

| 프로세스 | WorkingSet | Private |
|---|---|---|
| CLI `--decide` 6만 줄 처리 중 | 6.75MB | 1.70MB |
| 코어 벤치 (en+ko+ja 내장) | 9.72MB | 4.68MB |

팩은 mmap으로 읽으므로 Private보다 WorkingSet에 잡힌다.
TIP 실사용은 설치 팩을 mmap해서 embedded 벤치보다 Private이 작다.

## 3. 실행 시간 (release)

| 측정 | 결과 |
|---|---|
| 프로세스 시작 + 1문장 변환 | ~28ms (release), ~38ms (debug) |
| `--decide` 2000단어 일괄 | 120~177ms → 단어당 60~89µs, 11k~16k 단어/s |
| `convert` 2000줄 일괄 | 103~127ms → 줄당 52~64µs |
| README 예문 1500문장 | 119ms → 문장당 79µs |

시작 시간에는 팩 로드·힙 초기화가 포함된다 (콜드 첫 실행 1회는 백신/페이지캐시 영향으로 수 초가 나올 수 있음).

## 4. 판단 시간 (`decide()` 1단어, in-process)

`LearningEngine::decide` 직접 호출, 워밍업 1000회 후 측정.

| 조건 | avg | p50 | p95 | p99 | max |
|---|---|---|---|---|---|
| en+ko, 확정 경로 (`partial=false`) | 16.1µs | 8.1µs | 62.5µs | 86.5µs | 218µs |
| en+ko, 타이핑 프리뷰 (`partial=true`) | 18.8µs | 10.0µs | 76.7µs | 106µs | 257µs |
| en+ko+ja, 확정 | 34.8µs | 24.1µs | 75.6µs | 110µs | 532µs |
| en+ko+ja, 프리뷰 | 37.1µs | 28.0µs | 85.1µs | 121µs | 238µs |

길이별 (확정 경로): `a` 3.9µs, `to` 5.2µs, `hello` 6.4µs,
`dkssudgktpdy`(12자) 33.8µs, 26자 85.5µs — 길이에 비례해 증가.

일본어를 켜면 약 2배. 그래도 키 입력 프레임(16ms) 대비 2~3 order 여유가 있어
판단이 타이핑 지연의 병목이 아니다.
