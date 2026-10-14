# Typescript Configuration & Management Guide

Typescript provides a local browser-based dashboard, a unified CLI management tool (`typescript-config`), and human-readable JSON configuration files.

[한국어](../ko/configuration.md) · [English Docs Index](README.md)

---

## 1. Storage Locations

Configuration and user-learned state are stored under the following platform paths:

| OS | Path |
| --- | --- |
| **Windows** | `%APPDATA%\Typescript\` (`C:\Users\<User>\AppData\Roaming\Typescript\`) |
| **macOS** | `~/Library/Application Support/Typescript/` |
| **Linux** | `~/.config/Typescript/` (or `$XDG_CONFIG_HOME/Typescript/`) |

---

## 2. Web Dashboard (`typescript-config`)

Launch the web management interface by clicking **Typescript Settings** from the Start Menu or executing `typescript-config` in your terminal:

```powershell
typescript-config
```

### Security & Lifecycle Design
- **Localhost Loopback Only**: The server listens exclusively on `127.0.0.1`, rejecting any external network connections.
- **Single-Use Access Token**: Every time the server starts, a random cryptographic token (`?token=...`) is injected into the URL query. Requests lacking this token are denied.
- **Automatic Self-Termination**: Once you close the browser tab, the background process detects the disconnection and cleanly exits after a brief idle grace period.

### Key Capabilities
1. **Language Management**: Download, enable, or remove language packs; reorder the `Esc` key rotation sequence via drag-and-drop.
2. **Typing Behaviors**: Configure Enter key confirmation, Tab autocomplete, pause/resume hotkeys, and Japanese live conversion.
3. **Per-App Customization**: Review currently running processes and tweak conversion thresholds or `Esc` routing directly from an interactive table.
4. **Interactive Playground**: Test typing directly in the browser with live visual feedback showing trigram scores and token splitting.
5. **Updates**: Check for new application releases and updated language packs with one-click installation.

---

## 3. CLI Command Reference

Manage all settings headless via the `typescript-config` command-line utility:

```powershell
# List installed packs, downloadable packs, and active cycle order
typescript-config list

# Download and enable a language pack (e.g., Japanese)
typescript-config add ja

# Disable a language and remove its pack files
typescript-config remove ja

# Reorder the Esc rotation sequence among active languages
typescript-config order ko,en,ja

# Check GitHub Releases for new updates
typescript-config check

# Upgrade installed packs and run the updated installer
typescript-config update

# Non-interactive automated initial setup
typescript-config setup --languages ko,en
```

---

## 4. Configuration Files Reference

Configuration files are formatted in standard JSON and can be edited in any text editor. Typescript automatically reloads configuration whenever input focus switches between windows.

### 4.1 `settings.json` (Global Settings)

```json
{
  "languages": ["en", "ko"],
  "returnConfirmsOnly": true,
  "flipKeys": ["Esc", "Hangul"],
  "pauseKeys": ["Alt+Hangul"],
  "autocomplete": true,
  "checkUpdates": true,
  "liveConversion": false,
  "fullWidthDigits": false,
  "fullWidthSpace": false
}
```

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `languages` | `string[]` | `["en", "ko"]` | Active languages and `Esc` rotation order. Supports `"en"`, `"ko"`, `"ja"`. |
| `returnConfirmsOnly` | `bool` | `true` | When `true`, pressing `Enter` on a composing word only commits the underlined text without inserting a newline in the app. |
| `flipKeys` | `string[]` | `["Esc", "Hangul"]` | Keys that switch the word being typed (or just typed) to its next language. Combinations such as `"Ctrl+Space"` work; a lone modifier such as `"Ctrl"` or `"RCtrl"` acts when released without another key. Up to 8; `[]` turns it off. Also editable under "Key assignment" in the dashboard settings. |
| `pauseKeys` | `string[]` | `["Alt+Hangul"]` | Keys that pause/resume Typescript. Same format as `flipKeys`; a key can't be in both lists. The old `pauseKey` is read and moved (`"off"` becomes `[]`). |
| `autocomplete` | `bool` | `true` | Enables `Tab` autocomplete and word frequency learning in `Typescript (Autocomplete)`. |
| `checkUpdates` | `bool` | `true` | Automatically checks GitHub Releases once a day for new versions. |
| `liveConversion` | `bool` | `false` | (Japanese) Displays ranked Kanji candidate conversions in real-time during typing. |
| `fullWidthDigits` | `bool` | `false` | (Japanese) Converts digits following Japanese phrases to full-width (`０３-１２３４`). |
| `fullWidthSpace` | `bool` | `false` | (Japanese) Inserts full-width space characters (`　`) following Japanese phrases. |

Key names: `Esc`, `Hangul`, `Hanja`, `CapsLock`, `Pause`, `ScrollLock`, `Space`, `F1`–`F24`, letters (`A`–`Z`) and digits (`0`–`9`); modifiers are `Ctrl`, `Alt`, `Shift` (right side only: `RCtrl`, `RAlt`, `RShift`). Not allowed: combinations with the Windows key, letters/digits/`Space` without Ctrl or Alt, common shortcuts such as `Ctrl+C` and `Ctrl+V`, and `Alt+F4`.

---

### 4.2 `apps.json` (Per-Application Rules)

Define custom behavior keyed by the application's executable filename (lowercase). `"*"` represents defaults applied to all applications.

```json
{
  "*": {
    "escapeToApp": false,
    "conversionThreshold": 0.5,
    "enabled": true
  },
  "windowsterminal.exe": {
    "conversionThreshold": 0.97
  },
  "code.exe": {
    "escapeToApp": true
  },
  "game.exe": {
    "enabled": false
  },
  "special.exe": {
    "useSystemIme": true
  }
}
```

| Property | Type | Description |
| --- | --- | --- |
| `enabled` | `bool` | When `false`, completely bypasses Typescript in that executable (passes raw keystrokes). Essential for games. |
| `conversionThreshold` | `float` | Confidence score required to classify a word as Korean/Japanese (`0.0` to `1.0`). Default is `0.5`. Set to `0.97` in terminals to prevent short commands (`ls`, `git`) from being translated into Hangul. |
| `escapeToApp` | `bool` | Passes standard `Esc` keystrokes directly to the application (e.g., Vim Normal Mode). Language rotation can still be triggered via `Shift + Esc`. |
| `useSystemIme` | `bool` | Automatically falls back to the native Microsoft Korean IME when this window gains focus, returning to Typescript when focus leaves. |
| `fullWidth` | `bool` | Set to `false` to enforce half-width digits/spaces in specific apps (such as terminals), overriding global settings. |

---

### 4.3 `lexicon.json` (User Correction Memory)

Stores explicit user overrides generated when pressing `Esc` during or immediately following composition. Two consecutive identical corrections cement the word as a learned preference:

```json
{
  "gksrmf": {
    "target": "ko",
    "hits": 3
  },
  "apple": {
    "target": "en",
    "hits": 2
  }
}
```

---

### 4.4 `user_history.json` (Autocomplete Vocabulary)

Tracks frequency metrics for committed words in the `Typescript (Autocomplete)` profile.
- **Requires 2+ occurrences**: Only words typed at least twice appear in Tab autocomplete suggestions, preventing accidental password or typo exposure.
- Flushed to disk every 10 committed words and on input focus shifts, making newly learned vocabulary immediately available across applications.
