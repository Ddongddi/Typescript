# Typescript

Windows용 영어/한국어(두벌식) 자동 판별 입력기.

```
dkssudgktpdy, hello world!          →  안녕하세요, hello world!
dlrjs githubdp commitgkf rjdi       →  이건 github에 commit할 거야
fltpssm ghkdlxld                    →  리센느 화이팅
```
## 설치

```powershell
.\scripts\install.ps1            # 빌드(x64 + x86) → 관리자 권한 요청(UAC 1회) → 등록
.\scripts\install.ps1 -Uninstall # 제거 (설정은 %APPDATA%\Typescript 에 남는다)
```

설치 후 **Win+Space** 로 입력기를 고른다. 언어 목록에서는 **영어(미국)** 아래에 있다 (없으면 설치가 영어(미국)을 추가한다). 한국어 아래에 두면 카카오톡 같은 IMM32 앱에서 Windows가 조합을 한 글자로 잘라 버려서다 (아래 "앱 호환"). 두 가지가 있다.

| 입력기 | 하는 일 |
| --- | --- |
| `Typescript (자동완성)` | 영어/한국어 자동 판별 + Tab 자동완성 + 쓴 단어 학습 |
| `Typescript` | 자동 판별만 (자동완성도, 단어 기록도 없다) |

이미 떠 있는 앱은 다시 열어야 새 입력기를 읽는다. 아이콘은 `assets/icon.png` 를 바꾸고 `.\scripts\make-icon.ps1` 후 다시 설치하면 된다.

## 쓰는 법

| 키 | 동작 |
| --- | --- |
| 글자 | 단어를 이어 간다. 점선 밑줄로 현재 판단을 보여 준다 |
| Space, 구두점 | 단어를 확정하고 키는 앱이 그대로 받는다 |
| Enter | 밑줄 단어를 확정만 한다 (다음 Enter가 앱의 것) |
| Esc | 읽기를 순환: 영어 → 영어+조사 → 한국어 |
| Shift+Esc | 어느 앱에서나 읽기를 순환 |
| 확정 직후 Esc | 방금 확정한 단어를 다음 읽기로 바꾼다. 같은 교정을 두 번 하면 학습한다 |
| Backspace | 영어는 글자 단위, 한국어는 두벌식처럼 자모 → 음절 단위 |
| Tab | (자동완성 입력기) 쓰는 중인 단어를 자주 쓴 단어로 완성한다. 내가 친 단어가 먼저고, 없으면 내장 사전이다. 완성될 단어는 단어 아래의 작은 알약 모양 창에 `안녕하세요  Tab`으로 미리 보인다 (앱 테마에 맞춰 라이트/다크). 완성할 게 없으면 단어만 확정하고 Tab은 앱으로 간다 |
| **한/영 키** | 이 텍스트 필드에서 Typescript를 끄고 켠다 (다른 칸으로 가면 다시 켜진다) |

## 설정 (`%APPDATA%\Typescript\`)

JSON을 직접 편집한다. 앱 간 전환할 때 다시 읽는다.

`settings.json`
```json
{ "returnConfirmsOnly": true, "pauseKey": "hangul", "autocomplete": true }
```
`autocomplete`: `false`면 `Typescript (자동완성)` 에서도 Tab 자동완성과 단어 기록을 끈다.
`pauseKey`: `"hangul"`(한/영 키) 또는 `"off"`.

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

`lexicon.json` — 교정으로 배운 단어 (직접 고쳐도 된다).

## 앱 호환

- **카카오톡, RichEdit 기반 앱 (IMM32 앱)**: Windows는 TSF 입력기의 조합을 IMM32 메시지로 바꿔 주는데, *한국어* 입력기에는 "조합은 한 글자(interim)" 규칙을 적용해 나머지를 바로 확정한다. 단어 전체를 미리보기로 보여 주는 이 입력기는 한국어로 등록하면 키마다 끊겼다 (`테스트` → `ㅌpㅅmㅌm`). 그래서 **영어(미국)로 등록한다**. 그러면 같은 앱에서도 단어 전체가 하나의 조합으로 유지된다. 또 IMM32 앱은 입력기가 "가져가겠다"고 한 키를 돌려받지 못하므로, 단어를 끝내고 앱으로 넘어가는 키(Space, 구두점, Enter, Tab, 방향키)는 TSF가 키를 물어보는 단계에서 바로 처리한다.
- 그래도 시스템 한국어 입력기를 쓰고 싶은 앱은 `apps.json` 에 `{ "앱.exe": { "useSystemIme": true } }` 로 그 프로세스만 Microsoft 한국어 IME로 넘길 수 있다 (다른 앱으로 가면 되돌아온다).
- 문제를 볼 때는 `%APPDATA%\Typescript\debug.on` 파일을 만들면 같은 폴더의 `debug.log` 에 이벤트 이름(포커스 이동, 조합 시작, 앱이 조합을 끝냄)만 남는다. 친 글자는 기록하지 않는다. 확인이 끝나면 파일을 지운다.
- `cargo run -p typescript-tip --example richedit_lab -- --ts` (또는 `--ms`) 는 RichEdit 창에 실제 키를 보내 결과와 앱이 받는 IME 메시지를 찍는다 (카카오톡과 같은 컨트롤).

## 개발

```powershell
cargo test                                   # core 단위/통합 테스트 (원본 Swift 테스트 포팅)
cargo run -p typescript-cli -- -v "dkssud hello"
echo "dkssud\tenglish" | cargo run -p typescript-cli -- --decide
cargo clippy --all-targets -- -D warnings
```

`crates/core` 는 OS 의존성이 없다 (판별 엔진, 세션 상태 머신). `crates/tip` 가 TSF COM DLL이다. 모델 테이블은 `build.rs` 가 빌드 때 구워서 DLL에 넣는다 (앱마다 로드되므로 실행 시 계산하지 않는다).

## 알려진 한계

- 영어(미국) 아래에 등록되므로 작업 표시줄의 언어 표시는 `ENG` 다.
- TSF 동작은 앱마다 다르다. 확정 직후 Esc(되돌리기)는 캐럿 뒤 텍스트를 읽을 수 있는 앱에서만 되고, 읽지 못하면 Esc는 앱에 전달된다.
- 한/영 키가 없는 키보드에서는 일시정지 키가 없다. 일시정지 상태를 보여 주는 표시도 아직 없다.
- 자동완성 제안은 단어 아래 창으로만 보인다 (글자 안에 회색으로 붙이는 방식은 아님).
- 데이터 라이선스는 [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) (`korean_vocabulary.txt` 는 CC BY-SA 4.0).
