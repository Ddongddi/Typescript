# Typescript Language Pack System & Developer Guide

To keep the core engine lightweight and ensure instantaneous application startup, Typescript separates language-specific datasets (trigram frequency tables, vocabulary dictionaries, and Kanji conversion indices) into standalone **Language Pack** files (`.pack`).

[한국어](../ko/language-packs.md) · [English Docs Index](README.md)

---

## 1. Language Pack Architecture

```
%LOCALAPPDATA%\Typescript\packs\ (Windows)
~/.local/share/Typescript/packs/ (Linux)
~/Library/Application Support/Typescript/packs/ (macOS)
├── en.pack          (~0.5 MB) - English trigrams and frequency vocabulary
├── ko.pack          (~0.4 MB) - Korean syllable trigrams and dictionary
└── ja.pack          (~6.0 MB) - Romaji rules + Kana trigrams + IPADIC dictionary
```

### Architectural Benefits
1. **Minimal Memory Footprint**: The DLL loaded into each client process remains under 1MB.
2. **Selective Downloads**: Users only download packs for the languages they actually use (e.g., the ~16MB Japanese IPADIC dictionary is optional).
3. **Memory Mapping (`mmap`)**: All running processes share the exact same physical memory pages mapped from the `.pack` files, eliminating system RAM bloat.
4. **Decoupled Extensibility**: Adding new language support requires minimal modifications to the core engine.

---

## 2. Pack Format Specification

Each `.pack` archive contains validated metadata and optimized binary tables.

### 2.1 `manifest.json`
Specifies language identity, version, table definitions, and SHA-256 integrity hashes:

```json
{
  "id": "ja",
  "name": "Japanese",
  "version": "1.0.0",
  "tables": {
    "kana_trigrams": "sha256:4a8b...12",
    "ipadic_words": "sha256:9c1e...34",
    "ipadic_matrix": "sha256:d83f...78"
  }
}
```

### 2.2 Binary Table Standards
- **Little-Endian byte order**.
- Fixed-size headers and precomputed index offsets allow $O(1)$ random lookups or $O(\log N)$ binary searches without runtime deserialization overhead.

---

## 3. Pack Generator Tooling (`typescript-packgen`)

The `typescript-packgen` crate converts raw text datasets (corpus frequencies, word lists) into finalized binary pack files.

### 3.1 Building Packs
```powershell
# Compile all official language packs (en, ko, ja)
cargo run -p typescript-packgen --bin build-packs -- crates/core/data target/packs
```

The output directory (`target/packs/`) contains the compiled `.pack` archives and the root index `packs.json` with computed checksums.

---

## 4. Adding a New Language (Step-by-Step)

To introduce a new language module (e.g., French `fr` or German `de`):

### Step 1: Prepare Raw Corpora
1. Extract a normalized character trigram frequency distribution (`trigrams.txt`).
2. Compile a frequency-sorted list of top 50,000 words (`vocabulary.txt`).
3. Place raw files under `crates/core/data/<lang>/`.
4. **Important**: Verify [data licensing requirements](#5-dataset-licensing-compliance).

### Step 2: Implement the Language Module (`crates/core/src/lang/<id>/`)
Implement `LanguageModule` (`crates/core/src/lang/module.rs`). It answers two questions about a run of keys: what do they type in this language, and does that reading look clean?

```rust
pub struct French;

impl LanguageModule for French {
    fn language(&self) -> Language { Language::French }
    /// The text these keys type; None when they can't.
    fn compose(&self, keys: &str) -> Option<String> { /* … */ }
    /// No stray pieces: Esc skips unclean readings while clean ones exist.
    fn is_clean(&self, keys: &str) -> bool { /* … */ }
}
```

### Step 3: Wire It In
A new language is **not** a one-line change. These are the places that name languages, in the order you will meet them:

1. `decide/engine.rs`: the `Language` variant and `name` / `id` / `from_id` / `parse` / `sentence_language`.
2. `lang/registry.rs`: add the module to `MODULES`.
3. `decide/engine.rs` (`HeuristicEngine::decide`): the language's score and its bias fields. This is the part that is specific to each language; there is no generic scorer yet.
4. `lib.rs` (`make_engine`) and `lang/set.rs`: build the engine for the enabled set and the default order.
5. `lang/pack.rs` (`embedded()`): the pack's files and which packs it `requires`, plus `usable()` if a damaged file could panic a reader.
6. `session/`: only if the language needs more than a reading (Japanese has a conversion window; see `session/japanese.rs`).
7. `store/history.rs` (`suggest`) for Tab completion, and the dashboard's language list, if they apply.

### Step 4: Add `packgen` Compiler Rule
Add a builder recipe in `crates/packgen/src/main.rs` to compile the new raw dataset into the binary pack format.

### Step 5: Test Accuracy and Golden Baselines
Ensure the new language does not degrade recognition accuracy for existing languages:

```powershell
cargo test -p typescript-core --test accuracy
cargo test -p typescript-core --test golden
```

---

## 5. Dataset Licensing Compliance

Typescript maintains strict open-source licensing standards:

1. **Permissive and Share-Alike Datasets Only**: Acceptable licenses include CC-BY, CC BY-SA, MIT, BSD, or Apache-2.0.
2. **No Non-Commercial (NC) Data**: Datasets with CC-BY-NC or commercial restrictions are strictly prohibited.
3. **Mandatory Attribution**: All dataset origins, authors, and license texts must be fully documented in the root [THIRD_PARTY_NOTICES.txt](../../THIRD_PARTY_NOTICES.txt).
