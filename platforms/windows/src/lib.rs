//! Typescript as a Windows text service (TSF): a COM DLL that Windows loads into every
//! application the user types in. The DLL exports the usual COM entry points plus
//! `EnableForUser` and `KeyHook` for `rundll32`.
//!
//! Rules of this crate: nothing panics across the COM boundary (see `guard` in service.rs;
//! the release profile keeps `panic = "unwind"` for that), and nothing heavy runs before the
//! first key.

#![allow(non_snake_case)]

mod candidates;
mod client;
mod delegate;
mod display;
mod ghost;
mod global;
mod guids;
mod hook;
mod keymap;
mod popup;
mod registry;
mod service;
mod shared;

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

use windows::core::{
    implement, Error, IUnknown, Interface, Ref, Result, BOOL, GUID, HRESULT, PCSTR,
};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, HMODULE, HWND, S_FALSE,
    S_OK,
};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::DisableThreadLibraryCalls;

use guids::CLSID_TYPESCRIPT;

static MODULE: AtomicIsize = AtomicIsize::new(0);
/// Live text services; `DllCanUnloadNow` only says yes at zero.
pub(crate) static OBJECTS: AtomicUsize = AtomicUsize::new(0);
static LOCKS: AtomicUsize = AtomicUsize::new(0);

/// Keeps the DLL loaded for the rest of the process: code of ours is registered with Windows
/// (a WinEvent hook) and must not disappear under it.
pub(crate) fn pin_module() {
    LOCKS.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn module_handle() -> HMODULE {
    HMODULE(MODULE.load(Ordering::SeqCst) as *mut c_void)
}

#[no_mangle]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(module.0 as isize, Ordering::SeqCst);
        // SAFETY: plain Win32 call on our own module.
        let _ = unsafe { DisableThreadLibraryCalls(module) };
    }
    BOOL(1)
}

fn guarded(f: impl FnOnce() -> HRESULT) -> HRESULT {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(E_FAIL)
}

/// # Safety
/// COM calls this with valid (or null) pointers, as `DllGetClassObject` always does.
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    clsid: *const GUID,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    guarded(|| {
        if clsid.is_null() || iid.is_null() || out.is_null() {
            return E_INVALIDARG;
        }
        // SAFETY: checked non-null; COM passes valid pointers.
        unsafe {
            *out = null_mut();
            if *clsid != CLSID_TYPESCRIPT {
                return CLASS_E_CLASSNOTAVAILABLE;
            }
            let factory: IClassFactory = ClassFactory.into();
            factory.query(iid, out)
        }
    })
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECTS.load(Ordering::SeqCst) == 0 && LOCKS.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    guarded(|| registry::register().map_or_else(|e| e.code(), |_| S_OK))
}

#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    guarded(|| registry::unregister().map_or_else(|e| e.code(), |_| S_OK))
}

/// `rundll32 typescript_tip.dll,EnableForUser`: switches the profile on for the current user.
#[no_mangle]
pub extern "system" fn EnableForUser(
    _hwnd: HWND,
    _instance: HMODULE,
    _command_line: PCSTR,
    _show: i32,
) {
    let _ = catch_unwind(AssertUnwindSafe(registry::enable_for_user));
}

/// `rundll32 typescript_tip.dll,KeyHook`: the Han/Eng key's shortcut, everywhere (started at
/// sign-in; runs until the session ends).
#[no_mangle]
pub extern "system" fn KeyHook(_hwnd: HWND, _instance: HMODULE, _command_line: PCSTR, _show: i32) {
    let _ = catch_unwind(AssertUnwindSafe(global::run));
}

#[implement(IClassFactory)]
struct ClassFactory;

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> Result<()> {
        if outer.is_some() {
            return Err(Error::from(CLASS_E_NOAGGREGATION));
        }
        let service: IUnknown = service::TextService::new().into();
        // SAFETY: COM passes valid `iid` / `out` pointers.
        unsafe { service.query(iid, out).ok() }
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            LOCKS.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCKS.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}
