# azooKey 의미 접속 점수 흡수 조사

2026-10-18. Mozc 사전 기반 Rust 후보 탐색 뒤에 azooKey의 의미 접속 점수를 붙일 때 필요한 원본 동작과 한계를 정리했다. 이는 통합 성능 평가 결과가 아니라 고정 소스와 데이터의 조사 결과다.

## 고정 입력

- 변환기: [`azooKey/AzooKeyKanaKanjiConverter`, `d59a28e4c7ca049aef04f29a91eae9677a7753f2`](https://github.com/azooKey/AzooKeyKanaKanjiConverter/tree/d59a28e4c7ca049aef04f29a91eae9677a7753f2).
- 사전: [`ensan-hcl/azooKey_dictionary_storage`, `4d418525b090cf49c219819d05a7e3cc2a4346eb`](https://github.com/ensan-hcl/azooKey_dictionary_storage/tree/4d418525b090cf49c219819d05a7e3cc2a4346eb).
- 비교 사전: Mozc `0136c35782536cb43d710575c311cd4185982738`의 기본 사전 10개. 아래 매핑 비율은 필터·보조 사전을 적용한 배포 팩 전체 비율과 다르다.

변환기 코드는 [MIT](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/LICENSE), 사전 저장소는 [Apache-2.0](https://github.com/ensan-hcl/azooKey_dictionary_storage/blob/4d418525b090cf49c219819d05a7e3cc2a4346eb/LICENSE)다. 데이터 흡수에는 사전 라이선스·출처 고지가 별도로 필요하다.

## 원래 계산

변환 점수는 큰 쪽이 우선이다. `processClauseCandidate`는 기존 단어·품사 접속 누적 점수에 **문절 대표 MID의 방향성 있는 접속 점수 합**을 계수 1로 더한다. 초기 MID는 500이고 마지막 문절 뒤에 별도 EOS 항은 더하지 않는다. [원본 계산](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/ConversionAlgorithms/Kana2Kanji.swift#L27-L37)

`semantic_score = Σ MM(previous_clause_mid, clause_mid)`

`MM(a,b)`는 row-major `mm[a * 502 + b]`다. 어느 쪽이든 MID 500이면 파일의 값 대신 0을 반환한다. BOS·EOS가 모두 500이고, 일반 MID는 501이다. **501을 결측값처럼 0 처리하면 원본과 달라진다.** `MIDData.totalCount`는 503이지만 실제 행렬 차원과 인덱싱은 502이므로 그 상수를 그대로 파일 규격으로 사용하면 안 된다. [조회·로드](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/DictionaryManagement/DicdataStore.swift), [MID 정의](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/DictionaryManagement/Entry/MIDData.swift)

원 저자는 사전 점수를 대략적인 로그 우도라고 설명한다. 로그 밑이나 Mozc 비용과의 환산 계수는 이 소스에서 확정하지 못했다. Mozc의 작은 값 우선 비용에 섞을 때는 부호를 반대로 하고 가중치를 별도 검증해야 한다. [원 저자의 비용 설계 설명](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Docs/failures.md#L9-L19)

## 문절 구성: 인접 단어 점수 합과 다르다

`RegisteredNode.getCandidateData`는 다음 규칙으로 경로를 문절로 묶는다. [원본 문절 추출](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/ConversionAlgorithms/RegisteredNode.swift#L145-L190)

- 직전 노드 `rcid`와 다음 노드 `lcid`를 `isClause`에 전달한다. 첫 문절이 비었거나 경계가 아니면 같은 문절이다.
- 같은 문절에서 기존 대표 MID가 500이고 새 MID가 다르거나, 새 노드가 의미 계산 대상이면 대표 MID를 새 노드의 것으로 바꾼다. 따라서 여러 내용어가 같은 문절에 있으면 마지막 대상이 대표가 된다.
- 새 문절에서는 의미 계산 대상일 때만 해당 MID를 대표로 쓰고, 그 외에는 500으로 시작한다.

품사 ID는 **azooKey의 ID 공간**이다. Mozc ID에 숫자 범위를 직접 적용하면 틀린다. 내용어 타입 1 외에 비자립 동사 895–1280, 비자립 명사 1297–1305도 의미 계산 대상이다. `isClause`는 BOS/EOS가 양쪽 중 하나면 false이고, 다음 타입이 전치(0)나 내용(1)일 때 직전 타입이 전치가 아니면 true다. 그 외에는 false다. 전치 ID는 `{1315,6,557,558,559,560}`, 내용 ID는 `561..<868`, `1283..<1297`, `1306..<1310`, `11..<53`, `555..<557`, `1281..<1283`, `{1314,3,2,4,5,1,9}`, BOS/EOS는 `{0,1316}`, 나머지는 후치다. [분류·경계·대상 함수](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/DictionaryManagement/DicdataStore.swift#L1008-L1080)

## 로컬 실측

카타카나 U+30A1–U+30F6을 히라가나로 바꾼 `(reading, surface)`를 기준으로 고정된 `loudstxt3` 파일을 파싱했다. 동일 키의 대표는 **azooKey 단어 점수가 가장 큰 행**으로 택했다. 원래 변환기는 문맥별 행을 모두 보유하므로 이 선택 자체가 근사다.

| 항목 | 결과 |
|---|---:|
| azooKey 서로 다른 키 | 503,220 |
| 같은 키의 추가 행 | 62,994 |
| 서로 다른 MID를 갖는 키 | 6,679 (1.33%) |
| Mozc 기본 사전 행 | 1,289,076 |
| azooKey 키와 정확히 맞는 Mozc 행 | 364,134 (28.25%) |
| 맞는 서로 다른 키 | 274,352 |
| 다중 MID 키에 해당하는 Mozc 행 | 15,157 |
| 대표 MID가 일반(501)인 맞는 Mozc 행 | 125,071 |

파일 파싱 규격은 [원본 LOUDS 파서](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/DictionaryManagement/DataStructure/extension%20LOUDS.swift#L97-L115)와 대조했다. 위 비율은 **사전 항목 빈도**이며 실제 사용자 입력의 토큰 커버리지가 아니다.

`mm.binary`는 1,008,020 bytes, float32 252,005개다. 앞의 502²=252,004개만 행렬이며 마지막에는 -30 하나가 더 있다. 생성기가 마지막 빈 줄도 변환하는 방식과 일치한다. 가져올 때 정확히 502²개의 값만 읽되, 추가 float가 있는 이 고정 파일을 잘못 거부하지 않아야 한다. [행렬 생성기](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/CliTool/Subcommands/DictCommands/BuildCommand.swift#L153-L166)

500행·500열을 제외한 MM의 최소/10%/중앙/90%/최대는 **-27.4693 / -10.7780 / -7.9739 / -5.4978 / -0.3873**이다. 모든 125,751개의 서로 다른 ID 쌍에서 방향을 뒤집으면 값이 달랐다. 대칭화하면 정보가 사라진다. 데이터 파일은 [고정 원본](https://github.com/ensan-hcl/azooKey_dictionary_storage/blob/4d418525b090cf49c219819d05a7e3cc2a4346eb/Dictionary/mm.binary)이다.

재현용 분석 스크립트와 원시 통계는 작업 캐시 `temp/japanese-research/semantic_measure.py`, `semantic-measure.json`에 있다. 정식 제품 생성기에 포함한 파일은 아니다.

## 구현에 대한 판단

가장 작은 실험은 기존 Mozc 경로 후보를 유지하면서 매핑된 azooKey MID 점수로 재정렬하는 것이다. 원본에 가깝게 하려면 MID뿐 아니라 azooKey lcid/rcid를 보관하여 문절 추출도 적용한다. 매핑하지 못한 노드의 의미·문절을 어떻게 처리할지는 원본이 정해 주지 않으므로 별도 정책이다.

특히 매핑된 항 사이의 음수 점수만 무조건 더하면, 매핑이 적거나 문절이 적은 경로가 유리해질 수 있다. 결측 처리·길이 효과·가중치를 개발 세트에서 확인하고, 독립 세트의 Top-1/Top-5·CER·지연으로 채택 여부를 판단해야 한다. 동일 키의 모든 MID 중 접속점수가 좋은 것만 매번 고르면 문맥상 불가능한 의미를 낙관적으로 고르는 또 다른 근사가 된다.

8개 후보의 재정렬은 그 안에 없는 정답을 만들지 못한다. 단어 빈도상 28.25% 매핑만으로 문맥 개선 폭을 보장할 수 없으며, 본 조사는 통합 코드·정확도 벤치마크·실기기 검증을 수행하지 않았다.
