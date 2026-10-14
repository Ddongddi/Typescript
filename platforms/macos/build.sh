#!/bin/sh
# Builds Typescript.app (a universal input method bundle) into $1 (default: platforms/macos/build).
# Needs a macOS with the Xcode command line tools and the Rust targets aarch64-apple-darwin and
# x86_64-apple-darwin (rustup target add ...). Run from the repository root.
set -eu
OUT=${1:-platforms/macos/build}
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/core/Cargo.toml | head -1)
APP="$OUT/Typescript.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
for arch in aarch64 x86_64; do
  swift_arch=$arch
  [ "$arch" = aarch64 ] && swift_arch=arm64
  cargo build --release -p typescript-ffi --target "$arch-apple-darwin"
  swiftc -O -target "$swift_arch-apple-macos11.0" \
    -import-objc-header platforms/ffi/typescript.h \
    platforms/macos/main.swift platforms/macos/Controller.swift \
    "target/$arch-apple-darwin/release/libtypescript_ffi.a" \
    -framework Cocoa -framework InputMethodKit -framework Security -framework CoreFoundation \
    -lresolv -liconv -o "$OUT/Typescript-$arch"
done
lipo -create "$OUT/Typescript-aarch64" "$OUT/Typescript-x86_64" -output "$APP/Contents/MacOS/Typescript"
rm "$OUT/Typescript-aarch64" "$OUT/Typescript-x86_64"
sed "s/@VERSION@/$VERSION/g" platforms/macos/Info.plist > "$APP/Contents/Info.plist"
# Ad-hoc signature: required to run on Apple silicon, needs no developer account.
codesign --force --deep -s - "$APP"
echo "$APP"
