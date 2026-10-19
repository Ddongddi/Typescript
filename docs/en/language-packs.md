# Tacit Language Pack System & Developer Guide

To keep the core engine lightweight and ensure instantaneous application startup, Tacit separates language-specific datasets (trigram frequency tables, vocabulary dictionaries, and Kanji conversion indices) into standalone **Language Packs**. Each pack is a single **folder** (`manifest.json` plus its table files); releases ship one zip per language.

[한국어](../ko/language-packs.md) · [English Docs Index](README.md)

---

## 1. Language Pack Architecture

### Supported, installed and active languages

Desktop builds support English/Korean by default. The `japanese` Cargo feature includes Japanese scoring, composition, segmentation, conversion, session handling and dedicated settings assets. `embedded-ja` enables that implementation and embeds its data; `japanese` alone reads an installed pack. Windows distribution binaries do not embed dictionaries.

The module registry defines **supported** languages (`tacit-config supported`). **Installed** languages have validated packs on disk; unsupported packs are not registered, mapped or updated. **Active** languages are the supported languages selected in settings. Input methods map only their packs and declared data dependencies. Old settings or leftover Japanese packs cannot enable Japanese in an English/Korean build.

```powershell
# English/Korean; do not build together with the CLI (Cargo unifies its embedded data features).
cargo build --release -p tacit-tip -p tacit-config
# Japanese-enabled desktop binaries; forward the feature to both packages.
cargo build --release -p tacit-tip -p tacit-config --features tacit-tip/japanese,tacit-config/japanese
```

Packages are `tacit-<os>.zip` and `tacit-<os>-ja.zip`. Their `edition` marker and the updater's compiled configuration preserve the edition across updates. Windows `-Languages en,ko,ja` / `-Japanese` and Unix `--languages en,ko,ja` select the Japanese-enabled edition. Adding an unsupported language through the dashboard is rejected before downloading; switch editions through the installer.

For another optional language, gate its core implementation and registry entry with one feature and forward it through adapters and config. Add its UI feature mapping in `crates/config/build.rs`; wrap dedicated assets in `/*language:<id>*/…/*language:/<id>*/` (HTML uses equivalent HTML comments). These blocks are removed before compilation. Add only the combination rules that are actually needed through `ComboRule`.

Run `cargo test --workspace` and `cargo test --workspace --all-features`. Each existing configuration must retain its golden results and exclude the new language's implementation, data and dedicated UI. Ship only configurations that are actually offered; do not generate every possible language combination. Register a new configuration in installer validation/package selection and update identity together. Stable language identifiers, old-settings compatibility, shared change logs and license notices remain common metadata. Already mapped data stays alive until process exit so existing model references remain valid.

Targets 0.10.x. Update program and packs together. The Japanese dictionary uses Mozc OSS word/connection costs in the native Rust converter; no Mozc executable is needed.

```
%LOCALAPPDATA%\tacit\packs\ (Windows)
~/.local/share/tacit/packs/ (Linux)
~/Library/Application Support/tacit/packs/ (macOS)
├── en-0.10.3\              (~0.45 MB zip) - English trigrams and vocabulary
│   ├── manifest.json      (id · version · core range · table file hashes)
│   ├── english_levels.txt
│   └── trigrams.bin
├── ko-0.10.3\              (~0.45 MB zip) - Korean word/syllable tables and word filter
│   ├── manifest.json
│   ├── korean_words_sorted.txt · korean_vocab_sorted.txt
│   ├── korean_syllables.txt · korean_syllable_counts.txt
│   └── korean_word_filter.bin
└── ja-0.10.3\              (~38.1 MB dictionary) - Kana trigrams + Mozc OSS IME data
    ├── manifest.json
    ├── kana_trigrams.bin
    ├── ipadic_compact.bin  (TSIPDIC4; legacy filename)
    ├── mozc-LICENSE.txt · mozc-DICTIONARY-LICENSE.txt
    └── dictionary-sources.json
```

The installer downloads a release's `tacit-pack-<id>-<version>.zip`, unpacks it, and only registers it as a pack folder when every table file matches `packs.json` (SHA-256 and size) and the manifest hashes.

### Architectural Benefits
1. **Minimal Memory Footprint**: The DLL loaded into each client process stays under 4MB.
2. **Selective Downloads**: Users only download packs for the languages they actually use (the Japanese dictionary is about 38.1 MB unpacked).
3. **Memory Mapping (`mmap`)**: All running processes share the exact same physical memory pages mapped from the table files, eliminating system RAM bloat.
4. **Decoupled Extensibility**: Adding new language support requires minimal modifications to the core engine.

---

## 2. Pack Format Specification

Each pack folder contains validated metadata (`manifest.json`) and binary table files. The release zip flattens a pack folder's files (manifest.json first) with fixed timestamps, so the same input always zips to the same bytes.

### 2.1 `manifest.json`
Specifies language identity, version, table definitions, and SHA-256 integrity hashes:

```json
{
  "id": "ja",
  "name": "日本語",
  "version": "0.10.3",
  "core": ">=0.10.0, <0.11.0",
  "requires": ["en"],
  "files": {
    "ipadic_compact.bin": "9c1e…",
    "kana_trigrams.bin": "4a8b…"
  }
}
```

### 2.2 Binary Table Standards
- **Little-Endian byte order**.
- Fixed-size headers and precomputed index offsets allow $O(1)$ random lookups or $O(\log N)$ binary searches without runtime deserialization overhead.

---

## 3. Pack Generator Tooling (`tacit-packgen`)

The `tacit-packgen` crate converts raw text datasets (corpus frequencies, word lists) into finalized language packs (binary table files plus a manifest).

### 3.1 Building Packs
```powershell
# Compile all official language packs (en, ko, ja)
cargo run -p tacit-packgen --bin build-packs -- crates/core/data target/packs
```

The default output is one folder per language version, like `target/packs/en-0.10.0/` (what a checkout installs directly). With `--release` it writes the release zips `tacit-pack-<id>-<version>.zip` plus a `packs.json` listing their sizes and SHA-256 hashes (what `tacit-config` reads).

---

## 4. Adding a New Language (Step-by-Step)

To introduce a new language module (e.g., French `fr` or German `de`):

### Step 1: Prepare Raw Corpora
1. Extract a normalized character trigram frequency distribution (`trigrams.txt`).
2. Compile a frequency-sorted list of top 50,000 words (`vocabulary.txt`).
3. Place raw files flat under `crates/core/data` as separate, language-prefixed files (see `english_words.txt`, `korean_vocabulary.txt`); no subfolders.
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
cargo test -p tacit-core --test accuracy
cargo test -p tacit-core --test golden
```

---

## 5. Dataset Licensing Compliance

Tacit maintains strict open-source licensing standards:

1. **Permissive and Share-Alike Datasets Only**: Acceptable licenses include CC-BY, CC BY-SA, MIT, BSD, or Apache-2.0.
2. **No Non-Commercial (NC) Data**: Datasets with CC-BY-NC or commercial restrictions are strictly prohibited.
3. **Mandatory Attribution**: All dataset origins, authors, and license texts must be fully documented in the root [THIRD_PARTY_NOTICES.txt](../../THIRD_PARTY_NOTICES.txt).
