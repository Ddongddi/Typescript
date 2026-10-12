//! Registration (regsvr32): the COM class in HKLM, the text service profile and its
//! categories with TSF, plus a per-user step that switches the profile on.

use windows::core::{Error, Result, PCWSTR};
use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::*;

use crate::guids::*;

fn dll_path() -> Result<String> {
    let mut buffer = [0u16; MAX_PATH as usize * 2];
    // SAFETY: the buffer is valid for the stated length.
    let len = unsafe { GetModuleFileNameW(Some(crate::module_handle()), &mut buffer) } as usize;
    if len == 0 {
        return Err(Error::from_thread());
    }
    Ok(String::from_utf16_lossy(&buffer[..len]))
}

fn set_string(subkey: &str, name: Option<&str>, value: &str) -> Result<()> {
    let subkey = wide(subkey);
    let name = name.map(wide);
    let data = wide(value);
    let mut key = HKEY::default();
    // SAFETY: all pointers refer to live, NUL-terminated buffers for the duration of the calls.
    unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let bytes = std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
        let name_ptr = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
        let result = RegSetValueExW(key, name_ptr, None, REG_SZ, Some(bytes)).ok();
        let _ = RegCloseKey(key);
        result
    }
}

fn init_com() {
    // regsvr32 has done this already; rundll32 hasn't. Either way a repeat is harmless.
    // SAFETY: plain COM initialization.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

const CATEGORIES: [windows::core::GUID; 4] = [
    GUID_TFCAT_TIP_KEYBOARD,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
    GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
];

pub fn register() -> Result<()> {
    init_com();
    let path = dll_path()?;
    let class = format!(r"SOFTWARE\Classes\CLSID\{}", guid_string(&CLSID_TYPESCRIPT));
    set_string(&class, None, DESCRIPTION)?;
    let server = format!(r"{class}\InprocServer32");
    set_string(&server, None, &path)?;
    set_string(&server, Some("ThreadingModel"), "Apartment")?;

    // SAFETY: COM objects created and used on this thread.
    unsafe {
        let profiles: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        // typescript.ico sits next to the DLL (install.ps1 puts it there); without it Windows
        // shows its default keyboard icon.
        let icon_path = std::path::Path::new(&path).with_file_name("typescript.ico");
        let icon: Vec<u16> =
            if icon_path.exists() { icon_path.to_string_lossy().encode_utf16().collect() } else { Vec::new() };
        for (guid, name, _) in PROFILES {
            let description: Vec<u16> = name.encode_utf16().collect();
            profiles.RegisterProfile(
                &CLSID_TYPESCRIPT,
                LANGID_KO_KR,
                &guid,
                &description,
                &icon,
                0,
                HKL::default(),
                0,
                true,
                0,
            )?;
        }
        let categories: ITfCategoryMgr = CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
        for category in CATEGORIES {
            categories.RegisterCategory(&CLSID_TYPESCRIPT, &category, &CLSID_TYPESCRIPT)?;
        }
    }
    Ok(())
}

pub fn unregister() -> Result<()> {
    init_com();
    // Keep going after a failure: a half-registered service is worse than a noisy error.
    let mut first_error = None;
    let mut note = |r: Result<()>| {
        if let Err(e) = r {
            first_error.get_or_insert(e);
        }
    };
    // SAFETY: COM objects created and used on this thread.
    unsafe {
        if let Ok(categories) = CoCreateInstance::<_, ITfCategoryMgr>(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER) {
            for category in CATEGORIES {
                note(categories.UnregisterCategory(&CLSID_TYPESCRIPT, &category, &CLSID_TYPESCRIPT));
            }
        }
        if let Ok(profiles) =
            CoCreateInstance::<_, ITfInputProcessorProfileMgr>(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
        {
            for (guid, _, _) in PROFILES {
                note(profiles.UnregisterProfile(&CLSID_TYPESCRIPT, LANGID_KO_KR, &guid, 0));
            }
        }
        let class = wide(&format!(r"SOFTWARE\Classes\CLSID\{}", guid_string(&CLSID_TYPESCRIPT)));
        note(RegDeleteTreeW(HKEY_LOCAL_MACHINE, PCWSTR(class.as_ptr())).ok());
    }
    first_error.map_or(Ok(()), Err)
}

/// Per user (no admin needed): turns the registered profile on for the current user.
pub fn enable_for_user() -> Result<()> {
    init_com();
    // SAFETY: COM objects created and used on this thread.
    unsafe {
        let profiles: ITfInputProcessorProfiles =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        for (guid, _, _) in PROFILES {
            profiles.EnableLanguageProfile(&CLSID_TYPESCRIPT, LANGID_KO_KR, &guid, true)?;
        }
        Ok(())
    }
}
