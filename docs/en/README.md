# Typescript Official Documentation (English)

Typescript is an intelligent, automatic multilingual Input Method Editor (IME) that lets you type without ever toggling a language key. As you type on a standard QWERTY keyboard, it analyzes keystrokes word-by-word to accurately distinguish and convert **English**, **Korean (2-set Hangul)**, and **Japanese (Romaji/Kana/Kanji)** on the fly.

[한국어 공식 문서](../ko/README.md) · [Main Documentation Index](../README.md)

---

## 📚 Table of Contents

1. [**Architecture & Core Principles**](architecture.md)
   - Input session state machine (`session`)
   - Word language decision pipeline (`decide`)
   - Language composition and models (`lang`)
   - English + Korean particle stripping and dynamic word learning
   - Memory-mapped (`mmap`) language pack architecture

2. [**Installation & Build Guide**](installation.md)
   - Windows (TSF COM text service one-liner, source build, uninstall)
   - macOS (InputMethodKit build and activation)
   - Linux (IBus engine build and desktop integration)
   - Android (Standalone APK download and source build)
   - Language pack downloads and offline deployment

3. [**Configuration & Management**](configuration.md)
   - Local web dashboard (`typescript-config`) usage
   - CLI command reference
   - Configuration files (`settings.json`, `apps.json`, `lexicon.json`)
   - Per-app custom rules (Terminal, Editor, Gaming optimizations)
   - Word autocomplete and vocabulary learning

4. [**Language Pack System**](language-packs.md)
   - Language pack structure and specification (`manifest.json`, binary tables)
   - `typescript-packgen` toolchain usage
   - Step-by-step procedure for adding new languages
   - Dataset requirements and licensing compliance

5. [**Troubleshooting & App Compatibility**](troubleshooting.md)
   - KakaoTalk and RichEdit IMM32 compatibility (why English layout registration is necessary)
   - Resolving key conflicts in terminals (PowerShell, WSL) and IDEs (Vim, VSCode)
   - Event logging (`debug.on`) and diagnostic workflow
   - Known limitations across platforms and FAQ

---

## ⚡ Quick Start (Windows)

Run the following command in PowerShell with administrator privileges to download, install, and activate Typescript:

```powershell
irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1 | iex
```

During installation, an interactive terminal selector allows you to choose your active languages (English, Korean, Japanese) and their cycling order. After installation, press **Win + Space** and select **Typescript** or **Typescript (Autocomplete)**.

To open the graphical settings dashboard, click **Typescript Settings** from the Start Menu or run:

```powershell
typescript-config
```
