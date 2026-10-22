# Tacit Installation & Build Guide

This document covers installation, building from source, and uninstallation across all supported operating systems: Windows, macOS, Linux, and Android.

[한국어](../ko/installation.md) · [English Docs Index](README.md)

---

## 1. Platform Support Summary

| Platform | Binary Type | Input Modality | Supported Features |
| --- | --- | --- | --- |
| **Windows** | TSF Text Service (x64 / x86 COM DLL) | Physical keyboard | EN/KO/JA detection, Tab autocomplete, word learning, preview UI |
| **Android** | Standalone APK (`InputMethodService`) | Virtual & external keyboards | EN/KO detection, 2-set engraved keyboard, hardware keyboard |
| **macOS** | InputMethodKit app (`tacit.app`) | Physical keyboard | EN/KO/JA detection, Option+Space pause, Esc cycle |
| **Linux** | IBus Engine (`ibus-engine-tacit`) | Physical keyboard | EN/KO/JA detection, IBus desktop integration |

---

## 2. Windows Installation

### 2.1 One-Liner Quick Install (Recommended)

Open PowerShell and run the following command (requires a single UAC administrator prompt):

```powershell
irm https://github.com/iamallyouneed/tacit/releases/latest/download/install.ps1 | iex
```

- On a first install, a **welcome page** opens in your browser to pick the active languages (English, Korean, Japanese; the list order is the `Esc` cycling order) and walk through a short practice. Update installs don't show it.
- The downloaded zip is verified against the official GitHub Release's SHA-256 checksum before anything is registered.

#### Script Arguments & Headless Configuration
You can automate or customize the installation without the interactive menu:

```powershell
$i = [scriptblock]::Create((irm https://github.com/iamallyouneed/tacit/releases/latest/download/install.ps1))

# Explicitly select languages and Esc order (Korean, English, Japanese)
& $i -Languages ko,en,ja

# Enable the Japanese language pack
& $i -Japanese

# Install a specific version
& $i -Version v0.10.4

# Uninstall (retains user settings and packs)
& $i -Uninstall
```

### 2.2 Selecting the Input Method
Installation sets **Tacit (Autocomplete)** as the Windows default input method. Updates preserve an existing Tacit default profile. Change the default in Windows **Settings → Time & language → Typing → Advanced keyboard settings → Override for default input method**.

For an existing installation that returns to the system input method after switching apps or signing in, select Tacit as the default there. If **Let me use a different input method for each app window** is enabled, Windows remembers a separate input method per window; turn it off to share one input method across windows.

1. Press **Win + Space** and select **Tacit** or **Tacit (Autocomplete)**.
2. In the Windows language list, Tacit appears under **English (United States)**. *(This intentional design avoids interim composition clipping bugs found in legacy IMM32 apps such as KakaoTalk).*
3. Applications that were already running when you installed Tacit must be restarted to detect the new text service.

### 2.3 Building from Source on Windows
The Windows TSF service requires building both 64-bit and 32-bit DLLs to support native and legacy 32-bit processes.

**Prerequisites:**
- Rust stable (`rustup`)
- 32-bit MSVC target: `rustup target add i686-pc-windows-msvc`
- Visual Studio C++ Build Tools (MSVC)

```powershell
git clone https://github.com/iamallyouneed/tacit.git
cd tacit

# Compiles x64 + x86 DLLs and registers TSF COM service
.\scripts\install.ps1

# To build and install with the Japanese language pack:
.\scripts\install.ps1 -Japanese
```

---

## 3. macOS Installation

### 3.1 Quick Install

Run in Terminal (no administrator privileges required):

```sh
curl -fsSL https://github.com/iamallyouneed/tacit/releases/latest/download/install.sh | sh
```

- Installed to `~/Library/Input Methods/tacit.app`.
- After installation, **log out and log back in**.
- Open **System Settings** → **Keyboard** → **Text Input (Input Sources)** → **Edit**, click `+`, and add **Tacit**.
- *Note:* Because releases are unsigned and un-notarized, macOS Gatekeeper may prompt you under **Privacy & Security** to "Open Anyway" on first run.

### 3.2 Building from Source on macOS
```sh
# Add Universal binary targets (Apple Silicon + Intel)
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Run build and packaging script
sh platforms/macos/build.sh
```

---

## 4. Linux Installation

### 4.1 Quick Install (IBus)

```sh
curl -fsSL https://github.com/iamallyouneed/tacit/releases/latest/download/install.sh | sh
```

- The engine installs to `~/.local/share/tacit/`, and the component XML to `~/.local/share/ibus/component/`.
- Log out and log back in, or run `ibus restart`.
- Add **Tacit** via your desktop's keyboard settings or `ibus-setup`.

### 4.2 Building from Source on Linux
**Prerequisites (Ubuntu/Debian):**
```sh
sudo apt update
sudo apt install libibus-1.0-dev pkg-config build-essential
```

**Build and Install:**
```sh
cargo build --release -p tacit-ffi
make -C platforms/linux
make -C platforms/linux install
ibus restart
```

---

## 5. Android Installation

### 5.1 Standalone APK Install
1. Download **`tacit.apk`** from [GitHub Releases](https://github.com/iamallyouneed/tacit/releases/latest).
2. Open the APK on your device and permit installation from unknown sources.
3. Navigate to Android **Settings** → **System** → **Languages & input** → **On-screen keyboard**, and toggle **Tacit** ON.
4. Tap any text field, select the input switcher icon, and select Tacit.

### 5.2 Building from Source on Android
**Prerequisites:**
- Android SDK (API 34 platform, build-tools 34.0.0)
- Android NDK (r25+)
- JDK 17
- Rust targets: `rustup target add aarch64-linux-android x86_64-linux-android`
- `cargo install cargo-ndk`

**Build Command:**
```powershell
# Build combined arm64 + x86_64 APK
.\platforms\android\build.ps1

# Phone architecture only (arm64)
.\platforms\android\build.ps1 -Abi arm64-v8a

# Install directly to a connected device via ADB
adb install -r platforms\android\build\tacit.apk
```

---

## 6. Uninstallation

### Windows
```powershell
$i = [scriptblock]::Create((irm https://github.com/iamallyouneed/tacit/releases/latest/download/install.ps1))
& $i -Uninstall

# To wipe user settings and learned vocabulary:
Remove-Item -Recurse -Force "$env:APPDATA\tacit"
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\tacit"
```

### macOS / Linux
```sh
curl -fsSL https://github.com/iamallyouneed/tacit/releases/latest/download/install.sh | sh -s -- --uninstall
```
