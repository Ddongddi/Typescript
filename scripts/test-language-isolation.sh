#!/bin/sh
# Run the installer's selection block without reaching its download/install steps.
set -eu
cd "$(dirname "$0")/.."
selection=$(awk '/^LANGS=\$\(printf/{on=1} on && /^if \[ -f "\$HERE\/tacit-config" \]; then/{exit} on{print}' scripts/install.sh)
[ -n "$selection" ] || { echo "installer selection block not found" >&2; exit 1; }
scratch=$(mktemp -d)
trap 'rm -f "$scratch/tacit/edition"; rmdir "$scratch/tacit" "$scratch"' EXIT
mkdir -p "$scratch/tacit"

check() (
  LANGS=$1 EDITION=$2 previous=$3 expected=$4 OS=linux
  XDG_DATA_HOME=$scratch BIN=$scratch/no-installed-bin
  rm -f "$scratch/tacit/edition"
  [ -z "$previous" ] || printf '%s\n' "$previous" > "$scratch/tacit/edition"
  eval "$selection"
  [ "$EDITION" = "$expected" ] || { echo "expected $expected, got $EDITION" >&2; exit 1; }
)

check '' '' '' enko
check 'ko,en' '' enkoja enko
check 'ko, en, ja' '' '' enkoja
check '' '' enkoja enkoja
check '' enko enkoja enko
if check ja enko '' enko 2>/dev/null; then echo 'unsupported edition accepted' >&2; exit 1; fi
if check fr '' '' enko 2>/dev/null; then echo 'unsupported language accepted' >&2; exit 1; fi
if check '' '' invalid enko 2>/dev/null; then echo 'invalid marker accepted' >&2; exit 1; fi
echo 'Installer language isolation: 8 checks passed.'
