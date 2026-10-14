# Typescript Troubleshooting & Application Compatibility

This guide resolves common configuration issues, key conflicts, and application quirks when using Typescript.

[한국어](../ko/troubleshooting.md) · [English Docs Index](README.md)

---

## 1. KakaoTalk & Legacy IMM32 App Compatibility

### 1.1 Characters Splitting Syllable-by-Syllable (`ㅌpㅅmㅌm`)
Legacy Windows applications—including KakaoTalk, older versions of Office, Notepad, and RichEdit controls—use the legacy **IMM32** subsystem rather than modern TSF.

#### Technical Background:
- Windows translates TSF composition events into IMM32 `WM_IME_COMPOSITION` messages.
- For Korean IMEs, Windows enforces a hardcoded constraint: **"Korean composition must never exceed 1 syllable (Interim state)."**
- Because Typescript presents an entire composing word with dotted previews, registering as a Korean IME causes Windows to prematurely finalize each keystroke. Typing `테스트` degenerates into `ㅌpㅅmㅌm`.

#### The Solution:
- Typescript registers under the **English (United States)** keyboard layout.
- In English layouts, Windows does not enforce single-character interim constraints, preserving multi-syllable word previews across all IMM32 applications.
- Seeing `ENG` in the Windows notification tray is intentional and expected.

### 1.2 Using the Native Microsoft Korean IME in Specific Apps
If corporate security software or banking applications require the official Microsoft Korean IME, configure an override in `apps.json`:

```json
{
  "banking_client.exe": {
    "useSystemIme": true
  }
}
```
Windows will automatically switch to the Microsoft Korean IME whenever that window gains focus, restoring Typescript when switching away.

---

## 2. Terminal & Console Optimization

### 2.1 Short Commands Becoming Hangul (`ls` → `ㅣㄴ`)
In shell environments, brief commands such as `ls`, `cd`, `dir`, `git`, or `cat` can occasionally be misclassified as Korean.

#### Solution:
Raise the classification threshold (`conversionThreshold`) in `apps.json` from the default `0.5` to `0.97`:

```json
{
  "windowsterminal.exe": {
    "conversionThreshold": 0.97
  },
  "powershell.exe": {
    "conversionThreshold": 0.97
  },
  "cmd.exe": {
    "conversionThreshold": 0.97
  }
}
```
At `0.97`, Typescript requires overwhelming statistical evidence before converting keystrokes to Korean.

---

## 3. Editor (Vim, VSCode) and Gaming Configuration

### 3.1 Vim/VSCode: Esc Key Not Returning to Normal Mode
By default, pressing `Esc` triggers Typescript's language cycling. In modal editors like Vim, this intercepts your attempt to return to Normal Mode.

#### Solution:
Enable `"escapeToApp": true` for your editor processes:

```json
{
  "code.exe": {
    "escapeToApp": true
  },
  "gvim.exe": {
    "escapeToApp": true
  }
}
```
- Raw `Esc` keypresses are forwarded directly to the editor.
- Language cycling remains accessible via **`Shift + Esc`**.

### 3.2 Full Pass-Through for Games
To eliminate any input interference in games, disable Typescript per executable:

```json
{
  "overwatch.exe": {
    "enabled": false
  },
  "steam.exe": {
    "enabled": false
  }
}
```

---

## 4. Diagnostics & Debug Logging (`debug.on`)

When troubleshooting crashes or unexpected behavior in a specific program:

### 4.1 Enabling Event Logs
1. Navigate to `%APPDATA%\Typescript\` (Windows) or your OS configuration folder.
2. Create an empty file named **`debug.on`**.
3. Relaunch the target application and reproduce the issue.
4. Open the newly generated **`debug.log`** file to inspect real-time TSF lifecycle events (focus acquisition, composition lifecycle, caret tracking).

> 🔒 **Privacy Guarantee**: Debug logs never capture typed text or keystrokes. Only internal TSF event IDs and error codes are recorded. Remember to delete `debug.on` after diagnostics.

### 4.2 RichEdit Diagnostic Harness
Test how RichEdit controls respond to TSF and IMM messages using the built-in laboratory:

```powershell
# Simulate Typescript interactions
cargo run -p typescript-tip --example richedit_lab -- --ts

# Compare against standard Microsoft IME interactions
cargo run -p typescript-tip --example richedit_lab -- --ms
```

---

## 5. Frequently Asked Questions (FAQ)

### Q: Why does the Windows taskbar show 'ENG' instead of '한'?
**A:** This is by design. Registering under the English (US) layout prevents Windows from clipping multi-syllable Korean words in legacy IMM32 software like KakaoTalk.

### Q: Pressing Esc right after typing a word doesn't flip it.
**A:** Post-commit undo/flip requires that the host application permits the input method to inspect preceding text. Terminals or sandboxed input boxes that deny caret-reading permissions pass `Esc` straight to the app.

### Q: The Hangul key (language switch) or Alt+Hangul (pause) doesn't respond.
**A:** On certain laptop keyboards, the Hangul key sends Right Alt (`VK_RMENU`). In the dashboard settings under "Key assignment", press `＋ 키 추가` and then the key you actually use. On macOS, use **Option + Space**. On Linux, use the **Hangul** or **Pause** key.
