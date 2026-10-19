# Lab changelog

The lab (`lab/`) has its own version, apart from the project's. It is raised when what the lab
measures changes, so that two numbers can be told apart from two different rulers:
a **minor** version when an existing metric stops being comparable with an older baseline (a set
is sampled or built another way, a case file changes what it expects); a **patch** for anything
else (a new metric, a flag, a fix). A baseline records the lab version it was made with.

## [0.2.11]

### Added

- Allocator requests and requested bytes per commit/prefix decision, measured outside
  the reporting code. Requested bytes include temporary allocations, not resident memory.
  Compare timing with the same instrumented lab build.

## [0.2.10]

### Changed

- Prefix ranker training visits all key prefixes, matching key-weighted replay,
  instead of sampling one shortened prefix. Key-group separation is preserved.

## [0.2.9]

### Fixed

- Prefix ranker training now includes fully spelled tokens with `partial=true`,
  matching the session state before a delimiter. Older frozen models are unchanged.

## [0.2.8]

### Added

- Frozen mixed-script documentation corpus with source URLs, raw HTML hashes,
  document-separated train/dev and a separate provider reserved for final evaluation.
- `--mixed-training PATH` adds a training-only text source; `--switching-input PATH`
  replays a specified corpus without altering the existing lab evaluation sets.

## [0.2.7]

### Fixed

- Standalone jamo in learned samples and switching replay now use the regular
  lab's canonical physical keys (`ㅋㅋ` → `zz`, not `ZZ`). Earlier replay
  metrics on inputs containing standalone jamo are superseded.

### Added

- Switching replay separates decision errors from presentation mismatches and
  split/stale traces, with bounded diagnostics controlled by `--misses`.
- `--separate-heads` trains independent open/closed token ranker weights; the
  second model header distinguishes the 144-weight model from the 72-weight one.

## [0.2.6]

### Added

- Lab-only common candidate ranker (`--fit-ranker`, `LAB_ENKO_RANKER`), with
  optional prefix and counterfactual-context training. Candidate coverage is
  reported separately; gold candidates are never injected into inference.
- `LAB_ENKO_RANKER_PREVIEW_ONLY` compares the same learned model for open
  tokens with the original engine for closed tokens; it is also lab-only.
- `--switching` replays real InputSession key events without completion or
  look-ahead, reporting preview errors, language changes, committed sentence
  accuracy and handle latency. This is FakeTextField replay, not OS UI latency.
- Existing production selection and existing evaluation sets are unchanged.

## [0.2.5]

### Added

- `--session` measures history lookup and session preview plus completion with
  0, 1,000 and 20,000 synthetic history entries. It does not commit words or write
  learning data, and uses NoopClient rather than measuring OS rendering.
- Existing decision, corpus, scenario and Japanese evaluation metrics are unchanged.

## [0.2.4]

### Added

- `--audit-enko` separates single-language selection errors from mixed-lattice edge
  gaps, blocked transitions, skipped searches and selection errors. It uses actual
  left-to-right converter contexts and reports look-ahead sentence accuracy separately.
- `LAB_ENKO_FULL_SEARCH` disables existing search skip gates for an enko-only lab
  ablation. Product builds do not include this switch; defaults remain unchanged.
- Gold-constrained candidate diagnostics reuse the production edges and search,
  retain lower-ranked paths, and leave the live lattice cache untouched.

## [0.2.3]

### Added

- `--audit-tune` reports source-stem and identical-key overlap across tuning folds.
- Opt-in, lab-only key n-gram CRF training (`--fit-enko`, `--augment-spans`) and
  evaluation (`LAB_ENKO_MODEL`) across common word, corpus, scenario and learning sets.
  Neither the model nor training code is included in product builds.

### Fixed

- Tuning partitions all forms of a source stem together, including bare terms,
  instead of splitting by row. Tuner scores are not comparable with earlier splits;
  regular lab evaluation sets and metrics are unchanged.

## [0.2.2]

### Added

- `--japanese --readings PATH --best` measures the same first whole-reading candidate
  used by live conversion. Full-list output remains the default. Prefix, context and
  learned-choice tests compare this path with the complete candidate list.

## [0.2.1]

### Fixed

- `report.sh` averages the middle two values for an even number of runs.
  Both profile self-checks cover odd and even medians and now run in CI,
  alongside every lab test (including profile validation).

## [0.2.0]

### Changed

- Reports select `--profile enko` (default) or `--profile enkoja`. Common accuracy,
  scenarios and learning simulations use that profile's enabled languages. Japanese
  conversion metrics are included only in enkoja and remain separate from `acc.ALL`.
- Baselines are isolated in `baseline-enko.tsv` and `baseline-enkoja.tsv`; the former
  `baseline.tsv` is retained as the enko baseline. Saved baselines include the profile.
- English preview flicker counts any non-English reading, including Japanese.
- `report.sh` uses separate release histories, excludes the Japanese pack from enko,
  includes Japanese conversion metrics in enkoja, and refuses missing required metrics.

## [0.1.3]

### Added

- Japanese external evaluation reports `top_available` and each exact-match miss's
  expected rank, distinguishing a returned but lower-ranked answer from one absent
  from the returned list. Absence does not by itself establish a dictionary gap.

## [0.1.2]

### Added

- Japanese converter Top-1/Top-5, per-category accuracy, load and latency metrics;
  language detection and English preservation guards (`--japanese`).
- `evaluate_japanese.py`: pinned AJIMEE JWTD_v2/v1 (200 multi-reference cases,
  with/without context reported separately) and Mozc OSS exact/regex regressions.
  Exact match, minimum character error rate, and full-string regex semantics are separate.

## [0.1.1]

### Added

- `preview.flicker.en.freq/*`: the share of common English words whose preview turns Korean on
  some key (from the second on) before the word is done.
- `preview.mismatch.*`: the share of words whose preview on the last key is not what Space then
  commits.
- `ko.lone/*`: the one-syllable Korean words of the list (해, 내, 새 ...), in the three contexts.

## [0.1.0]

First numbered version.

### Added

- `--fold K/N`: the big sets keep every N-th line from K, so a choice is made on one fold and
  checked on another that shares no sentence with it.
- `big.ko.comment.*`: news comments (`python lab/fetch_data.py comments`), informal text from a
  second place that no word list or tuning ever sees.
- `big.cs_short.bare`: short English words that also compose to whole Hangul syllables
  (`cases/short_terms.txt`: so, go, do ...) inside Korean sentences; they have to stay English.
  The guard for any change that makes a short Korean word easier to reach.
- The baseline file starts with `#lab <version>`; the report says when the baseline is from
  another lab version.
