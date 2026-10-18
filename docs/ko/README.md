# Tacit 공식 문서 (한국어)

Tacit은 한/영 키를 누를 필요 없이, 영문 쿼티(QWERTY) 키보드로 치는 내용을 단어 단위로 분석하여 **영어**, **한국어(두벌식)**, **일본어(로마자/가나/한자)**를 자동으로 판별해 알맞은 글자로 변환해 주는 다국어 스마트 입력기(IME)입니다.

[English Documentation](../en/README.md) · [전체 문서 인덱스](../README.md)

---

## 📚 문서 목차

1. [**아키텍처 & 핵심 원리**](architecture.md)
   - 입력 세션 상태 머신 (`session`)
   - 단어 언어 판별 엔진 파이프라인 (`decide`)
   - 언어별 조합 및 언어 모델 (`lang`)
   - 영문+조사 분리 및 단어 학습 메커니즘
   - 메모리 매핑 기반 언어 팩 설계

2. [**설치 및 빌드 가이드**](installation.md)
   - Windows (TSF COM 서비스 설치, 소스 빌드, 제거)
   - macOS (InputMethodKit 빌드 및 활성화)
   - Linux (IBus 엔진 빌드 및 데스크톱 통합)
   - Android (APK 다운로드 및 소스 빌드)
   - 언어 팩 다운로드 및 오프라인 배포

3. [**설정 및 관리 가이드**](configuration.md)
   - 로컬 웹 대시보드 (`tacit-config`) 사용법
   - CLI 명령어 레퍼런스
   - 설정 파일 (`settings.json`, `apps.json`, `lexicon.json`)
   - 앱별 규칙 (터미널, 에디터, 게임 최적화)
   - 자동완성과 사용자 사전 학습

4. [**언어 팩 시스템**](language-packs.md)
   - 언어 팩 구조 및 포맷 사양 (`manifest.json`, 바이너리 테이블)
   - `tacit-packgen` 도구 사용법
   - 새로운 언어 모듈 추가 절차
   - 사전/트라이그램 데이터셋 요건 및 라이선스 고지

5. [**문제 해결 & 앱 호환성**](troubleshooting.md)
   - 카카오톡 및 RichEdit 계열 IMM32 호환성 (영어 레이아웃 등록 이유)
   - 터미널(Windows Terminal, Vim) 및 IDE 키 충돌 대처
   - 디버그 로깅 (`debug.on`) 및 문제 추적
   - 플랫폼별 알려진 한계 및 자주 묻는 질문(FAQ)

---

## ⚡ 빠른 시작 (Windows)

관리자 권한 PowerShell에서 아래 한 줄을 실행하면 최신 버전을 설치하고 입력기를 활성화합니다:

```powershell
irm https://github.com/iamallyouneed/tacit/releases/latest/download/install.ps1 | iex
```

설치가 끝나면 브라우저에 **시작 안내 페이지**가 열려 사용할 언어(영어, 한국어, 일본어)를 고르고 켤 수 있습니다. 설치 후 **Win + Space**를 눌러 **Tacit** 또는 **Tacit (자동완성)**을 선택하면 즉시 사용할 수 있습니다.

설정 창을 열려면 시작 메뉴에서 **Tacit 설정**을 클릭하거나, 터미널에서 다음 명령을 실행합니다:

```powershell
tacit-config
```
