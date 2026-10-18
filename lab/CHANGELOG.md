# Lab changelog

The lab (`lab/`) has its own version, apart from the project's. It is raised when what the lab
measures changes, so that two numbers can be told apart from two different rulers:
a **minor** version when an existing metric stops being comparable with an older baseline (a set
is sampled or built another way, a case file changes what it expects); a **patch** for anything
else (a new metric, a flag, a fix). A baseline records the lab version it was made with.

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
