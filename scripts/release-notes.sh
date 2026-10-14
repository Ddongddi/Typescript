#!/bin/sh
# Prints the CHANGELOG.md entry of a release: the lines under "## [X.Y.Z]" up to the next
# "## [" heading. This is what a GitHub release shows (and the dashboard's update tab).
#   sh scripts/release-notes.sh v0.5.0
# Exits 1, printing nothing, when the version has no (or an empty) entry.
set -eu
tag=${1:?usage: release-notes.sh vX.Y.Z}
version=${tag#v}
cd "$(dirname "$0")/.."
notes=$(awk -v v="$version" '
  index($0, "## [" v "]") == 1 { on = 1; next }
  on && /^## / { exit }
  on && /^\[[^]]+\]: / { exit }
  on { print }
' CHANGELOG.md | sed -e :a -e '/^[[:space:]]*$/{$d;N;ba' -e '}' | sed '1{/^[[:space:]]*$/d;}')
[ -n "$notes" ] || { echo "CHANGELOG.md has no entry for $version" >&2; exit 1; }
printf '%s\n' "$notes"
