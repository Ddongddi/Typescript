# Typescript

영어/한국어(두벌식) 자동 판별 입력기. Windows(TSF)와 Android(물리 키보드). 일본어(로마자)는 **일본어 포함판**에서만 된다 (아래 "일본어").

```
dkssudgktpdy, hello world!          →  안녕하세요, hello world!
dlrjs githubdp commitgkf rjdi       →  이건 github에 commit할 거야
fltpssm ghkdlxld                    →  리센느 화이팅
kyouhaiitenkidesune ⏎               →  きょうはいいてんきですね   (일본어를 켰을 때)
```
## 설치
PowerShell에서 한 줄이면 최신 릴리스를 받아 설치하고 입력기까지 켠다 (관리자 권한 확인창 1회).
```powershell
irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1 | iex
```
판이 둘이다. 기본판은 영어/한국어만 들어 있어 가볍다 (약 3.5MB). **일본어 포함판**은 영어/한국어/일본어이고, 일본어 모델과 IPADIC 한자 사전(16MB) 때문에 약 21MB다. 일본어 포함판은 `-Japanese` 로 설치하고, 기본판으로 돌아가려면 그 스위치 없이 다시 설치한다.
```powershell
$i = [scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))
& $i -Japanese             # 영어 + 한국어 + 일본어
```
받은 zip은 릴리스의 SHA-256과 맞는지 확인한 뒤에만 설치한다. 제거와 버전 지정은 스크립트 블록으로 부른다.
```powershell
$i = [scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))
& $i -Uninstall            # 제거 (설정은 %APPDATA%\Typescript 에 남는다)
& $i -Version v0.2.0       # 특정 버전
```
소스에서 직접 빌드해 설치하려면 (Rust와 `rustup target add i686-pc-windows-msvc` 필요):
```powershell
.\scripts\install.ps1            # 빌드(x64 + x86) → 관리자 권한 요청(UAC 1회) → 등록
.\scripts\install.ps1 -Japanese  # 일본어 포함판 (Cargo 기능 `japanese`)
.\scripts\install.ps1 -Uninstall # 제거
```
설치 후 **Win+Space** 로 입력기를 고른다. 언어 목록에서는 **영어(미국)** 아래에 있다 (없으면 설치가 영어(미국)을 추가한다). 한국어 아래에 두면 카카오톡 같은 IMM32 앱에서 Windows가 조합을 한 글자로 잘라 버려서다 (아래 "앱 호환"). 두 가지가 있다.

| 입력기 | 하는 일 |
| --- | --- |
| `Typescript (자동완성)` | 영어/한국어 자동 판별 + Tab 자동완성 + 쓴 단어 학습 |
| `Typescript` | 자동 판별만 (자동완성도, 단어 기록도 없다) |

이미 떠 있는 앱은 다시 열어야 새 입력기를 읽는다. 아이콘은 `assets/icon.png` 를 바꾸고 `.\scripts\make-icon.ps1` 후 다시 설치하면 된다.

