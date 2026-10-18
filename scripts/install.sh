#!/bin/sh
# Installs (or removes) tacit on Linux (IBus) and macOS (an input method). For the current
# user only: nothing here needs root.
#
#   curl -fsSL https://github.com/iamallyouneed/tacit/releases/latest/download/install.sh | sh
#   ... | sh -s -- --uninstall          remove it (settings are kept)
#   ... | sh -s -- --version v0.5.0     a specific release
#   ... | sh -s -- --languages ko,en,ja the languages to turn on, in Esc order
#
# Run from an unpacked release (tacit-linux.zip / tacit-macos.zip) it installs those
# files; otherwise it downloads the latest release and checks its SHA-256 first.
set -eu

REPO=iamallyouneed/tacit
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
  *) echo "tacit supports Linux and macOS here; on Windows use install.ps1." >&2; exit 1 ;;
esac

BIN="${HOME}/.local/bin"
if [ "$OS" = linux ]; then
  LIB="${XDG_DATA_HOME:-$HOME/.local/share}/tacit"
  COMPONENTS="${XDG_DATA_HOME:-$HOME/.local/share}/ibus/component"
else
  APP="$HOME/Library/Input Methods/tacit.app"
fi

if [ "$UNINSTALL" = 1 ]; then
  rm -f "$BIN/tacit" "$BIN/tacit-config"
  if [ "$OS" = linux ]; then
    rm -rf "$LIB"
    rm -f "$COMPONENTS/tacit.xml"
    if command -v ibus >/dev/null 2>&1; then ibus restart >/dev/null 2>&1 || true; fi
  else
    rm -rf "$APP"
  fi
  echo "tacit removed. Your settings are kept."
  exit 0
fi

HERE=$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo .)
if [ -f "$HERE/tacit-config" ]; then
  SRC=$HERE
else
  if [ "$VERSION" = latest ]; then
    BASE="https://github.com/$REPO/releases/latest/download"
  else
    BASE="https://github.com/$REPO/releases/download/$VERSION"
  fi
  SRC=$(mktemp -d)
  trap 'rm -rf "$SRC"' EXIT
  NAME="tacit-$OS.zip"
  echo "Downloading tacit ($VERSION)..."
  curl -fsSL "$BASE/$NAME" -o "$SRC/$NAME"
  curl -fsSL "$BASE/$NAME.sha256" -o "$SRC/$NAME.sha256"
  EXPECTED=$(cut -d' ' -f1 "$SRC/$NAME.sha256")
  if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL=$(sha256sum "$SRC/$NAME" | cut -d' ' -f1)
  else
    ACTUAL=$(shasum -a 256 "$SRC/$NAME" | cut -d' ' -f1)
  fi
  [ "$ACTUAL" = "$EXPECTED" ] || { echo "Checksum mismatch, not installing." >&2; exit 1; }
  # Nothing in the archive may land outside the folder it is unpacked into.
  if unzip -Z1 "$SRC/$NAME" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
    echo "The archive has paths outside its folder, not installing." >&2
    exit 1
  fi
  unzip -q -o "$SRC/$NAME" -d "$SRC"
fi

mkdir -p "$BIN"
cp "$SRC/tacit" "$SRC/tacit-config" "$BIN/"
chmod +x "$BIN/tacit" "$BIN/tacit-config"

if [ "$OS" = linux ]; then
  mkdir -p "$LIB" "$COMPONENTS"
  cp "$SRC/ibus-engine-tacit" "$LIB/"
  chmod +x "$LIB/ibus-engine-tacit"
  VER=$(cat "$SRC/VERSION" 2>/dev/null || true)
  sed -e "s|@EXEC@|$LIB/ibus-engine-tacit|" -e "s|@VERSION@|${VER:-0}|" \
    "$SRC/tacit.xml.in" > "$COMPONENTS/tacit.xml"
  if command -v ibus >/dev/null 2>&1; then ibus restart >/dev/null 2>&1 || true; fi
else
  mkdir -p "$HOME/Library/Input Methods"
  rm -rf "$APP"
  if tar -tf "$SRC/tacit.app.tar" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
    echo "The app archive has paths outside its folder, not installing." >&2
    exit 1
  fi
  tar -xf "$SRC/tacit.app.tar" -C "$HOME/Library/Input Methods"
  # Not signed or notarized: allow it once.
  xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true
fi

# Language packs: the ones turned on in settings (or the default English + Korean).
if [ "$VERSION" != latest ]; then
  export TACIT_RELEASE="https://github.com/$REPO/releases/download/$VERSION/"
fi
FRESH=0
if [ -n "$LANGS" ]; then
  "$BIN/tacit-config" setup --languages "$LANGS"
elif ! "$BIN/tacit-config" setup --if-configured 2>/dev/null; then
  # A first install with no languages given: the first-run page asks (opened at the end).
  FRESH=1
fi

echo
if [ "$OS" = linux ]; then
  echo "Installed. Add \"tacit\" under Settings > Keyboard > Input Sources (or run ibus-setup),"
  echo "then log out and in once if it is not listed."
else
  echo "Installed. Log out and in, then add \"tacit\" under System Settings > Keyboard >"
  echo "Input Sources > Edit. The first time, allow it under Privacy & Security."
fi
case ":$PATH:" in *":$BIN:"*) ;; *) echo "Add $BIN to your PATH to use tacit-config." ;; esac

# The first-run page runs on its own and ends a few minutes after its tab is closed. Its output
# holds the page's address with a session token, so it is not kept anywhere.
if [ "$FRESH" = 1 ]; then
  echo "Opening the first-run page in your browser (choose languages, try it out)..."
  nohup "$BIN/tacit-config" welcome >/dev/null 2>&1 &
  echo "If no page opened, run: tacit-config welcome"
fi
