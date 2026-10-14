# Typescript 설정 및 관리 가이드

Typescript는 브라우저 기반의 로컬 웹 대시보드, CLI 관리 도구(`typescript-config`), 그리고 직접 편집할 수 있는 JSON 설정 파일을 제공합니다.

[English](../en/configuration.md) · [한국어 문서 홈](README.md)

---

## 1. 설정 저장 경로

운영체제별 설정 및 사용자 데이터 저장 경로는 다음과 같습니다:

| OS | 경로 |
| --- | --- |
| **Windows** | `%APPDATA%\Typescript\` (`C:\Users\<사용자>\AppData\Roaming\Typescript\`) |
| **macOS** | `~/Library/Application Support/Typescript/` |
| **Linux** | `~/.config/Typescript/` (또는 `$XDG_CONFIG_HOME/Typescript/`) |

---

## 2. 웹 대시보드 (`typescript-config`)

시작 메뉴에서 **Typescript 설정**을 클릭하거나, 터미널에서 `typescript-config`를 입력하면 기본 웹 브라우저에 관리 인터페이스가 열립니다.

```powershell
typescript-config
```

### 보안 및 라이프사이클
- **로컬 전용 바인딩**: 외부 접속을 차단하며 오직 `127.0.0.1` 루프백 인터페이스에서만 수신 대기합니다.
- **일회용 보안 토큰**: 실행 시 무작위 토큰(`?token=...`)이 URL에 부여되어, 사용자가 브라우저 탭을 열 때 이 토큰이 일치해야만 제어 API가 동작합니다.
- **자동 종료**: 사용자가 설정 탭을 닫으면 백그라운드 프로세스가 잠시 후 자동으로 종료되어 자원을 소모하지 않습니다.

### 대시보드 주요 기능
1. **언어 관리**: 사용 가능한 언어 팩 다운로드/삭제, 언어 활성화 토글, `Esc` 키 순환 순서 드래그 앤 드롭 변경.
2. **입력 옵션**: Enter 키 확정 동작, Tab 자동완성 켜기/끄기, 일시정지 키 설정, 일본어 라이브 변환 및 전각 문자 옵션.
3. **앱별 프로필**: 실행 중인 프로세스 목록을 확인하고, 앱별로 변환 민감도(`conversionThreshold`)나 `Esc` 처리 방식을 GUI 테이블로 조정.
4. **실시간 체험(Playground)**: 브라우저 내에서 직접 타이핑하며 판별 점수와 분리 결과를 즉시 확인.
5. **업데이트**: 최신 릴리스 및 언어 팩 확인과 원클릭 업데이트.

---

## 3. CLI 명령어 레퍼런스

터미널이나 스크립트에서 `typescript-config` 명령줄 도구를 활용해 모든 설정을 제어할 수 있습니다.

```powershell
# 현재 설치된 언어, 다운로드 가능한 언어, 순환 순서 출력
typescript-config list

# 언어 팩 다운로드 및 활성화 (예: 일본어)
typescript-config add ja

# 언어 비활성화 및 팩 파일 삭제
typescript-config remove ja

# Esc 순환 순서 재정렬 (활성화된 언어 내에서)
typescript-config order ko,en,ja

# 새 프로그램 릴리스 및 언어 팩 업데이트 확인
typescript-config check

# 언어 팩 갱신 및 신규 설치 프로그램 실행
typescript-config update

