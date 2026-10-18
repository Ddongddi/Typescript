# tacit Architecture & Design Principles

tacit is strictly architected into an OS-agnostic core written in pure Rust (`crates/core`) and native platform adapter layers (`platforms/`) that integrate with each operating system's native input method framework.

[한국어](../ko/architecture.md) · [English Docs Index](README.md)

---

## 1. System Overview

```mermaid
flowchart TD
    subgraph Platforms [Platform Adapters]
        WIN[Windows TSF<br/>x64/x86 COM DLL]
        AND[Android IME<br/>JNI + Java Service]
        MAC[macOS<br/>InputMethodKit]
        LNX[Linux<br/>IBus Engine]
    end

    subgraph Core [crates/core (OS-Agnostic Rust Core)]
        subgraph SessionLayer [Session Layer: session/]
            SESSION[InputSession State Machine]
            WORD[Word Composition Buffer<br/>WordState]
            FLIP[Post-Commit Flip<br/>session/flip.rs]
            JP_SESSION[Japanese Conversion<br/>session/japanese.rs]
        end

        subgraph DecideLayer [Decide Layer: decide/]
            LEARN[User Correction Memory<br/>LearningEngine]
            MIXED[English+Particle Splitter<br/>SplittingEngine]
            ENGINE[Score-based Decision Engine<br/>HeuristicEngine]
            KANJI[Kanji Dictionary Engine<br/>KanjiDictionary / IPADIC]
        end

        subgraph LangLayer [Language Module Layer: lang/]
            REG[Language Registry]
            EN[English Model<br/>Trigrams + Vocabulary]
            KO[Korean Model<br/>2-Set Automata + Syllable Trigrams]
            JA[Japanese Model<br/>Romaji-to-Kana + Kana Trigrams]
            PACK[Language Pack Loader<br/>mmap-based Zero Copy]
        end

        subgraph StoreLayer [Store Layer: store/]
            POLICY[Settings & AppPolicy]
            HIST[Word Frequency History]
        end
    end

    Platforms -->|Key Events + TextClient Context| SESSION
    SESSION -->|Evaluate Word| LEARN
    LEARN --> MIXED
    MIXED --> ENGINE
    ENGINE --> REG
    REG --> EN & KO & JA
    PACK --> EN & KO & JA
    JP_SESSION --> KANJI
    SESSION -->|Text Edit Commands (Insert/Replace)| Platforms
```

---

## 2. Input Session State Machine (`session/`)

When a user begins typing, the platform adapter maps native OS keystrokes to abstract `session::Key` events and delegates them to `session::InputSession`.

### State Lifecycle
1. **Composing (In-Progress Typing)**:
   - Alphanumeric keystrokes accumulate in `WordState`.
   - On each keypress, the `decide` engine evaluates the token and generates a dotted underline (preview string) indicating the currently predicted language.
2. **Commit (Word Boundaries)**:
   - Delimiters such as `Space`, newline (`Enter`), or punctuation marks (`.`, `,`, `?`, `!`) trigger commitment.
   - The decided text is committed to the application, and the committed token is kept in the post-commit flip state (`session/flip.rs`) so a following `Esc` can undo or re-convert it.
3. **Esc Cycling**:
   - Pressing `Esc` during composition cycles through candidate readings:
     $$\text{English} \longrightarrow \text{English + Particle} \longrightarrow \text{Korean} \longrightarrow \text{Japanese (if enabled)}$$
   - Immediately after commitment (when the caret is right behind the committed word), pressing `Esc` replaces the word in the target application with the next candidate reading.
4. **Dynamic Learning**:
   - If the user corrects the same word twice with `Esc`, `LearningEngine` records this mapping in `lexicon.json`, prioritizing this decision for all future keystrokes.

---

## 3. Language Decision Pipeline (`decide/`)

Whenever a word buffer is evaluated, it traverses a prioritized multi-stage evaluation pipeline:

### Stage 1: User Correction Lexicon (`LearningEngine`)
- Checks `lexicon.json`. If the user has manually corrected this word twice or more, statistical scoring is skipped, and the explicit override is applied immediately.

