# Typescript 아키텍처 & 동작 원리

Typescript는 OS에 종속되지 않는 순수 Rust 기반의 핵심 코어(`crates/core`)와, 각 운영체제별 입력 프레임워크와 결합하는 플랫폼 어댑터(`platforms/`) 계층으로 엄격히 분리되어 있습니다.
 
[English](../en/architecture.md) · [한국어 문서 홈](README.md)

---

## 1. 전체 시스템 구조

```mermaid
flowchart TD
    subgraph Platforms [플랫폼 계층]
        WIN[Windows TSF<br/>x64/x86 COM DLL]
        AND[Android IME<br/>JNI + Java Service]
        MAC[macOS<br/>InputMethodKit]
        LNX[Linux<br/>IBus Engine]
    end

    subgraph Core [crates/core (OS 독립적 Rust 코어)]
        subgraph SessionLayer [세션 계층: session/]
            SESSION[InputSession 상태 머신]
            WORD[단어 조합 버퍼 WordBuffer]
            FLIP[확정 직후 Esc 변환 FlipBuffer]
            JP_SESSION[일본어 변환 세션 JapaneseSession]
        end

        subgraph DecideLayer [판별 계층: decide/]
            LEARN[사용자 교정 사전<br/>LearningEngine]
            MIXED[영문+조사 분리<br/>SplittingEngine]
            ENGINE[점수 기반 판별 엔진<br/>DecisionEngine]
            KANJI[한자 사전 엔진<br/>KanjiDictionary / IPADIC]
        end

        subgraph LangLayer [언어 모듈 계층: lang/]
            REG[Language Registry]
            EN[English Model<br/>트라이그램 + 사전]
            KO[Korean Model<br/>두벌식 오토마타 + 한국어 트라이그램]
            JA[Japanese Model<br/>로마자-가나 변환 + 가나 트라이그램]
            PACK[Language Pack Loader<br/>mmap 기반 읽기]
        end

        subgraph StoreLayer [스토리지 계층: store/]
            POLICY[앱별 정책/설정 Settings & AppPolicy]
            HIST[단어 빈도 이력 History]
        end
    end

    Platforms -->|키 이벤트 (Key) + 컨텍스트| SESSION
    SESSION -->|단어 평가 요청| LEARN
    LEARN --> MIXED
    MIXED --> ENGINE
    ENGINE --> REG
    REG --> EN & KO & JA
    PACK --> EN & KO & JA
    JP_SESSION --> KANJI
    SESSION -->|텍스트 편집 명령 (Insert/Replace)| Platforms
```

---

## 2. 입력 세션 상태 머신 (`session/`)

사용자가 타이핑을 시작하면, 플랫폼 계층은 키 입력을 추상화된 `session::Key` 이벤트로 변환하여 `session::InputSession`에 전달합니다.

### 상태 전이 흐름
1. **입력 중 (Composing)**:
   - 영문자 키(`a-z`, `A-Z`)가 입력되면 현재 단어 버퍼(`WordBuffer`)에 누적됩니다.
   - 단어가 갱신될 때마다 `decide` 엔진이 현재까지의 입력을 평가하여 점선 밑줄(미리보기, Preview)로 표시할 후보 텍스트를 생성합니다.
2. **단어 경계 및 확정 (Commit)**:
   - `Space`, 줄바꿈(`Enter`), 구두점(`.`, `,`, `?`, `!` 등)을 누르면 현재 단어가 최종 결정된 언어로 확정(Commit)됩니다.
   - 확정된 단어는 `FlipBuffer`에 저장되어 바로 뒤에 이어지는 `Esc` 키로 취소/재변환할 수 있습니다.
3. **Esc 순환 (Cycling)**:
   - 입력 도중 `Esc`를 누르면 현재 단어의 가능한 후보 읽기가 순환합니다:
     $$\text{영어} \longrightarrow \text{영어 + 조사} \longrightarrow \text{한국어} \longrightarrow \text{일본어(활성화 시)}$$
   - 확정한 직후(커서가 단어 바로 뒤에 위치할 때) `Esc`를 눌러도 확정된 글자를 지우고 다음 후보로 교정합니다.
4. **동적 학습**:
   - 사용자가 동일한 단어에 대해 동일한 Esc 교정을 2회 수행하면, `LearningEngine`이 이를 `lexicon.json`에 저장하여 이후 타이핑 시 최우선 순위로 자동 적용합니다.

---

## 3. 언어 판별 엔진 파이프라인 (`decide/`)

