#!/bin/sh
# Builds tacit.app (a universal input method bundle) into $1 (default: platforms/macos/build).
# Needs a macOS with the Xcode command line tools and the Rust targets aarch64-apple-darwin and
# x86_64-apple-darwin (rustup target add ...). Run from the repository root.
set -eu
OUT=${1:-platforms/macos/build}
FEATURES=
case "${2:-enko}" in
  enko) ;;
  enkoja) FEATURES="--features japanese" ;;
  *) echo "unknown edition" >&2; exit 2 ;;
esac
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/core/Cargo.toml | head -1)
APP="$OUT/tacit.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
for arch in aarch64 x86_64; do
  swift_arch=$arch
  [ "$arch" = aarch64 ] && swift_arch=arm64
  cargo build --release -p tacit-ffi --target "$arch-apple-darwin" $FEATURES
  swiftc -O -target "$swift_arch-apple-macos11.0" \
    -import-objc-header platforms/ffi/tacit.h \
    platforms/macos/main.swift platforms/macos/Controller.swift platforms/macos/InputTextContext.swift \
    "target/$arch-apple-darwin/release/libtacit_ffi.a" \
    -framework Cocoa -framework InputMethodKit -framework Security -framework CoreFoundation \
    -lresolv -liconv -o "$OUT/tacit-$arch"
done
lipo -create "$OUT/tacit-aarch64" "$OUT/tacit-x86_64" -output "$APP/Contents/MacOS/tacit"
rm "$OUT/tacit-aarch64" "$OUT/tacit-x86_64"
sed "s/@VERSION@/$VERSION/g" platforms/macos/Info.plist > "$APP/Contents/Info.plist"
# Ad-hoc signature: required to run on Apple silicon, needs no developer account.
codesign --force --deep -s - "$APP"
echo "$APP"
