# Lab changelog

The lab (`lab/`) has its own version, apart from the project's. It is raised when what the lab
measures changes, so that two numbers can be told apart from two different rulers:
a **minor** version when an existing metric stops being comparable with an older baseline (a set
is sampled or built another way, a case file changes what it expects); a **patch** for anything
else (a new metric, a flag, a fix). A baseline records the lab version it was made with.

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