판별 엔진은 단어 하나가 들어왔을 때 다음과 같은 다단계 파이프라인을 거쳐 언어(`Language::English`, `Language::Korean`, `Language::Japanese`)를 확정합니다.

### 1단계: 사용자 교정 사전 (`LearningEngine`)
- 사용자가 직접 `Esc`로 교정한 이력(`lexicon.json`)에 일치하는 단어가 있다면, 점수 계산을 생략하고 저장된 언어로 즉시 결정합니다.

### 2단계: 영어·한국어 붙여쓰기 분할 (`SplittingEngine`, `CodeSwitch`)
- 예: `githubdp` → `github` + `에`, `reportwha` → `report` + `좀`, `kakaotalkaptlswj` → `kakaotalk` + `메신저`, `nwp` → `n` + `제`
- 토큰을 **단어**의 경로로 읽는 모든 방법을 같은 척도(로그 확률)로 점수 매기고 가장 높은 경로를 고릅니다(`decide/codeswitch.rs`, 단어 격자 위의 비터비 탐색). 경우별 규칙은 없고 토큰은 길이와 상관없이 통째로 탐색하므로, 띄어쓰기 없이 친 이야기 한 편도 `githubdp`와 같은 방식으로 잘립니다.
  - 간선: 사전에 있는 영어 단어(로그 빈도), 사전에 없는 영어 구간(글자 트라이그램에서 고정 비용을 뺌), 사전에 있는 한국어 단어, 조사·어미, 어느 단어에도 없는 한국어 한 음절(그 음절의 확률, 본 적 없는 음절은 자모 확률). 한국어 간선은 조합했을 때 온전한 음절이어야 하고(`ㅏ마맷미`는 불가), 한 번도 본 적 없는 음절은 한 번만 본 음절들이 가리키는 작은 질량(Good-Turing)만 나눠 가지므로 드문 음절은 흔한 음절보다 훨씬 비쌉니다. 한 글자·대문자 약어(`n`, `A`, `QR`)는 맨 앞에서만 라벨입니다. 한 글자 영어 단어(`a`, `I`)는 한글 음절 사이에 끼지 못합니다.
  - 연결: 영어와 한국어 사이를 넘을 때마다 비용을 내고, 단어가 받쳐 주지 않는 간선에 닿으면 더 냅니다. 라벨 뒤에는 라벨이 이름 붙이는 명사(토큰이 더 이어지면 조금 더 비쌈: `B급의`)나 격조사만 올 수 있고, 격조사는 붙을 말(영어, 한국어 단어, 음절 연쇄: 단어도 아닌 한 음절은 안 됨)이 있을 때만 한국어의 증거가 됩니다. 영어로 시작해 한국어로 넘어가는 경로는 직전 단어가 한국어였다면 정해진 값을 내지만(`lead_en`), 처음부터 끝까지 영어인 읽기와 비교할 때는 내지 않습니다.
  - 전환이 있는 최선의 경로가 단일 언어 경로보다 `margin` 이상 앞서면 `Mixed`입니다. 그 경로의 한국어가 한국어 단어·격조사·라벨 명사·자주 쓰는 한 음절 단어 어느 것으로도 받쳐지지 않으면(aortosclerosis의 `맥`) 더 큰 차이를 요구합니다. 아니면 3단계로 넘깁니다.
- 가중치는 손으로 고르지 않고 `lab`의 `--tune`(좌표 하강)과 문장 단위 탐색으로 맞춥니다. `lab/src/tune.rs`를 보세요.
- 키를 칠 때마다 느려지지 않도록, 전체가 영어 단어이거나 깨끗한 한국어인 토큰, 타이핑 중이면 사전에 있는 영어 단어의 앞부분인 토큰은 건너뛰고 마지막에 친 토큰의 격자를 보관합니다. 키 하나는 마지막 48키만 다시 탐색합니다(1600키에서 약 0.6ms, 전에는 토큰 전체를 78ms). 시작 키마다 필요한 것은 처음부터 다시 묻지 않고 한 글자씩 따라가며 이어 갑니다. 영어 단어 목록과 글자 모델(`Walk`), 한글 조합과 한국어 단어 목록의 범위 좁히기(`KoWalk`)가 그렇습니다. 점수는 마지막 몇 위치만, 경로의 출처는 전부 보관하고, 한꺼번에 들어온 긴 토큰은 조각으로 나눠 탐색하므로 토큰이 아무리 길어도 메모리가 늘지 않습니다.
- `MixedWord`(앞은 영어, 뒤는 조사·어미)는 학습 엔진과 Esc 순환이 쓰는 단순한 꼴로 남아 있습니다.