### Android
릴리스의 `typescript.apk` 를 받아 설치한다 (출처를 알 수 없는 앱 허용 필요). 설정 → 키보드 목록에서 **Typescript** 를 켜고, 입력창에서 키보드로 고른다. 화면 키보드(안드로이드 기본 `KeyboardView`에 두벌식 자모를 키마다 같이 표시)와 블루투스/USB 물리 키보드 둘 다 된다. 한/영 판별, 밑줄 미리보기, Esc 순환은 Windows와 같은 코어가 처리한다. 자동완성(Tab)과 단어 학습은 Android에는 없다 (폰 키보드에 이미 있는 기능이라 뺐다).
화면 키보드에서는 `가/A` 키가 Esc(말하는 중인 단어 또는 방금 친 단어의 언어 순환), `한/영` 키가 일시정지(켜져 있으면 `OFF` 표시)다. 비밀번호 칸과 텍스트가 아닌 칸에서는 판별 없이 글자만 넣는다.
```powershell
.\platforms\android\build.ps1                 # platforms\android\build\typescript.apk (arm64 + x86_64)
.\platforms\android\build.ps1 -Abi arm64-v8a  # 폰용만
adb install -r platforms\android\build\typescript.apk
```
Android SDK(build-tools, platform 34), NDK, JDK 17, `rustup target add aarch64-linux-android x86_64-linux-android`, `cargo install cargo-ndk` 가 필요하다. Gradle은 쓰지 않는다. 릴리스 APK는 고정된 릴리스 키로 서명돼서 새 버전을 덮어 설치할 수 있다 (키는 GitHub Secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`). 직접 빌드하면 디버그 키로 서명되므로 릴리스 APK와는 덮어 설치되지 않는다. 직접 빌드를 릴리스 키로 서명하려면 `TS_KEYSTORE`, `TS_KEYSTORE_PASS` 환경변수에 키스토어 경로와 비밀번호를 준다 (별칭은 `typescript`).

## 쓰는 법

| 키 | 동작 |
| --- | --- |
| 글자 | 단어를 이어 간다. 점선 밑줄로 현재 판단을 보여 준다 |
| Space, 구두점 | 단어를 확정하고 키는 앱이 그대로 받는다. 일본어 단어에서는 Space가 변환을 연다 (아래 "일본어") |
| Enter | 밑줄 단어를 확정만 한다 (다음 Enter가 앱의 것). 일본어 단어는 어느 설정에서도 확정만 한다 |
| Esc | 읽기를 순환: 영어 → 영어+조사 → 한국어 → 일본어(켰을 때) |
| Shift+Esc | 어느 앱에서나 읽기를 순환 |
| 확정 직후 Esc | 방금 확정한 단어를 다음 읽기로 바꾼다. 같은 교정을 두 번 하면 학습한다 |
| Backspace | 영어는 글자 단위, 한국어는 두벌식처럼 자모 → 음절 단위, 일본어는 가나 한 글자 |
| ⌃K / F7, ⌃J / F6 | (일본어) 읽기를 가타카나 / 히라가나로 보여 준다. Space로 둘을 뒤집고 Enter로 확정한다 |
| Tab | (자동완성 입력기) 쓰는 중인 단어를 자주 쓴 단어로 완성한다. 내가 친 단어가 먼저고, 없으면 내장 사전이다. 완성될 단어는 단어 아래의 작은 알약 모양 창에 `안녕하세요  Tab`으로 미리 보인다 (앱 테마에 맞춰 라이트/다크). 완성할 게 없으면 단어만 확정하고 Tab은 앱으로 간다 |
| **한/영 키** | 이 텍스트 필드에서 Typescript를 끄고 켠다 (다른 칸으로 가면 다시 켜진다) |

## 일본어 (일본어 포함판)

일본어 포함판에서는 기본으로 켜져 있고, `settings.json` 의 `"japanese": false` 로 끌 수 있다. 기본판에는 일본어 모델이 없어서 설정과 상관없이 동작하지 않는다. 공백 없이 친 로마자를 단어마다 일본어로 읽을지 판단해 가나로 보여 주고 (점선 밑줄), Esc 순환과 확정 규칙은 영어/한국어와 같다.

```
kyouhaiitenkidesune ⏎   →  今日はいい天気ですね
kyouhagoogledekensaku ⏎ →  今日はgoogleで検索   (문구 안 영어 단어는 그대로)
```

- Space가 변환을 연다: 후보를 Space/↓(다음), ↑(이전), 1–9, Enter(확정)로 고르고, Esc/Backspace는 가나 편집으로 돌아간다. ←/→는 첫 구절의 경계를 늘리고 줄인다.
- **한자 변환**은 IPADIC 사전(39만 항목 + 연결 비용, NAIST 라이선스)으로 하고, 단어 비용은 wordfreq 빈도로 보정한다 (사전 데이터 ~16MB가 DLL에 함께 실리고 쓰일 때만 읽힌다). Space 후보로 전체 읽기의 여러 경로·첫 구절 경계·가나/카타카나가 나오고, 고른 후보는 읽기별로 학습한다 (`%APPDATA%\Typescript\kanji_learning.json`, 읽기당 5개).
- 품질은 Mozc·Apple 엔진보다 낮다: 첫 후보가 틀리면 Space로 넘기면 되고(예: `ほん` → ホン, 本 순서), 같은 교정을 두 번 하면 다음부터 먼저 나온다.
- `"liveConversion": true` 면 치는 동안 첫 후보를 바로 보여 준다 (macOS판 라이브 변환: `きょうは…` → 今日は…).
- `"fullWidthDigits"`, `"fullWidthSpace"` (둘 다 기본 꺼짐): 일본어 뒤의 숫자와 공백을 전각으로 (０３－１２３４, 　). `apps.json` 의 `"fullWidth": false` 로 그 앱에서만 반각으로 둘 수 있다 (터미널).
- 문구 안에서 `, . ? ! -` 는 、。？！ー 로 붙고, 일본어 뒤의 기호는 스스로 한 단어가 되어 Return이나 다음 단어가 확정한다.
- 사전 데이터를 다시 만들려면 `tools/build_ipadic.py` (IPADIC 원본과 wordfreq zipf JSON이 필요하다 — 도구 설명 참고).

## 설정 (`%APPDATA%\Typescript\`)

JSON을 직접 편집한다. 앱 간 전환할 때 다시 읽는다.

`settings.json`
```json
{ "returnConfirmsOnly": true, "pauseKey": "hangul", "autocomplete": true,
  "japanese": false, "liveConversion": false, "fullWidthDigits": false, "fullWidthSpace": false }
```
`autocomplete`: `false`면 `Typescript (자동완성)` 에서도 Tab 자동완성과 단어 기록을 끈다.
`pauseKey`: `"hangul"`(한/영 키) 또는 `"off"`.
`japanese`: 로마자 일본어 자동 판별 (위 "일본어"). 기본값은 판에 따라 다르다 (일본어 포함판 `true`, 기본판 `false` 이고 기본판에서는 켜도 무시된다). `liveConversion`, `fullWidthDigits`, `fullWidthSpace` 도 거기서 설명한다.

**단어 학습** — 자동완성 입력기에서 확정한 영어/한글 단어를 `user_history.json` 에 센다. **2번 이상 친 단어만** 제안한다 (한 번 친 비밀번호·오타는 나오지 않는다). 10단어마다, 그리고 입력칸을 옮길 때 파일에 쓰고 다시 읽으므로 다른 앱에서 배운 단어도 곧 제안된다. 숫자나 영한 섞인 토큰, 1글자는 기록하지 않는다.

`apps.json` — 키는 **exe 파일 이름**(대소문자 무시). `"*"` 는 기본값이고, 앱 항목에는 다른 키만 적는다.
```json
{ "*": { "escapeToApp": false },
  "windowsterminal.exe": { "conversionThreshold": 0.97 },
  "game.exe": { "enabled": false } }
```
- `enabled: false` 키를 그대로 통과 (영문 입력)
- `conversionThreshold` 기본 0.5. 0.97(엄격)은 터미널에서 짧은 명령이 한글로 바뀌는 것을 막는다
- `escapeToApp: true` Esc를 앱에 넘기고 Shift+Esc로 읽기를 바꾼다 (에디터)
- `useSystemIme: true` 그 앱에서는 시스템 한국어 입력기로 넘긴다 (한/영 키로 전환. 아래 "앱 호환")
- `fullWidth: false` (일본어) 그 앱에서는 전각 숫자·공백을 반각으로 둔다 (터미널)

`lexicon.json` — 교정으로 배운 단어 (직접 고쳐도 된다).

## 앱 호환

- **카카오톡, RichEdit 기반 앱 (IMM32 앱)**: Windows는 TSF 입력기의 조합을 IMM32 메시지로 바꿔 주는데, *한국어* 입력기에는 "조합은 한 글자(interim)" 규칙을 적용해 나머지를 바로 확정한다. 단어 전체를 미리보기로 보여 주는 이 입력기는 한국어로 등록하면 키마다 끊겼다 (`테스트` → `ㅌpㅅmㅌm`). 그래서 **영어(미국)로 등록한다**. 그러면 같은 앱에서도 단어 전체가 하나의 조합으로 유지된다. 또 IMM32 앱은 입력기가 "가져가겠다"고 한 키를 돌려받지 못하므로, 단어를 끝내고 앱으로 넘어가는 키(Space, 구두점, Enter, Tab, 방향키)는 TSF가 키를 물어보는 단계에서 바로 처리한다.
- 그래도 시스템 한국어 입력기를 쓰고 싶은 앱은 `apps.json` 에 `{ "앱.exe": { "useSystemIme": true } }` 로 그 프로세스만 Microsoft 한국어 IME로 넘길 수 있다 (다른 앱으로 가면 되돌아온다).
- 문제를 볼 때는 `%APPDATA%\Typescript\debug.on` 파일을 만들면 같은 폴더의 `debug.log` 에 이벤트 이름(포커스 이동, 조합 시작, 앱이 조합을 끝냄)만 남는다. 친 글자는 기록하지 않는다. 확인이 끝나면 파일을 지운다.
- `cargo run -p typescript-tip --example richedit_lab -- --ts` (또는 `--ms`) 는 RichEdit 창에 실제 키를 보내 결과와 앱이 받는 IME 메시지를 찍는다 (카카오톡과 같은 컨트롤).

## 릴리스
`v` 로 시작하는 태그를 푸시하면 GitHub Actions(`.github/workflows/release.yml`)가 테스트 → x64/x86 빌드 → 패키징(`scripts/package.ps1`) → 릴리스 생성까지 한다. 올라가는 파일은 기본판 `typescript-windows.zip`, 일본어 포함판 `typescript-windows-ja.zip`, 각각의 `.sha256`, `install.ps1`, `typescript.apk` 이다. 푸시와 PR마다 `ci.yml` 이 fmt, clippy, 테스트를 돌린다.
```powershell
git tag v0.2.0; git push origin v0.2.0
```
## 개발

```powershell
cargo test                                   # core 단위/통합 테스트 (원본 Swift 테스트 포팅)
cargo run -p typescript-cli -- -v "dkssud hello"
echo "dkssud\tenglish" | cargo run -p typescript-cli -- --decide
echo "きょうはいいてんきですね" | cargo run -p typescript-cli -- --convert   # 한자 후보
cargo clippy --all-targets -- -D warnings
# 일본어 포함판 (Cargo 기능 `japanese`, 사전 16MB 포함): 일본어 테스트는 이 기능이 있어야 돈다
cargo test --workspace --features typescript-core/japanese,typescript-tip/japanese,typescript-cli/japanese
```

`crates/core` 는 OS 의존성이 없다. 모듈은 역할별로 나뉜다.
| 모듈 | 내용 |
| --- | --- |
| `lang/` | 한글 조합(`hangul`), 한국어/영어 모델, 조사·어미 목록(`particles`), 로마자→가나(`romaji`), 가나 트라이그램 모델(`japanese`) |
| `decide/` | 판별 엔진 체인: 교정 학습(`learning`) → 영어+조사 분리(`mixed`) → 점수(`engine`); 문구 안 영어 분리(`segmenter`), 한자 변환 인터페이스(`convert`)와 사전 엔진(`kanji`) |
| `session/` | 입력 상태 머신: `word`(치는 중인 단어), `preview`(밑줄 미리보기), `flip`(확정 직후 Esc), `japanese`(일본어 문구·변환), `client`/`key`(앱과의 경계) |
| `store/` | 디스크에 남는 것: 앱별 정책·설정(`policy`), 단어 기록(`history`) |

`platforms/android/native` 는 코어를 JNI로 감싼 Rust 라이브러리(키와 캐럿 앞 텍스트를 받아 편집 명령을 돌려준다), `platforms/android/app` 은 그 결과를 `InputConnection` 에 적용하는 Java `InputMethodService` 다. 판별 로직은 전부 Rust에 있다.

`platforms/windows` 가 TSF COM DLL이다. 모델 테이블은 `build.rs` 가 빌드 때 구워서 DLL에 넣는다 (앱마다 로드되므로 실행 시 계산하지 않는다).

## 알려진 한계

- 영어(미국) 아래에 등록되므로 작업 표시줄의 언어 표시는 `ENG` 다.
- TSF 동작은 앱마다 다르다. 확정 직후 Esc(되돌리기)는 캐럿 뒤 텍스트를 읽을 수 있는 앱에서만 되고, 읽지 못하면 Esc는 앱에 전달된다.
- 한/영 키가 없는 키보드에서는 일시정지 키가 없다. 일시정지 상태를 보여 주는 표시도 Windows에는 아직 없다.
- Android 화면 키보드는 한 번에 한 손가락만 인식한다 (프레임워크 `KeyboardView`의 한계). 아주 빠르게 칠 때 키를 놓칠 수 있다.
- 자동완성 제안은 단어 아래 창으로만 보인다 (글자 안에 회색으로 붙이는 방식은 아님).
- (일본어) 한자 후보의 순위는 IPADIC+빈도 보정 수준이라 Mozc·Apple 엔진보다 낮다. 후보 창 없이 단어 자리를 순환하며 보여 준다.
- 데이터 라이선스는 [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) (`korean_vocabulary.txt` 는 CC BY-SA 4.0, `kana_trigrams.txt` 는 Tatoeba CC-BY 2.0 FR).
