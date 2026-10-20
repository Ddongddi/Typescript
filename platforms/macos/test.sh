#!/bin/sh
set -eu

ROOT=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/tacit-macos-tests.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

swiftc \
    "$ROOT/platforms/macos/InputTextContext.swift" \
    "$ROOT/platforms/macos/tests/InputTextContextTests.swift" \
    -o "$WORK/input-text-context-tests"
"$WORK/input-text-context-tests"

mkdir -p "$WORK/home/Library/Application Support/tacit"
printf '%s\n' '{"languages":["ko","en"]}' > "$WORK/home/Library/Application Support/tacit/settings.json"

cargo build --manifest-path "$ROOT/Cargo.toml" -p tacit-ffi
TARGET_DIR=$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --no-deps | \
    python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')

clang -fobjc-arc -fblocks \
    -c "$ROOT/platforms/macos/tests/RecordingClient.m" \
    -o "$WORK/RecordingClient.o"
swiftc \
    -import-objc-header "$ROOT/platforms/macos/tests/RecordingClient.h" \
    "$ROOT/platforms/macos/InputTextContext.swift" \
    "$ROOT/platforms/macos/Controller.swift" \
    "$ROOT/platforms/macos/tests/ControllerTests.swift" \
    "$WORK/RecordingClient.o" \
    "$TARGET_DIR/debug/libtacit_ffi.a" \
    -framework Cocoa \
    -framework InputMethodKit \
    -framework Carbon \
    -framework Security \
    -framework CoreFoundation \
    -lresolv \
    -liconv \
    -o "$WORK/controller-tests"
HOME="$WORK/home" "$WORK/controller-tests"
