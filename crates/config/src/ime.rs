//! Which input method the user has selected right now, so the dashboard can say when it is not
//! tacit (the keys and the typing it configures only work while tacit is selected).

use serde_json::{json, Value};

/// What `/api/heartbeat` reports: `{"ime": null}` where the system cannot say (not Windows, or
/// TSF does not answer), else `{"ime": {"tacit": bool, "name": "..."}}`.
pub fn status() -> Value {
    match active() {
        Some((tacit, name)) => json!({ "ime": { "tacit": tacit, "name": name } }),
        None => json!({ "ime": null }),
    }
}

/// On-demand local diagnostics. Missing OS information is null rather than guessed.
pub fn diagnostics() -> Value {
    #[allow(unused_mut)]
    let mut value = status();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); @{defaultInputMethod=(Get-WinDefaultInputMethodOverride).InputMethodTip; perAppInputMethod=(Get-WinLanguageBarOption).IsLegacySwitchingMode} | ConvertTo-Json -Compress"])
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(extra) = serde_json::from_slice::<Value>(&output.stdout) {
                    value["defaultInputMethod"] = extra["defaultInputMethod"].clone();
                    value["perAppInputMethod"] = extra["perAppInputMethod"].clone();
                }
            }
        }
        value["activeApp"] = foreground_app().into();
    }
    value
}

#[cfg(windows)]
fn foreground_app() -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    // SAFETY: query-only handle, bounded buffer, and the handle is closed before returning.
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 32768];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        Some(
            std::path::Path::new(&path)
                .file_name()?
                .to_string_lossy()
                .to_lowercase(),
        )
    }
}

/// macOS: the selected input source from `defaults read com.apple.HIToolbox
/// AppleSelectedInputSources`. tacit is selected when its name is in it; otherwise the
/// keyboard layout (or input mode) it names is the name.
#[cfg(any(target_os = "macos", test))]
fn parse_macos_input_source(output: &str) -> Option<(bool, String)> {
    if output.trim().is_empty() {
        return None;
    }
    if output.to_lowercase().contains("tacit") {
        return Some((true, "Tacit".to_string()));
    }
    let name = output
        .lines()
        .filter(|l| l.contains("KeyboardLayout Name") || l.contains("Input Mode"))
        .find_map(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_end_matches(';').trim_matches('"').to_string())
        .unwrap_or_default();
    Some((false, name))
}

#[cfg(target_os = "macos")]
fn active() -> Option<(bool, String)> {
    let out = std::process::Command::new("defaults")
        .args(["read", "com.apple.HIToolbox", "AppleSelectedInputSources"])
        .output()
        .ok()?;
    parse_macos_input_source(&String::from_utf8_lossy(&out.stdout))
}

/// Linux (IBus): `ibus engine` names the engine in use; tacit's is `tacit`.
#[cfg(all(unix, not(target_os = "macos")))]
fn active() -> Option<(bool, String)> {
    let out = std::process::Command::new("ibus")
        .arg("engine")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!name.is_empty()).then(|| (name == "tacit", name))
}

#[cfg(not(any(windows, unix)))]
fn active() -> Option<(bool, String)> {
    None
}

/// The CLSID of the tacit text service (platforms/windows/src/guids.rs).
#[cfg(windows)]
const CLSID_TACIT: windows::core::GUID =
    windows::core::GUID::from_u128(0xE231DAD4_326F_436F_B74C_D08F1D66D822);

#[cfg(windows)]
fn active() -> Option<(bool, String)> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::TextServices::*;

    // SAFETY: COM calls on this thread; every pointer is one the calls themselves fill in. The
    // thread is only uninitialized when this call initialized it.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let found = (|| {
            let manager: ITfInputProcessorProfileMgr =
                CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
                    .ok()?;
            let mut profile: TF_INPUTPROCESSORPROFILE = std::mem::zeroed();
            manager
                .GetActiveProfile(&GUID_TFCAT_TIP_KEYBOARD, &mut profile)
                .ok()?;
            if profile.clsid == CLSID_TACIT {
                return Some((true, "Tacit".to_string()));
            }
            let name = if profile.dwProfileType == TF_PROFILETYPE_INPUTPROCESSOR {
                let profiles: ITfInputProcessorProfiles = manager.cast().ok()?;
                profiles
                    .GetLanguageProfileDescription(
                        &profile.clsid,
                        profile.langid,
                        &profile.guidProfile,
                    )
                    .map(|name| name.to_string())
                    .unwrap_or_default()
            } else {
                String::new() // a plain keyboard layout has no text service to name
            };
            Some((false, name))
        })();
        if initialized {
            CoUninitialize();
        }
        found
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_status_is_always_an_ime_object_or_null() {
        let status = super::status();
        let ime = &status["ime"];
        assert!(ime.is_null() || ime["tacit"].is_boolean());
    }

    use super::*;

    #[test]
    fn the_selected_macos_input_source_is_read_from_the_defaults_dump() {
        let abc = "(\n    {\n        \"InputSourceKind\" = \"Keyboard Layout\";\n        \"KeyboardLayout ID\" = 252;\n        \"KeyboardLayout Name\" = ABC;\n    }\n)";
        assert_eq!(
            parse_macos_input_source(abc),
            Some((false, "ABC".to_string()))
        );
        let ours = "(\n {\n \"Bundle ID\" = \"io.github.iamallyouneed.inputmethod.tacit\";\n }\n)";
        assert_eq!(
            parse_macos_input_source(ours),
            Some((true, "Tacit".to_string()))
        );
        assert_eq!(parse_macos_input_source("  "), None);
    }
}
