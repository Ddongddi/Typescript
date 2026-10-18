# Typescript 설치 및 빌드 가이드

이 문서는 지원되는 모든 플랫폼(Windows, macOS, Linux, Android)에서 Typescript를 설치, 빌드 및 관리하는 방법을 안내합니다.

[English](../en/installation.md) · [한국어 문서 홈](README.md)

---

## 1. 지원 플랫폼 요약

| 플랫폼 | 바이너리 형태 | 입력 대상 | 주요 지원 기능 |
| --- | --- | --- | --- |
| **Windows** | TSF 텍스트 서비스 (x64 / x86 COM DLL) | 물리 키보드 | 한/영/일 판별, Tab 자동완성, 단어 학습, 풍선창 UI |
| **Android** | `InputMethodService` 독립 APK | 화면 키보드 + 외장 키보드 | 한/영 판별, 두벌식 각인 가상 키보드, 하드웨어 키보드 |
| **macOS** | InputMethodKit 앱 (`Typescript.app`) | 물리 키보드 | 한/영/일 판별, Option+Space 일시정지, Esc 순환 |
| **Linux** | IBus 입력기 엔진 (`ibus-engine-typescript`) | 물리 키보드 | 한/영/일 판별, IBus 설정 통합 |

---

## 2. Windows 설치

### 2.1 원클릭 빠른 설치 (권장)

PowerShell을 열고 다음 명령어를 실행합니다 (설치 시 관리자 권한 확인 창이 1회 표시됩니다):

```powershell
irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1 | iex
```

- 설치가 끝나면(첫 설치) **시작 안내 페이지**가 브라우저에 열려 사용할 언어(영어, 한국어, 일본어)를 고르고 켤 수 있고, 짧은 연습까지 안내합니다. 업데이트 설치에서는 열리지 않습니다.
- 다운로드된 zip 파일은 공식 릴리스의 SHA-256 체크섬을 검증한 후 안전하게 설치됩니다.

#### 옵션을 지정하여 설치
대화형 창 없이 명령줄 인수로 직접 구성할 수 있습니다:

```powershell
$i = [scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))

# 언어 지정 (한국어, 영어, 일본어 순서로 Esc 순환)
& $i -Languages ko,en,ja

# 일본어 팩 추가 활성화
& $i -Japanese

# 특정 버전 설치
& $i -Version v0.9.1

# 삭제/제거 (설정 및 언어 팩은 보존)
& $i -Uninstall
```

### 2.2 설치 후 입력기 선택
1. **Win + Space**를 누르고 키보드 레이아웃 목록에서 **Typescript** 또는 **Typescript (자동완성)**을 선택합니다.
2. 언어 목록에서는 **영어(미국)** 아래에 위치합니다 (카카오톡 등 구형 IMM32 애플리케이션의 조합 잘림 문제를 회피하기 위해 의도적으로 영어 레이아웃에 등록됩니다).
3. 이미 실행 중인 프로그램은 종료 후 다시 시작해야 새 입력기를 인식합니다.

### 2.3 Windows 소스 직접 빌드
Windows TSF DLL은 64비트 앱과 32비트 앱을 모두 지원해야 하므로 두 아키텍처 모두 컴파일해야 합니다.

**필수 요구사항:**
- Rust stable (`rustup`)
- 32비트 타깃 추가: `rustup target add i686-pc-windows-msvc`
- Visual Studio C++ 빌드 도구 (MSVC)

```powershell
# 1. 리포지토리 클론
git clone https://github.com/iamallyouneed/Typescript.git
cd Typescript

# 2. x64 + x86 자동 빌드 및 TSF 서비스 등록
.\scripts\install.ps1

# 일본어 팩도 함께 빌드 및 등록하려면:
.\scripts\install.ps1 -Japanese
```

---

## 3. macOS 설치

### 3.1 빠른 설치

터미널에서 아래 명령을 실행합니다:

```sh
curl -fsSL https://github.com/iamallyouneed/Typescript/releases/latest/download/install.sh | sh
```

- 설치 위치: `~/Library/Input Methods/Typescript.app` (관리자 권한 불필요)
- 설치가 완료되면 **시스템 로그아웃 후 다시 로그인**합니다.
- **시스템 설정** → **키보드** → **텍스트 입력 (입력 소스)** → **편집**에서 `+`를 누르고 **Typescript**를 추가합니다.
- *참고*: 공증(Notarization)을 거치지 않은 경우 처음 실행 시 macOS **개인정보 보호 및 보안**에서 "확인 없이 열기"를 눌러야 할 수 있습니다.

### 3.2 macOS 소스 빌드
```sh
# 유니버설 바이너리(Apple Silicon + Intel) 타깃 추가
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# 빌드 및 패키징 스크립트 실행
sh platforms/macos/build.sh
```

---

## 4. Linux 설치

### 4.1 빠른 설치 (IBus)

```sh
curl -fsSL https://github.com/iamallyouneed/Typescript/releases/latest/download/install.sh | sh
```

- 엔진 파일은 `~/.local/share/typescript/`, IBus 컴포넌트 XML은 `~/.local/share/ibus/component/`에 설치됩니다.
- 설치 후 로그아웃했다가 로그인하거나 `ibus restart`를 실행합니다.
- 데스크톱 환경 설정의 입력기 메뉴 또는 `ibus-setup`에서 **Typescript**를 추가합니다.

### 4.2 Linux 소스 직접 빌드
**필수 패키지 (Ubuntu/Debian 기준):**
```sh
sudo apt update
sudo apt install libibus-1.0-dev pkg-config build-essential
```

**컴파일 및 설치:**
```sh
cargo build --release -p typescript-ffi
make -C platforms/linux
make -C platforms/linux install
ibus restart
```

---

## 5. Android 설치

### 5.1 APK 직접 설치
1. [GitHub Releases](https://github.com/iamallyouneed/Typescript/releases/latest) 페이지에서 **`typescript.apk`**를 다운로드합니다.
2. 기기에서 APK를 실행하고 "출처를 알 수 없는 앱 설치"를 허용합니다.
3. 안드로이드 **설정** → **시스템** → **언어 및 입력** → **화면 키보드** 목록에서 **Typescript**를 활성화합니다.
4. 입력창을 터치한 후 키보드 전환 아이콘을 눌러 Typescript를 선택합니다.

### 5.2 Android 소스 빌드
**필수 요구사항:**
- Android SDK (API 34 platform, build-tools 34.0.0)
- Android NDK (r25 이상)
- JDK 17
- Rust 타깃: `rustup target add aarch64-linux-android x86_64-linux-android`
- `cargo install cargo-ndk`

**빌드 명령어:**
```powershell
# arm64 + x86_64 통합 빌드
.\platforms\android\build.ps1

# 폰용만 (arm64)
.\platforms\android\build.ps1 -Abi arm64-v8a

# 기기에 즉시 설치 (ADB 연결 상태)
adb install -r platforms\android\build\typescript.apk
```

---

## 6. 제거 및 클린업

### Windows
```powershell
# 설치 스크립트를 통한 제거
$i = [scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))
& $i -Uninstall

# 설정 및 학습 기록까지 완전 삭제하려면:
Remove-Item -Recurse -Force "$env:APPDATA\Typescript"
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\Typescript"
```

### macOS / Linux
```sh
curl -fsSL https://github.com/iamallyouneed/Typescript/releases/latest/download/install.sh | sh -s -- --uninstall
```