### Stage 2: English and Korean Typed Together (`SplittingEngine`, `CodeSwitch`)
- Handles tokens such as `githubdp` → `github` + `에`, `reportwha` → `report` + `좀`, `kakaotalkaptlswj` → `kakaotalk` + `메신저`, `nwp` → `n` + `제`.
- Every way to read the token as a path of **words** is scored on one scale (log probability) and the best path wins (`decide/codeswitch.rs`, a Viterbi search over a word lattice). There is no rule per case, and a token of any length is searched whole, so a story typed with no space at all is cut the same way as `githubdp`.
  - Edges: a listed English word (log frequency), an English run that is none (letter trigrams, less a fixed price), a listed Korean word, a particle or ending, a single Korean syllable that is in no word (its probability, or its jamo's for one never seen). A Korean edge has to compose to whole syllables (`ㅏ마맷미` is out), and a syllable no text has shown shares only the small mass that the once-seen syllables point to (Good-Turing), so a rare one costs far more than a common one. One letter or 2-4 capitals (`n`, `A`, `QR`) are a label only at the very start. A one-letter English word (`a`, `I`) never sits between Korean syllables.
  - Joins: each switch between English and Korean costs, more when it lands on an edge with no word behind it; a label may only be followed by the noun it names (a little more when more of the token follows: `B급의`) or by a case particle, and a case particle counts as Korean only when it has a word to attach to (English, a Korean word, a run of syllables: not a lone syllable that is no word). A path that begins in English and turns Korean pays a fixed price when the word before was Korean (`lead_en`), but not against a reading that is English all through.
  - The token is `Mixed` when the best path that switches beats the best single-language one by `margin`, and by a larger one when its Korean has no word, case particle, label noun or frequent one-syllable word to stand on (the 맥 of aortosclerosis); otherwise it goes on to stage 3.
- The weights are fitted, not chosen by hand: `lab --tune` (coordinate descent) and a search over whole sentences. See `lab/src/tune.rs`.
- To stay fast on every keystroke, tokens that are a whole English word, clean Korean, or (while typing) the start of a listed English word are skipped, and the lattice of the token typed last is kept: a new key only searches the last 48 keys again (about 0.6 ms at 1600 keys; the whole token took 78 ms). What each start key needs is walked along, one letter at a time, instead of asked afresh: the English word list and letter model (`Walk`), and the Hangul composition with the narrowing of the Korean word lists (`KoWalk`). Scores are kept for the last positions only (where each path came from is kept for all), and a token that comes in at once is searched in pieces, so memory stays flat however long the token is. A stretch of letters that is no word is first ruled out by the pack's word filter (`korean_word_filter.bin`, a 64 KB Bloom filter of every Korean word: it never misses a word and only sometimes lets a non-word through), so the Korean word lists are not searched for it.
- `MixedWord` (English stem + particle) remains as the plain shape the learning and the Esc readings use.

### Stage 3: Statistical Heuristic Engine (`HeuristicEngine`)
Standard non-mixed tokens are evaluated using statistical language models:
- **Character Trigrams**: Computes log-likelihood probabilities across sliding 3-character windows for each candidate language.
- **Vocabulary Lookups**:
  - English frequency vocabulary.
  - Korean morphological dictionary (evaluating whether the 2-set transliteration exists in Hangul word lists).
- **Contextual Smoothing (`DecisionContext`)**:
  - Incorporates the previous word's language and confidence score to prevent abrupt, erratic flickers within a continuous phrase.
- **Threshold Matching (`conversionThreshold`)**:
  - The default threshold is `0.5`. Terminal profiles can raise this to `0.97` to prevent short shell commands from being parsed as Korean.

---

## 4. Language Automata & Conversion (`lang/`)

### 4.1 Korean (2-Set Hangul Automata, `lang/hangul.rs`)
- Implements the complete Korean 2-set syllable automata, combining initial consonants (Choseong), medial vowels (Jungseong), and final consonants (Jongseong) into precomposed Unicode codepoints (`U+AC00` through `U+D7A3`).
- Handles complex consonants (`ㄳ`, `ㄵ`, `ㄶ`, `ㄺ`) and compound vowels (`ㅘ`, `ㅙ`, `ㅚ`, `ㅝ`) with accurate decomposition on backspace.

### 4.2 Japanese (Romaji & Kanji Engine, `lang/romaji.rs`, `decide/kanji.rs`)
- **Romaji-to-Kana**: Maps QWERTY input to Hiragana/Katakana based on modified Hepburn and Kunrei-shiki romanization, supporting double consonants (`tt` → `っ`) and nasal sounds (`nn` → `ん`).
- **IPADIC Kanji Conversion**: When Japanese is active, pressing `Space` triggers a Viterbi search against the IPADIC dictionary (390k lexical entries and connection cost matrices) to generate ranked Kanji candidates.
- **Frequency Calibration**: Integrates `wordfreq` Zipf frequency data so that modern, commonly used orthographies appear first.

---

## 5. Memory-Mapped Language Packs (`lang/pack.rs`)

Because tacit runs as an in-process DLL loaded into every active application, memory efficiency and instant startup times are paramount.

- **Zero-Copy Binary Tables**: Trigram frequencies and lexical indices are precompiled into contiguous binary tables by `crates/packgen`.
- **Memory Mapping (`mmap`)**: Packs are mapped directly into virtual memory. Multiple processes concurrently running tacit share the same underlying physical memory pages.
- **Modular Packaging**: Large dictionaries (such as the ~10MB Japanese IPADIC pack) are decoupled from the base binaries and only loaded if the user enables the language.

---

## 6. Storage & State Persistence (`store/`)

Per-user runtime configuration is located under `%APPDATA%\tacit\` (Windows) or `~/.config/tacit/` (Linux):
- `settings.json`: Global operational parameters (language cycle order, autocomplete toggles, enter key rules).
- `apps.json`: Per-executable rules (conversion thresholds, pass-through flags).
- `lexicon.json`: User-corrected word associations.
- `user_history.json`: Vocabulary frequency records for Tab autocomplete.
