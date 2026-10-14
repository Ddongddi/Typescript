# 변경 기록 (Changelog)

릴리스마다 무엇이 바뀌었는지 적습니다. 형식은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/)를,
버전은 [유의적 버전](https://semver.org/lang/ko/)을 따릅니다. 릴리스를 만들 때 해당 버전의 항목이
GitHub 릴리스 노트가 되고 대시보드의 업데이트 탭에도 그대로 보입니다. 쓰는 법은
[CONTRIBUTING.md](CONTRIBUTING.md#변경-기록)에 있습니다.

## [Unreleased]

## [0.5.1] - 2026-10-14

### 추가
- 문서: `docs/`의 한국어·영어 문서(설치, 설정, 아키텍처, 언어 팩, 문제 해결)와 영문 README.
- 대시보드의 입력 설정에 **채팅으로 체험**: 영문 자판으로 직접 쳐서 실제 입력기와 같은 방식으로 확인합니다.
  "Enter는 확정만"이 켜져 있으면 전송에 Enter 두 번, 꺼져 있으면 한 번이 걸리는 것을 말풍선의 `Enter ×N`으로
  보여 주고, 두 동작을 버튼으로 바꿔 가며 비교할 수 있습니다. Tab 자동완성과 Backspace도 됩니다.
- 업데이트로 프로그램이 바뀌면 설정 화면을 닫고 `typescript-config`를 다시 열라고 안내합니다.

### 수정
- 프로그램을 업데이트한 뒤 대시보드에 "쓸 수 없는 팩: …needs core …"가 남던 문제. 같은 팩의 새 버전이 이미 동작하면 옛 버전 폴더는 문제로 보지 않고, 팩을 설치할 때 그 팩의 옛 버전 폴더를 정리합니다.
- 켜 둔 언어의 팩이 없거나 이 버전과 맞지 않을 때 대시보드의 체험이 오류를 내던 문제. 입력기처럼 그 언어를 빼고 동작하고, 쓸 수 있는 팩이 없으면 안내 문구를 보여 줍니다.
- 대시보드에서 키보드 조작 뒤 패널 전체에 큰 윤곽선이 씌워지던 문제.

## [0.5.0] - 2026-10-14

### 추가
- **macOS**: InputMethodKit 입력기(`Typescript.app`, Apple silicon + Intel).
- **Linux**: IBus 엔진(`ibus-engine-typescript`).
- `install.sh`: macOS·Linux 한 줄 설치/제거(현재 사용자에게만, 관리자 권한 불필요, SHA-256 확인).
- `platforms/ffi`: 입력기를 C 함수로 감싼 인터페이스(`typescript.h`). macOS·Linux 입력기가 공유합니다.
- 대시보드: 체험 탭에서 단어를 누르면 다른 언어로 읽었을 때의 결과를 보고 바꿔 볼 수 있습니다.
- 대시보드: 앱 규칙의 변환 기준이 슬라이더가 되었고, 움직이는 동안 샘플 문장이 어떻게 읽히는지 미리 봅니다.
- 대시보드: Esc 순서 칩을 끌어서 바꾸고, Esc를 눌러 순환하는 모습을 확인합니다.
- 대시보드: "Enter는 확정만", "Tab 자동완성" 설정의 짧은 시연.
- 대시보드: 변경 되돌리기, 단축키(`1`–`5` 탭 이동, `/` 앱 찾기).
- 새 로고와 아이콘.
- `LICENSE`를 AGPL-3.0으로 바꾸고 `NOTICE`, `CONTRIBUTING.md`, 이 변경 기록을 추가.

### 변경
- 설정과 언어 팩 폴더가 시스템별로 나뉩니다. Windows는 그대로이고, macOS는 `~/Library/Application Support/Typescript`,
  Linux는 `~/.config/Typescript`(팩은 `~/.local/share/Typescript/packs`)입니다.
- 라이선스: MIT → AGPL-3.0-only (v0.4.1까지의 릴리스는 MIT로 남습니다).
- 대시보드 소스를 `crates/config/src/web/`의 마크업·스타일·스크립트 파일로 나눴습니다.
- CI와 릴리스가 Windows, Android에 더해 Linux, macOS에서도 빌드·테스트·패키징합니다.

### 수정
- 접근성 속성(`aria-pressed` 등)이 빈 값으로 나가던 것을 고쳤습니다.

## [0.4.1] - 2026-10-13

### 추가
- 대시보드의 체험 탭: 영문 자판으로 친 글이 단어마다 어떻게 읽히는지 바로 봅니다.
- 설정마다 예시와 "기본값 아님" 표시, "기본값으로" 되돌리기.
- 앱별 규칙 카드, 프리셋(터미널·게임·vim·카카오톡), 실행 중인 앱 목록(검색, 페이지, 끌어놓기).
- 커스텀 드롭다운, 접근성(키보드, 스크린 리더, 고대비)과 좁은 화면 대응.

### 변경
- 대시보드를 미니멀한 평면 디자인으로 다듬었습니다.

## [0.4.0] - 2026-10-13

### 추가
- **언어 팩**: 언어 데이터를 팩으로 나눠 필요한 것만 받습니다. 언어를 모듈로 추가할 수 있는 구조.
- `typescript-config`: 터미널 설정 화면, 명령줄, 웹 대시보드.
- 릴리스에서 새 버전과 언어 팩을 확인하고 갱신하는 업데이트.
- 설치 때 언어를 고르고 팩을 받습니다.

### 변경
- 엔진이 켜진 언어의 임의 조합으로 판별합니다. Esc 순환은 켜진 언어를 따릅니다.

## [0.3.0] - 2026-10-13

### 추가
- 영어·한국어 판과 일본어 판을 따로 배포(`install.ps1 -Japanese`).

### 수정
- IPADIC에 없는 단어의 가타카나 후보, 영어 단어 뒤의 `ー` 입력.

## [0.2.1] - 2026-10-13

### 추가
- **일본어**: 로마자 → 가나 판별, 변환 흐름, 전각 처리, IPADIC 기반 한자 변환.

### 변경
- 고정 릴리스 키로 서명한 첫 APK.

## [0.2.0] - 2026-10-13

### 추가
- **Android**: `InputMethodService` 앱과 화면 키보드.

### 변경
- 코어를 `platforms/windows`와 `platforms/android`가 함께 씁니다.

## [0.1.1] - 2026-10-12

### 수정
- 설치 스크립트가 관리자 단계에서 저장소 경로를 못 찾던 문제.

## 0.1.0

- 첫 공개: Windows용 영어·한국어 자동 판별 입력기(TSF), Tab 자동완성, 앱별 규칙, 카카오톡 호환.

[Unreleased]: https://github.com/iamallyouneed/Typescript/compare/v0.5.1...HEAD
[0.5.1]: https://github.com/iamallyouneed/Typescript/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/iamallyouneed/Typescript/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/iamallyouneed/Typescript/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/iamallyouneed/Typescript/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/iamallyouneed/Typescript/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/iamallyouneed/Typescript/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/iamallyouneed/Typescript/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/iamallyouneed/Typescript/releases/tag/v0.1.1
