#!/bin/sh
# Installs (or removes) Typescript on Linux (IBus) and macOS (an input method). For the current
# user only: nothing here needs root.
#
#   curl -fsSL https://github.com/iamallyouneed/Typescript/releases/latest/download/install.sh | sh
#   ... | sh -s -- --uninstall          remove it (settings are kept)
#   ... | sh -s -- --version v0.5.0     a specific release
#   ... | sh -s -- --languages ko,en,ja the languages to turn on, in Esc order
#
# Run from an unpacked release (typescript-linux.zip / typescript-macos.zip) it installs those
# files; otherwise it downloads the latest release and checks its SHA-256 first.
set -eu

REPO=iamallyouneed/Typescript
VERSION=latest
LANGS=
UNINSTALL=0
while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) UNINSTALL=1 ;;
    --version) VERSION=$2; shift ;;
    --languages) LANGS=$2; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) echo "Typescript supports Linux and macOS here; on Windows use install.ps1." >&2; exit 1 ;;
esac

BIN="${HOME}/.local/bin"
if [ "$OS" = linux ]; then
  LIB="${XDG_DATA_HOME:-$HOME/.local/share}/typescript"
  COMPONENTS="${XDG_DATA_HOME:-$HOME/.local/share}/ibus/component"
else
  APP="$HOME/Library/Input Methods/Typescript.app"
fi

if [ "$UNINSTALL" = 1 ]; then
  rm -f "$BIN/typescript" "$BIN/typescript-config"
  if [ "$OS" = linux ]; then
    rm -rf "$LIB"
    rm -f "$COMPONENTS/typescript.xml"
    if command -v ibus >/dev/null 2>&1; then ibus restart >/dev/null 2>&1 || true; fi
  else
    rm -rf "$APP"
  fi
  echo "Typescript removed. Your settings are kept."
  exit 0
fi

HERE=$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo .)
if [ -f "$HERE/typescript-config" ]; then
  SRC=$HERE
else
  if [ "$VERSION" = latest ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
  else
    BASE="https://github.com/$REPO/releases/download/$VERSION"
  fi
  SRC=$(mktemp -d)
  trap 'rm -rf "$SRC"' EXIT
  NAME="typescript-$OS.zip"
  echo "Downloading Typescript ($VERSION)..."
  curl -fsSL "$BASE/$NAME" -o "$SRC/$NAME"
  curl -fsSL "$BASE/$NAME.sha256" -o "$SRC/$NAME.sha256"
  EXPECTED=$(cut -d' ' -f1 "$SRC/$NAME.sha256")
  if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL=$(sha256sum "$SRC/$NAME" | cut -d' ' -f1)
  else
    ACTUAL=$(shasum -a 256 "$SRC/$NAME" | cut -d' ' -f1)
  fi
  [ "$ACTUAL" = "$EXPECTED" ] || { echo "Checksum mismatch, not installing." >&2; exit 1; }
  unzip -q -o "$SRC/$NAME" -d "$SRC"
fi

mkdir -p "$BIN"
cp "$SRC/typescript" "$SRC/typescript-config" "$BIN/"
chmod +x "$BIN/typescript" "$BIN/typescript-config"

if [ "$OS" = linux ]; then
  mkdir -p "$LIB" "$COMPONENTS"
  cp "$SRC/ibus-engine-typescript" "$LIB/"
  chmod +x "$LIB/ibus-engine-typescript"
  VER=$(cat "$SRC/VERSION" 2>/dev/null || true)
  sed -e "s|@EXEC@|$LIB/ibus-engine-typescript|" -e "s|@VERSION@|${VER:-0}|" \
    "$SRC/typescript.xml.in" > "$COMPONENTS/typescript.xml"
  if command -v ibus >/dev/null 2>&1; then ibus restart >/dev/null 2>&1 || true; fi
else
  mkdir -p "$HOME/Library/Input Methods"
  rm -rf "$APP"
  tar -xf "$SRC/Typescript.app.tar" -C "$HOME/Library/Input Methods"
  # Not signed or notarized: allow it once.
  xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true
fi

# Language packs: the ones turned on in settings (or the default English + Korean).
if [ "$VERSION" != latest ]; then
  export TYPESCRIPT_RELEASE="https://github.com/$REPO/releases/download/$VERSION/"
fi
if [ -n "$LANGS" ]; then "$BIN/typescript-config" setup --languages "$LANGS"; else "$BIN/typescript-config" setup; fi

echo
if [ "$OS" = linux ]; then
  echo "Installed. Add \"Typescript\" under Settings > Keyboard > Input Sources (or run ibus-setup),"
  echo "then log out and in once if it is not listed."
else
  echo "Installed. Log out and in, then add \"Typescript\" under System Settings > Keyboard >"
  echo "Input Sources > Edit. The first time, allow it under Privacy & Security."
fi
case ":$PATH:" in *":$BIN:"*) ;; *) echo "Add $BIN to your PATH to use typescript-config." ;; esac