### 3단계: 점수 기반 휴리스틱 판별 (`HeuristicEngine`)
혼합 단어가 아닌 일반 단어는 통계적 언어 모델을 통해 점수를 계산합니다.
- **문자 트라이그램(Trigram) 확률**:
  단어의 연속된 3글자 빈도 확률을 기반으로 언어별 로그 우도(Log-likelihood)를 계산합니다.
- **단어 사전 매칭**:
  - 영어 빈도 사전 (수만 개 영단어)
  - 한국어 표제어 사전 (두벌식 역변환 결과가 한국어 형태소 사전에 존재하는지 확인)
- **컨텍스트 반영 (`DecisionContext`)**:
  - 직전 단어가 어떤 언어였는지(언어 지속성 가중치)
  - 직전 단어의 확신도가 어떠했는지를 감안하여 스무딩(Smoothing) 처리합니다.
- **임계값(`conversionThreshold`)**:
  - 기본값은 `0.5`이며, 터미널 같은 환경에서는 오변환을 방지하기 위해 `0.97` 등 엄격한 기준을 적용할 수 있습니다.

---

## 4. 언어별 처리 및 오토마타 (`lang/`)

### 4.1 한국어 (두벌식 오토마타, `lang/hangul.rs`)
- 초성, 중성, 종성 조합 상태 머신을 내장하여, 한글 입력 시 실시간으로 완성형 유니코드 음절(`U+AC00` ~ `U+D7A3`)을 조합합니다.
- 복자음(`ㄳ`, `ㄵ`, `ㄶ`, `ㄺ` 등) 및 복모음(`ㅘ`, `ㅙ`, `ㅚ`, `ㅝ` 등)의 입력과 백스페이스 분해를 표준 두벌식 자판 규칙대로 완벽히 처리합니다.

### 4.2 일본어 (로마자 및 한자 변환, `lang/romaji.rs`, `decide/kanji.rs`)
- **로마자-가나 변환**: QWERTY 자모 입력을 헵번식(Hepburn) 및 훈령식 로마자 규칙에 따라 히라가나/가타카나로 실시간 매핑합니다. 촉음(`tt` → `っ`), 발음(`nn` → `ん`) 규칙을 지원합니다.
- **IPADIC 한자 변환**: 일본어 단어 확정 시 `Space`를 누르면 IPADIC(39만 표제어 및 연접 비용 행렬) 기반의 Viterbi 최적 경로 탐색을 거쳐 한자 변환 후보 목록을 제공합니다.
- **빈도 가중치 보정**: `wordfreq`의 Zipf 스코어를 결합하여 실생활에서 자주 쓰이는 표기가 최상위 후보에 오르도록 보정합니다.

---

## 5. 메모리 맵 기반 언어 팩 (`lang/pack.rs`)

Typescript는 모든 앱 프로세스(브라우저, 터미널, 오피스 등)에 DLL 형태로 주입되어 실행됩니다. 따라서 각 인스턴스의 메모리 사용량과 시작 속도가 극도로 중요합니다.

- **컴팩트 바이너리 포맷**: 트라이그램 테이블과 사전 데이터는 런타임에 파싱하지 않고, 컴파일/패키징 시점(`crates/packgen`)에 바이트 정렬된 바이너리 테이블로 빌드됩니다.
- **`mmap` (Memory Mapping)**: DLL 로드 시 언어 팩 파일들을 메모리에 직접 매핑하므로, 수십 개의 프로세스가 Typescript를 로드하더라도 실제 물리 RAM은 한 카피만 공유됩니다.
- **모듈러 아키텍처**: 영/한 기본 모듈 외에 대용량 사전이 필요한 일본어(약 10MB) 등은 별도의 확장 언어 팩으로 분리되어 필요한 사용자만 선택적으로 로드합니다.

---

## 6. 스토리지 및 설정 (`store/`)

- `%APPDATA%\Typescript\` (Windows) 또는 `~/.config/Typescript/` (Linux) 아래에 다음 파일들이 관리됩니다:
  - `settings.json`: 전역 동작 옵션 (활성 언어 순서, 자동완성 여부, 엔터키 동작 등)
  - `apps.json`: 실행 파일별 커스텀 규칙 (임계값, 활성화 여부, 키 바이패스)
  - `lexicon.json`: 사용자의 Esc 교정을 통해 학습된 커스텀 단어 매핑
  - `user_history.json`: 자동완성에 사용되는 사용자 입력 단어 빈도 카운터 (2회 이상 입력 시 제안)
