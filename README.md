# Typescript

Windows용 영어/한국어(두벌식) 자동 판별 입력기.

```
dkssudgktpdy, hello world!          →  안녕하세요, hello world!
dlrjs githubdp commitgkf rjdi       →  이건 github에 commit할 거야
```
## 설치

```powershell
.\scripts\install.ps1            # 빌드(x64 + x86) → 관리자 권한 요청(UAC 1회) → 등록
.\scripts\install.ps1 -Uninstall # 제거 (설정은 %APPDATA%\Typescript 에 남는다)
```

설치 후 **Win+Space** 로 `Typescript (영어/한국어 자동)` 을 고른다. 이미 떠 있는 앱은 다시 열어야 새 입력기를 읽는다.

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
| **한/영 키** | 이 텍스트 필드에서 Typescript를 끄고 켠다 (다른 칸으로 가면 다시 켜진다) |

## 설정 (`%APPDATA%\Typescript\`)

JSON을 직접 편집한다. 앱 간 전환할 때 다시 읽는다.

`settings.json`
```json
{ "returnConfirmsOnly": true, "pauseKey": "hangul" }
```
`pauseKey`: `"hangul"`(한/영 키) 또는 `"off"`.

`apps.json` — 키는 **exe 파일 이름**(대소문자 무시). `"*"` 는 기본값이고, 앱 항목에는 다른 키만 적는다.
```json
{ "*": { "escapeToApp": false },
  "windowsterminal.exe": { "conversionThreshold": 0.97 },
  "game.exe": { "enabled": false } }
```
- `enabled: false` 키를 그대로 통과 (영문 입력)
- `conversionThreshold` 기본 0.5. 0.97(엄격)은 터미널에서 짧은 명령이 한글로 바뀌는 것을 막는다
- `escapeToApp: true` Esc를 앱에 넘기고 Shift+Esc로 읽기를 바꾼다 (에디터)

`lexicon.json` — 교정으로 배운 단어 (직접 고쳐도 된다).

## 개발

```powershell
cargo test                                   # core 단위/통합 테스트 (원본 Swift 테스트 포팅)
cargo run -p typescript-cli -- -v "dkssud hello"
echo "dkssud\tenglish" | cargo run -p typescript-cli -- --decide
cargo clippy --all-targets -- -D warnings
```

`crates/core` 는 OS 의존성이 없다 (판별 엔진, 세션 상태 머신). `crates/tip` 가 TSF COM DLL이다. 모델 테이블은 `build.rs` 가 빌드 때 구워서 DLL에 넣는다 (앱마다 로드되므로 실행 시 계산하지 않는다).
