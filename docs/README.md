# tacit Documentation · 문서

[한국어](#한국어-문서) · [English](#english-documentation)

Welcome to the official documentation for **tacit**, an automatic multilingual input method editor (IME) that intelligently distinguishes between English, Korean, and Japanese as you type—without needing to toggle a language key.

---

## 한국어 문서

tacit의 구조, 설치 방법, 상세 설정, 언어 팩 개발 및 문제 해결에 관한 상세 문서입니다.

| 문서 | 설명 |
| --- | --- |
| 📖 [**한국어 문서 홈**](ko/README.md) | 한국어 문서 개요 및 빠른 시작 가이드 |
| 🏗️ [**아키텍처 & 동작 원리**](ko/architecture.md) | Rust 코어 구조, 입력 세션 상태 머신, 판별 엔진 및 언어 모델 상세 |
| 💻 [**설치 및 빌드 가이드**](ko/installation.md) | Windows, Android, macOS, Linux 플랫폼별 원클릭 설치 및 직접 빌드 |
| ⚙️ [**설정 및 관리 가이드**](ko/configuration.md) | `settings.json`, `apps.json`, `lexicon.json` 및 `tacit-config` 웹 UI/CLI 사용법 |
| 📦 [**언어 팩 시스템**](ko/language-packs.md) | 언어 팩 포맷, `packgen` 바이너리 생성기, 새 언어 추가 및 데이터 라이선스 |
| 🛠️ [**문제 해결 & 앱 호환성**](ko/troubleshooting.md) | IMM32/RichEdit 호환성 원리, 터미널/에디터 충돌 해결, 디버그 로깅 및 FAQ |
| 📈 [**릴리즈 리포트**](report/performance.md) | 릴리즈마다 크기·정확도·속도·메모리를 같은 방법으로 재서 쌓는 기록 ([score.md](report/score.md) 점수 규격, [trial.md](report/trial.md) 시도 기록) |

---

## English Documentation

Comprehensive guides on tacit's architecture, multi-platform installation, configuration, language pack creation, and troubleshooting.

| Document | Description |
| --- | --- |
| 📖 [**Documentation Hub**](en/README.md) | English documentation index and quick overview |
| 🏗️ [**Architecture & Design**](en/architecture.md) | Rust core engine, session state machine, score-based decision engine, and models |
| 💻 [**Installation & Build**](en/installation.md) | One-line install scripts and build steps for Windows, macOS, Linux, and Android |
| ⚙️ [**Configuration Guide**](en/configuration.md) | `settings.json`, `apps.json`, `lexicon.json`, web dashboard, and CLI management |
| 📦 [**Language Pack System**](en/language-packs.md) | Pack folder format, `packgen` compiler, adding new languages, and licensing |
| 🛠️ [**Troubleshooting & Compatibility**](en/troubleshooting.md) | Windows IMM32 workarounds, editor/terminal integration, debug logging, and FAQ |
| 📈 [**Release Reports**](report/performance.md) | Size, accuracy, speed and memory measured the same way for every release ([score.md](report/score.md) for the score spec, [trial.md](report/trial.md) for tried experiments) |
