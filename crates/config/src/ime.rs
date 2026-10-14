//! Which input method the user has selected right now, so the dashboard can say when it is not
//! Typescript (the keys and the typing it configures only work while Typescript is selected).

use serde_json::{json, Value};

/// What `/api/heartbeat` reports: `{"ime": null}` where the system cannot say (not Windows, or
/// TSF does not answer), else `{"ime": {"typescript": bool, "name": "..."}}`.
pub fn status() -> Value {
    match active() {
        Some((typescript, name)) => json!({ "ime": { "typescript": typescript, "name": name } }),
        None => json!({ "ime": null }),
    }
}

#[cfg(not(windows))]
fn active() -> Option<(bool, String)> {
    None
}

/// The CLSID of the Typescript text service (platforms/windows/src/guids.rs).
#[cfg(windows)]
const CLSID_TYPESCRIPT: windows::core::GUID =
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
            if profile.clsid == CLSID_TYPESCRIPT {
                return Some((true, "Typescript".to_string()));
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
        assert!(ime.is_null() || ime["typescript"].is_boolean());
    }
}
