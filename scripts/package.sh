#!/bin/sh
# Packs the release build for Linux or macOS into dist/ (the counterpart of package.ps1):
#   dist/typescript-<os>.zip         typescript, typescript-config, the input method, install.sh
#   dist/typescript-<os>.zip.sha256  checked by install.sh and by typescript-config update
#   dist/install.sh                  the one-line installer itself (released next to the zip)
# Usage: scripts/package.sh linux|macos   (after the release builds, see .github/workflows)
set -eu
OS=${1:?usage: package.sh linux|macos}
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/core/Cargo.toml | head -1)
NAME="typescript-$OS"
STAGE="dist/stage-$NAME"
mkdir -p dist
rm -rf "$STAGE" "dist/$NAME.zip"
mkdir -p "$STAGE"

cp target/release/typescript target/release/typescript-config "$STAGE/"
cp THIRD_PARTY_NOTICES.txt scripts/install.sh "$STAGE/"
echo "$VERSION" > "$STAGE/VERSION"
if [ "$OS" = linux ]; then
  make -C platforms/linux
  cp platforms/linux/ibus-engine-typescript platforms/linux/typescript.xml.in "$STAGE/"
else
  sh platforms/macos/build.sh platforms/macos/build
  # One flat file: typescript-config's updater unpacks only folder-less packages.
  tar -C platforms/macos/build -cf "$STAGE/Typescript.app.tar" Typescript.app
fi
chmod +x "$STAGE"/typescript "$STAGE"/typescript-config "$STAGE"/install.sh

(cd "$STAGE" && zip -qr "../$NAME.zip" .)
if command -v sha256sum >/dev/null 2>&1; then SUM=$(sha256sum "dist/$NAME.zip"); else SUM=$(shasum -a 256 "dist/$NAME.zip"); fi
echo "$(echo "$SUM" | cut -d' ' -f1)  $NAME.zip" > "dist/$NAME.zip.sha256"
cp scripts/install.sh dist/install.sh
rm -rf "$STAGE"
ls -l dist