# 비대화형 초기 셋업 (배포 자동화용)
typescript-config setup --languages ko,en
```

---

## 4. 설정 파일 레퍼런스

설정 파일은 사람이 읽을 수 있는 JSON 형식이며, 텍스트 에디터로 직접 수정할 수 있습니다. 입력기는 포커스가 전환될 때 변경 사항을 자동으로 감지하여 다시 읽어들입니다.

### 4.1 `settings.json` (전역 설정)

```json
{
  "languages": ["en", "ko"],
  "returnConfirmsOnly": true,
  "hangulKey": "Hangul",
  "hangulAction": "flip",
  "flipKeys": ["Esc", "HangulKey"],
  "pauseKeys": ["Alt+HangulKey"],
  "autocomplete": true,
  "checkUpdates": true,
  "liveConversion": false,
  "fullWidthDigits": false,
  "fullWidthSpace": false
}
```

| 필드 | 타입 | 기본값 | 설명 |
| --- | --- | --- | --- |
| `languages` | `string[]` | `["en", "ko"]` | 활성화할 언어 목록 및 `Esc` 순환 순서. (`"en"`, `"ko"`, `"ja"` 지원) |
| `returnConfirmsOnly` | `bool` | `true` | `true`일 경우 단어를 입력 중일 때 `Enter`를 누르면 현재 단어 밑줄만 확정하고 줄바꿈은 앱에 보내지 않습니다. |
| `hangulKey` | `string` | `"Hangul"` | 한영키로 쓸 키. 목록에서 `HangulKey`(또는 `한영키`)라고 쓰면 이 키를 가리킵니다. `Ctrl`·`RAlt` 같은 수식어 키로 바꾸면 `Alt+HangulKey` 같은 조합은 만들 수 없습니다. |
| `hangulAction` | `string` | `"flip"` | 한/영 키만 눌렀을 때 하는 일. `"flip"`(단어 언어 전환), `"none"`(아무것도 안 함), 또는 앱에 보낼 단축키(`"Ctrl+C"`, `"Win+L"`, `"Alt+Tab"` …). 단축키를 보내면 단어 언어 전환은 하지 않습니다. 대시보드의 "키" 탭에서도 정할 수 있습니다(Windows). |
| `flipKeys` | `string[]` | `["Esc", "HangulKey"]` | 쓰는 중인(방금 친) 단어를 다음 언어로 바꾸는 키. `"Ctrl+Space"`처럼 조합할 수 있고, `"Ctrl"`·`"RCtrl"` 같은 수식어 키는 다른 키 없이 눌렀다 뗄 때 동작합니다. 최대 8개, `[]`이면 사용하지 않습니다. |
| `pauseKeys` | `string[]` | `["Alt+HangulKey"]` | 입력기 일시정지/재개 키. 형식은 `flipKeys`와 같고, 두 목록에 같은 키를 쓸 수 없습니다. 예전 `pauseKey`는 읽어서 옮깁니다(`"off"`는 `[]`). |
| `autocomplete` | `bool` | `true` | `Typescript (자동완성)` 입력기에서 `Tab` 키 자동완성 및 빈도 학습 활성화 여부. |
| `checkUpdates` | `bool` | `true` | 하루 1회 GitHub 릴리스를 조회하여 새 버전 존재 여부를 확인합니다. |
| `liveConversion` | `bool` | `false` | (일본어) 타이핑하는 도중 추천 한자 변환 결과를 실시간으로 미리보기로 표시합니다. |
| `fullWidthDigits` | `bool` | `false` | (일본어) 일본어 단어 뒤에 입력되는 숫자를 전각(`０３-１２３４`)으로 입력합니다. |
| `fullWidthSpace` | `bool` | `false` | (일본어) 일본어 단어 뒤의 공백을 전각 공백(`　`)으로 입력합니다. |

키 이름은 `Esc`, `Hangul`, `Hanja`, `CapsLock`, `Pause`, `ScrollLock`, `Space`, `F1`~`F24`, 글자(`A`~`Z`)와 숫자(`0`~`9`)이고, 수식어는 `Ctrl` `Alt` `Shift`(오른쪽만은 `RCtrl` `RAlt` `RShift`)입니다. Win 키 조합, 수식어 없는 글자·숫자·`Space`, `Ctrl+C`·`Ctrl+V` 같은 흔한 단축키, `Alt+F4`는 지정할 수 없습니다.

---

### 4.2 `apps.json` (애플리케이션별 규칙)

애플리케이션의 실행 파일명(소문자)을 키로 하여 개별 앱 맞춤형 동작을 정의합니다. `"*"`는 모든 앱의 기본값입니다.

```json
{
  "*": {
    "escapeToApp": false,
    "conversionThreshold": 0.5,
    "enabled": true
  },
  "windowsterminal.exe": {
    "conversionThreshold": 0.97
  },
  "code.exe": {
    "escapeToApp": true
  },
  "game.exe": {
    "enabled": false
  },
  "special.exe": {
    "useSystemIme": true
  }
}
```

| 속성 | 타입 | 설명 |
| --- | --- | --- |
| `enabled` | `bool` | `false` 지정 시 해당 앱에서는 입력기를 완전히 비활성화(순수 영문 입력으로 통과)합니다. 게임 등에서 유용합니다. |
| `conversionThreshold` | `float` | 한글/일본어로 판별하기 위한 확신도 임계값 (`0.0` ~ `1.0`). 기본 `0.5`. 터미널에서는 `ls`, `cd`, `git` 등의 짧은 명령어가 한글로 바뀌는 것을 막기 위해 `0.97` 수준의 엄격한 값을 권장합니다. |
| `escapeToApp` | `bool` | `true`로 설정하면 단일 `Esc` 키를 Typescript가 가로채지 않고 앱(Vim, VSCode 등)으로 직접 전달합니다. 언어 순환은 `Shift + Esc`로 수행하게 됩니다. |
| `useSystemIme` | `bool` | 해당 앱에 포커스가 갈 때 자동으로 Microsoft 기본 한국어 IME로 전환합니다. 포커스를 벗어나면 Typescript로 복귀합니다. |
| `fullWidth` | `bool` | `false`로 두면 전역 일본어 전각 설정이 켜져 있더라도 이 앱(터미널 등)에서는 반각 숫자/공백을 유지합니다. |

---

### 4.3 `lexicon.json` (사용자 교정 사전)

사용자가 타이핑 도중 `Esc` 키를 눌러 교정한 단어가 기록됩니다. 동일한 교정이 2회 이상 발생하면 자동 학습되어 이후 입력 시 최우선 순위로 판별됩니다.

```json
{
  "gksrmf": {
    "target": "ko",
    "hits": 3
  },
  "apple": {
    "target": "en",
    "hits": 2
  }
}
```

필요한 경우 이 파일을 직접 열어 오교정된 단어를 삭제하거나 새 매핑을 추가할 수 있습니다.

---

### 4.4 `user_history.json` (자동완성 학습 기록)

`Typescript (자동완성)` 입력기에서 확정한 정상 단어들의 사용 빈도가 카운팅됩니다.

- **2회 이상 입력된 단어만** 자동완성 제안 목록에 노출되므로, 오타나 1회성 비밀번호가 노출되지 않습니다.
- 10단어 입력 시점마다, 그리고 입력 포커스를 이동할 때 디스크에 동기화되므로 다른 앱에서 입력한 단어도 즉시 제안됩니다.
