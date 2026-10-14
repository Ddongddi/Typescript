# Typescript 언어 팩 시스템 & 새 언어 추가 가이드

Typescript는 코어 엔진의 가벼움을 유지하고 프로세스 시작 속도를 극대화하기 위해, 언어별 데이터(트라이그램 확률표, 단어 사전, 한자 변환 테이블 등)를 별도의 **언어 팩(Language Pack)** 파일(`.pack`)로 분리하여 관리합니다.

[English](../en/language-packs.md) · [한국어 문서 홈](README.md)

---

## 1. 언어 팩 아키텍처

```
%LOCALAPPDATA%\Typescript\packs\ (Windows)
~/.local/share/Typescript/packs/ (Linux)
~/Library/Application Support/Typescript/packs/ (macOS)
├── en.pack          (~0.5 MB) - 영어 트라이그램 및 빈도 어휘 사전
├── ko.pack          (~0.4 MB) - 한국어 음절 트라이그램 및 단어 목록
└── ja.pack          (~6.0 MB) - 로마자 규칙 + 가나 트라이그램 + IPADIC 사전
```

### 왜 언어 팩 구조인가?
1. **초경량 메모리 발자국**: 각 앱 프로세스에 로드되는 DLL 파일 크기를 1MB 미만으로 유지합니다.
2. **선택적 다운로드**: 일본어처럼 용량이 큰 사전(약 10MB)은 일본어 사용을 원하는 유저만 내려받아 사용합니다.
3. **메모리 매핑(`mmap`)**: 모든 프로세스가 단일 물리 메모리 페이지만 공유하므로 시스템 리소스 낭비가 없습니다.
4. **쉬운 확장성**: 새로운 언어를 추가할 때 코어 로직의 변경을 최소화하고 팩 단위로 배포할 수 있습니다.

---

## 2. 언어 팩 포맷 사양

각 `.pack` 파일은 내부적으로 검증된 메타데이터와 바이너리 테이블 데이터로 구성됩니다.

### 2.1 `manifest.json`
언어 팩의 식별자, 버전, 테이블 파일 목록 및 SHA-256 해시를 정의합니다:

```json
{
  "id": "ja",
  "name": "日本語",
  "version": "0.8.1",
  "core": ">=0.8.0, <0.9.0",
  "requires": ["en"],
  "files": {
    "ipadic_compact.bin": "9c1e…",
    "kana_trigrams.bin": "4a8b…"
  }
}
```

### 2.2 바이너리 테이블 규격
- **리틀 엔디언(Little-Endian)** 정렬.
- 고정 크기 헤더와 인덱스 테이블: 포인터 역참조 없이 바이너리 오프셋 연산만으로 $O(1)$ 또는 이진 탐색 $O(\log N)$ 조회 가능.

---

## 3. 언어 팩 빌드 도구 (`typescript-packgen`)

원시 텍스트 데이터셋(단어 목록, n-gram 카운트 등)을 정규화된 팩 바이너리로 변환하는 전용 도구입니다.

### 3.1 팩 빌드 명령어
```powershell
# 모든 릴리스 팩 (en, ko, ja) 생성
cargo run -p typescript-packgen --bin build-packs -- crates/core/data target/packs
```

출력 디렉토리(`target/packs/`)에는 각 언어별 `.pack` 압축 파일과 전체 팩의 무결성 목록인 `packs.json`이 생성됩니다.

---

## 4. 새로운 언어 추가 절차 (Step-by-Step)

예를 들어 프랑스어(`fr`)나 독일어(`de`)를 추가하는 절차는 다음과 같습니다:

### 1단계: 원시 말뭉치 데이터 준비
1. 대규모 텍스트 코퍼스에서 정규화된 문자 트라이그램 빈도표(`trigrams.txt`)를 추출합니다.
2. 빈도 기준 상위 50,000개 어휘 목록(`vocabulary.txt`)을 준비합니다.
3. 데이터는 `crates/core/data/<lang>/` 폴더에 배치합니다.
4. **주의:** [라이선스 준수](#5-데이터-라이선스-요건)를 반드시 확인해야 합니다.

### 2단계: 언어 모듈 구현 (`crates/core/src/lang/<id>/`)
`LanguageModule`(`crates/core/src/lang/module.rs`)을 구현합니다. 키 입력에 대해 두 가지를 답합니다. 이 언어로 치면 어떤 글자가 되는지, 그 읽기가 깨끗한지입니다.

```rust
pub struct French;

impl LanguageModule for French {
    fn language(&self) -> Language { Language::French }
    /// 이 키들이 만드는 글자. 만들 수 없으면 None.
    fn compose(&self, keys: &str) -> Option<String> { /* … */ }
    /// 흩어진 조각이 없는가: 깨끗한 읽기가 있으면 Esc는 깨끗하지 않은 읽기를 건너뜁니다.
    fn is_clean(&self, keys: &str) -> bool { /* … */ }
}
```

### 3단계: 연결하기
새 언어는 **한 줄로 끝나지 않습니다.** 언어 이름이 등장하는 곳은 아래와 같습니다 (만나게 되는 순서).

1. `decide/engine.rs`: `Language` 변형과 `name` / `id` / `from_id` / `parse` / `sentence_language`.
2. `lang/registry.rs`: `MODULES`에 모듈 추가.
3. `decide/engine.rs` (`HeuristicEngine::decide`): 그 언어의 점수와 보정 필드. 언어마다 달라서, 아직 공통 점수기 트레잇은 없습니다.
4. `lib.rs` (`make_engine`)와 `lang/set.rs`: 켜진 언어 조합으로 엔진을 만들고 기본 순서를 정합니다.
5. `lang/pack.rs` (`embedded()`): 팩 파일과 `requires`(필요한 다른 팩). 손상된 파일이 읽기에서 패닉을 낼 수 있으면 `usable()` 검사도 추가합니다.
6. `session/`: 읽기 이상이 필요한 언어만 (일본어는 변환 창이 있어 `session/japanese.rs` 가 따로 있습니다).
7. 해당되면 Tab 자동완성(`store/history.rs` 의 `suggest`)과 대시보드 언어 목록.

### 4단계: `packgen` 빌더 등록
`crates/packgen/src/main.rs`에 해당 언어의 원시 데이터를 바이너리 테이블로 컴파일하는 규칙을 추가합니다.

### 5단계: 정확도 및 골든 테스트 검증
새 언어가 추가됨에 따라 기존 영어/한국어 판별 정확도에 악영향을 주지 않는지 검증합니다:

```powershell
cargo test -p typescript-core --test accuracy
cargo test -p typescript-core --test golden
```

---

## 5. 데이터 라이선스 요건

Typescript는 오픈소스 프로젝트로서 라이선스 순수성을 엄격히 유지합니다.

1. **상용 재배포 허용 라이선스**: CC-BY, CC BY-SA, MIT, BSD, Apache-2.0 등 자유로운 재배포 및 수정이 가능한 데이터만 수용합니다.
2. **비상업적(NC) 데이터 사용 금지**: CC-BY-NC 또는 비상업적 용도로 제한된 데이터셋은 일체 포함할 수 없습니다.
3. **고지 의무**: 추가된 모든 원본 데이터셋의 출처, 저작권자, 라이선스 전문은 루트의 [THIRD_PARTY_NOTICES.txt](../../THIRD_PARTY_NOTICES.txt)에 명시해야 합니다.
