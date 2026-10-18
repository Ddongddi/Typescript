# Japanese engine sources: Mozc and azooKey

Research date: 2026-10-18. This is a source/data comparison for absorbing upstream work into Typescript's Rust engine, not an end-to-end benchmark of the upstream products. Sizes and counts below were measured from the pinned checkouts, not taken from marketing claims.

## Decision

Use **Mozc's OSS lexicon and its matching connection costs as the first native-Rust replacement for IPADIC**. Preserve Typescript's composition, language decision, UI and platform adapters. The upstream source is plain text, the cost units already fit signed 16-bit integers, and it supplies 1,289,076 entries versus the 566,214 entries measured in the current azooKey default dictionary. Vocabulary count is not an accuracy result: require the existing lab cases and the independent AJIMEE evaluation before making quality claims.

Borrow azooKey's incremental lattice reuse and explicit content-word context scoring only when measurements show the corresponding bottleneck. Its neural Zenzai path is promising for later reranking, but adding Swift plus llama.cpp plus model downloads now would not be an absorption of the existing Rust engine. It also changes the size and hardware budget substantially. Do not combine Mozc and azooKey POS IDs or word costs directly: their models have different ID spaces and cost scales.

Primary sources: [Mozc dictionary description](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/data/dictionary_oss/README.txt), [azooKey converter algorithms](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Docs/conversion_algorithms.md), [azooKey dictionary data](https://github.com/azooKey/azooKey_dictionary_storage/tree/4d418525b090cf49c219819d05a7e3cc2a4346eb/Dictionary).

## Pinned inputs and reproducible downloads

| Component | Revision | Download |
|---|---|---|
| Mozc | `0136c35782536cb43d710575c311cd4185982738` | [source archive](https://codeload.github.com/google/mozc/tar.gz/0136c35782536cb43d710575c311cd4185982738) |
| azooKey iOS app | `b50db4aec1069a8d2341f70415da3bdc61f1fce6` | [source archive](https://codeload.github.com/azooKey/azooKey/tar.gz/b50db4aec1069a8d2341f70415da3bdc61f1fce6) |
| AzooKeyKanaKanjiConverter | `d59a28e4c7ca049aef04f29a91eae9677a7753f2` | [source archive](https://codeload.github.com/azooKey/AzooKeyKanaKanjiConverter/tar.gz/d59a28e4c7ca049aef04f29a91eae9677a7753f2) |
| azooKey default dictionary (converter submodule revision) | `4d418525b090cf49c219819d05a7e3cc2a4346eb` | [source archive](https://codeload.github.com/azooKey/azooKey_dictionary_storage/tar.gz/4d418525b090cf49c219819d05a7e3cc2a4346eb) |
| AJIMEE-Bench | `401666cd56d1a570c2021798b64b6da4396bfd45` | [source archive](https://codeload.github.com/azooKey/AJIMEE-Bench/tar.gz/401666cd56d1a570c2021798b64b6da4396bfd45) |

Local inspection copies live under `temp/japanese-research/` and are not production dependencies. The converter's dictionary revision is an explicit gitlink, not an assumed compatible latest revision. See [converter submodules](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/.gitmodules).

## Mozc: direct data absorption

Files under `src/data/dictionary_oss/`:

| Input | Measured shape / size |
|---|---|
| `dictionary00.txt` through `dictionary09.txt` | UTF-8 TSV, 60,482,646 bytes total; 1,289,076 rows; 745,965 unique readings |
| `connection_single_column.txt` | 36,428,642 bytes; first line `2672`, followed by 7,139,584 integer costs |
| `id.def` | 151,747 bytes; POS/context ID definitions, IDs up to 2671 |
| `evaluation.tsv` | 61,534 bytes; upstream expected results and tests, not a simple input/exact-gold table |
| `reading_correction.tsv` | 29,590 bytes; correction resources, separate from lexicon |

A base dictionary row has `reading<TAB>lid<TAB>rid<TAB>cost<TAB>surface`, sometimes additional columns. Word costs measured 0..18318. Matrix costs measured 0..15281. All fit `i16`, but path totals must remain at least `i32`. Matrix layout is explicitly `cost[previous_rid * 2672 + next_lid]`: [ConnectionFileReader](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/data_manager/connection_file_reader.h). A dense i16 matrix costs exactly **14,279,168 bytes**; there is no need to port Mozc's compressed C++ connector to make the first functional native model.

The raw dictionary has up to **296 rows for one reading** (`こうじ`), and 259 for `ゆうき`. Exact tuple dedup still leaves a maximum 286, while dedup solely by surface still leaves 261 and loses context distinctions. Thus a u8 per-reading count cannot faithfully store this model. IDs require 12 bits. Maximum reading length is 33 Unicode scalar values. There are **66 rows with non-BMP surface characters**; reject silent u16 truncation. No surface exceeded 255 Unicode scalar values in this revision. Validate the generated pack and round-trip these boundary cases before using it.

Mozc's actual dictionary generation also applies `dictionary_filter.tsv`, expands `aux_dictionary.tsv` and `src/data/dictionary_manual/`, and can add Japanese postal codes. Importing only the ten base files is a documented subset, not the identical shipped Mozc vocabulary. Prefer running/porting those small source-generation steps if the full model is desired: [dictionary BUILD rules](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/data/dictionary_oss/BUILD.bazel), [manual dictionary](https://github.com/google/mozc/tree/0136c35782536cb43d710575c311cd4185982738/src/data/dictionary_manual).

Useful algorithms beyond dictionary import are shortest-path lattice costs with BOS/EOS transitions, N-best alternatives, POS-based clause boundaries, and context-preserving conversion. These are separable from the app UI: [immutable converter](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/converter/immutable_converter.cc), [N-best generator](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/converter/nbest_generator.cc), [boundary rules](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/data/rules/boundary.def).

## azooKey: smaller traditional model, richer context and optional neural model

The app is separate from the reusable Swift package. The package's checked-in CI describes macOS, Ubuntu, Windows and Android builds; Android includes x86_64, aarch64 and armv7, with emulator tests on x86_64. Therefore the converter should not be described as Apple-only. A Rust absorption of data/algorithms can reuse Typescript's existing four-platform core without requiring Swift at runtime. Actual target verification remains Typescript's responsibility: [CI workflow](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/.github/workflows/swift.yml).

Measured default dictionary resources:

| Directory/file | Bytes | Meaning |
|---|---:|---|
| `Dictionary/louds/` | 21,914,915 | Compressed tries, labels and entry blocks |
| `Dictionary/louds/*.loudstxt3` alone | 21,161,376 | 566,214 actual dictionary entries |
| `Dictionary/cb/` | 1,176,976 | Sparse POS connection scores |
| `Dictionary/p/` | 1,253,823 | Prediction resources |
| `Dictionary/mm.binary` | 1,008,016 | 502 x 502 float32 semantic/content-word scores |
| **All `Dictionary/`** | **25,353,734** | Counts include zero-length `.gitkeep` files |

Entry scores measured -48.75..-1.56340003, with larger (less negative) preferred. CID IDs reach 1318 and MID IDs 501. BOS is CID 0 but EOS is **1316**, unlike a model that treats both as 0. The IDs are IPADIC plus custom extensions, according to the author's [design mistakes document](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Docs/failures.md). Do not round these into Mozc costs without an explicit model conversion.

A dictionary exporter can read entry blocks directly without implementing LOUDS traversal. Each `.loudstxt3` file is little-endian: `u16 block_count`, then `u32 offsets[block_count]`. Each block is `u16 entry_count`, followed by `(u16 lcid, u16 rcid, u16 mid, f32 score)` for every entry, then UTF-8 tab-separated fields: one shared ruby reading followed by one surface per entry. An empty surface means the ruby itself. Reading this format across the pinned files found no block with an inconsistent number of string fields. See [binary reader](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/DictionaryManagement/DataStructure/extension%20LOUDS.swift).

`cb/<previous_rcid>.binary` contains `(i32 next_lcid, f32 score)` pairs. First key -1 supplies the row default; later pairs override individual cells. Missing rows default to -25 in the reader. `mm.binary` is row-major f32 with 502 columns, with MID 500 treated specially as zero connection. See [DicdataStore](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/KanaKanjiConverterModule/DictionaryManagement/DicdataStore.swift).

azooKey uses Viterbi, specialized incremental paths for append/delete/change, and semantic/content-word bigram scoring across clauses. Keeping only its surface vocabulary discards those advantages. Its non-neural model is not established as more accurate than Mozc by this source inspection. [Algorithm explanation](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Docs/conversion_algorithms.md).

### Neural option: measured download budget

Current app gitlinks point to the following Apache-2.0 tagged model repositories. Hugging Face API metadata was inspected on the research date; the weights were not downloaded or executed.

| Model | Pinned revision | Q5_K_M GGUF bytes | Parameters | Context |
|---|---|---:|---:|---:|
| zenz-v3.2-xsmall | `4f5423f0fad41a73b1242eb96fe5c12ae4fdca83` | 20,970,304 | 25,583,616 | 1024 |
| zenz-v3.2-small | `c67e03e07d215c869f591b274c1631170d3e11fe` | 73,871,936 | 95,058,432 | 1024 |

Direct weight URLs: [xsmall GGUF](https://huggingface.co/Miwa-Keita/zenz-v3.2-xsmall-gguf/resolve/4f5423f0fad41a73b1242eb96fe5c12ae4fdca83/ggml-model-Q5_K_M.gguf), [small GGUF](https://huggingface.co/Miwa-Keita/zenz-v3.2-small-gguf/resolve/c67e03e07d215c869f591b274c1631170d3e11fe/ggml-model-Q5_K_M.gguf). Metadata: [xsmall API](https://huggingface.co/api/models/Miwa-Keita/zenz-v3.2-xsmall-gguf?blobs=true), [small API](https://huggingface.co/api/models/Miwa-Keita/zenz-v3.2-small-gguf?blobs=true).

The xsmall SHA-256 is `00c64b3d318045a708d0cad5434faccab10f5481a49e6362864551fd0995fa58`; small is `29c223d4c23327b80fd13ebb5ab2555057a46317997d5da391584ffbef0db673` (LFS metadata). Actual peak memory also includes KV cache and inference buffers. The converter explicitly pins llama.cpp b4846 and documents a newer build's +63.77 MiB repack allocation and device regression; this is upstream-specific evidence of the need for real hardware tests, not a prediction of Typescript latency. Default package traits leave Zenzai disabled. [Package.swift](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Package.swift), [Zenzai API](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Docs/zenzai.md).

## Evaluation data and scoring

### Mozc regressions: development suite

Pinned raw files:

- [oss.tsv](https://raw.githubusercontent.com/google/mozc/0136c35782536cb43d710575c311cd4185982738/src/data/test/quality_regression_test/oss.tsv)
- [regression.tsv](https://raw.githubusercontent.com/google/mozc/0136c35782536cb43d710575c311cd4185982738/src/data/test/quality_regression_test/regression.tsv)
- [evaluation.tsv](https://raw.githubusercontent.com/google/mozc/0136c35782536cb43d710575c311cd4185982738/src/data/dictionary_oss/evaluation.tsv)

The first two are source tests: `label, reading, target-or-regex, command, optional rank...`. `evaluation.tsv` prepends upstream status and actual output, so its third column is not universally an exact gold answer. This pinned generated file contains 304 plain `Conversion Expected`, 47 `Expected 2`, 91 `Expected 9`, 50 `Expected 99`, 24 `Expected 999`, 53 regex `Conversion Match`, 37 regex `Conversion Not Match`, plus several smaller groups and prediction/suggestion tests. Ranks are **zero-based thresholds** upstream: `Expected 2` permits actual rank <=2, not top2. Upstream rank is computed over clause candidates, so Typescript flat whole-string top-k should be reported as its own metric rather than claimed identical. See [quality regression parser/evaluator](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/converter/quality_regression_util.cc).

Keep prediction and suggestion commands out of a conversion-only score; report excluded counts. Use source regressions as development data because the dictionary has been tuned against them. Do not train overrides from the exact benchmark answers and then call that a generalization result.

### AJIMEE-Bench: independent evaluation suite

Pinned data URL: [evaluation_items.json](https://raw.githubusercontent.com/azooKey/AJIMEE-Bench/401666cd56d1a570c2021798b64b6da4396bfd45/JWTD_v2/v1/evaluation_items.json). Size 134,570 bytes; 200 cases; 100 have left context; 83 have multiple accepted outputs, up to 14. Maximum input is 117 Unicode scalar values. Data was derived from Kyoto University's Japanese Wikipedia typo dataset v2 test split, sampled for kana-kanji conversion errors and manually checked, according to [benchmark provenance](https://github.com/azooKey/AJIMEE-Bench/blob/401666cd56d1a570c2021798b64b6da4396bfd45/README.md).

Each object has `index`, `context_text`, `input` (katakana reading), `expected_output` (array of acceptable full strings), `original_text`, and `splitted_input_for_limited_input_length`. Convert the reading from katakana to hiragana for the current Typescript API; preserve punctuation and the accepted gold strings. Do not use `original_text` as the answer, because the test input is only a slice of that text.

Report at least exact top1 and top5 membership across all accepted answers, plus average minimum CER (Unicode-scalar Levenshtein distance / gold length, minimum across accepted references). Upstream utilities implement top1 and minimum CER: [utils.py](https://github.com/azooKey/AJIMEE-Bench/blob/401666cd56d1a570c2021798b64b6da4396bfd45/utils.py). Score all 200 and the 100 no-context cases separately. If Typescript ignores left context, explicitly label that condition; do not imply a context-aware comparison. Keep the long inputs or report truncation exclusions; silently sampling short cases would hide the challenge. The split suggestions are for constrained engines, not permission to modify gold references arbitrarily.

The converter's `anco evaluate` documentation mentions TSV, but its current implementation actually decodes JSON objects with `query`, `answer`, `tag`, optional `left_context`, `right_context`, and `user_dictionary`; follow [EvaluateCommand.swift](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/Sources/CliTool/Subcommands/EvaluateCommand.swift) when doing an upstream execution comparison.

### Long-text speed corpus

Mozc's [stress sentences](https://raw.githubusercontent.com/google/mozc/0136c35782536cb43d710575c311cd4185982738/src/data/test/stress_test/sentences.txt) are readings of Natsume Soseki's expired-copyright text via Aozora Bunko. The file identifies the source, preparation and public-domain status. They are suitable for length/p95/p99 stress testing, not exact conversion accuracy because there are no paired gold surfaces in this file. Keep literary text results separate from modern everyday-input fixtures.

## Licensing and redistribution provenance

This is a record of upstream notices to carry with copied material, not a substitute for license texts.

| Material | Upstream license / required provenance |
|---|---|
| Mozc Google-authored code | BSD-3-Clause; retain copyright, conditions and disclaimer and avoid endorsement. [LICENSE](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/LICENSE) |
| Mozc dictionary and data/test/dictionary | Mixed: IPADIC/NAIST + ICOT notices, Okinawa public-domain dictionary, Google additions. Carry the complete dictionary README alongside the Google code license. The OSS dictionary excludes Google's large proprietary web vocabulary; do not claim parity with Google Japanese Input. [dictionary README](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/src/data/dictionary_oss/README.txt), [top-level license notes](https://github.com/google/mozc/blob/0136c35782536cb43d710575c311cd4185982738/README.md) |
| azooKey app | MIT; app copyright notice differs from the converter. [LICENSE](https://github.com/azooKey/azooKey/blob/b50db4aec1069a8d2341f70415da3bdc61f1fce6/LICENSE) |
| AzooKeyKanaKanjiConverter code | MIT, copyright 2023 Miwa / Ensan. [LICENSE](https://github.com/azooKey/AzooKeyKanaKanjiConverter/blob/d59a28e4c7ca049aef04f29a91eae9677a7753f2/LICENSE) |
| azooKey dictionary storage | Apache-2.0, copyright 2024 Miwa / ensan. Preserve license and modified-file notice. Its README currently supplies no detailed data-source derivation; do not infer that the dictionary is MIT merely because the app is. [LICENSE](https://github.com/azooKey/azooKey_dictionary_storage/blob/4d418525b090cf49c219819d05a7e3cc2a4346eb/LICENSE) |
| Zenz v3.2 model repositories | Apache-2.0 according to model cards/API metadata; preserve that information if redistributing weights. This inspection did not establish the full training-data provenance. [small model card](https://huggingface.co/Miwa-Keita/zenz-v3.2-small-gguf/blob/c67e03e07d215c869f591b274c1631170d3e11fe/README.md), [xsmall model card](https://huggingface.co/Miwa-Keita/zenz-v3.2-xsmall-gguf/blob/4f5423f0fad41a73b1242eb96fe5c12ae4fdca83/README.md) |
| AJIMEE JWTD_v2/v1 data | CC-BY-SA 3.0 inherited from Japanese Wikipedia typo dataset v2. Preserve attribution to benchmark authors, upstream dataset and Wikipedia provenance; disclose input-script normalization or other changes. Keep a redistributed/modified dataset under the same data license. [README provenance/license](https://github.com/azooKey/AJIMEE-Bench/blob/401666cd56d1a570c2021798b64b6da4396bfd45/README.md) |
| AJIMEE utils.py and test_utils.py | CC0-1.0 per the repository README; this is distinct from its dataset license. [README](https://github.com/azooKey/AJIMEE-Bench/blob/401666cd56d1a570c2021798b64b6da4396bfd45/README.md) |

## Required evidence for a finished improvement

The chosen native absorption has now been implemented and measured. See [implementation results and validation](japanese-improvement.md) and [five-run metrics](japanese-metrics.json). The checklist below describes the evidence expected from that implementation; this source-comparison document itself is not an upstream runtime benchmark.

1. Freeze baseline results with the current IPADIC pack on curated romaji/session tests, Mozc development regressions, and untouched AJIMEE cases.
2. Generate a versioned Mozc-derived pack, validating 12-bit IDs, >255 entry groups, non-BMP surfaces and exact matrix orientation; retain a loader test for legacy packs if supported.
3. Compare identical lab commands after the switch. Publish top1/top5 and minimum CER by corpus, plus p50/p95/p99 warm conversion latency, initialization time, pack bytes and peak working set. Report regressions and context limitations.
4. Keep dictionary generation source-pinned and deterministic, and place complete third-party notices in source, language pack and platform distribution paths used by this repository.
5. Run the shared-core tests plus target builds. Source inspection confirms portability feasibility but does not verify the new engine on Windows, Android, macOS or Linux hardware.

Not checked here: execution of upstream Mozc/azooKey, neural inference quality/latency, physical-device memory/latency, and legal review of training datasets. Nothing in this report establishes that a native subset reproduces either complete upstream engine's accuracy.
