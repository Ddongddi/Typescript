//! The display attribute of the composition: a dotted underline, so the word being decided
//! reads as "not yet committed" in apps that don't draw composition text themselves.

use std::cell::Cell;

use windows::core::{implement, Error, Result, BSTR, GUID};
use windows::Win32::Foundation::{E_INVALIDARG, S_FALSE};
use windows::Win32::UI::TextServices::*;

use crate::guids::GUID_DISPLAY_ATTRIBUTE;

fn attribute() -> TF_DISPLAYATTRIBUTE {
    // Colors stay TF_CT_NONE (the default): the app's own text and background.
    TF_DISPLAYATTRIBUTE {
        lsStyle: TF_LS_DOT,
        bAttr: TF_ATTR_INPUT,
        ..Default::default()
    }
}

pub fn info() -> ITfDisplayAttributeInfo {
    DisplayInfo.into()
}

pub fn enumerate() -> IEnumTfDisplayAttributeInfo {
    EnumInfo {
        done: Cell::new(false),
    }
    .into()
}

#[implement(ITfDisplayAttributeInfo)]
struct DisplayInfo;

impl ITfDisplayAttributeInfo_Impl for DisplayInfo_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(GUID_DISPLAY_ATTRIBUTE)
    }
    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("Typescript composition"))
    }
    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        if pda.is_null() {
            return Err(E_INVALIDARG.into());
        }
        // SAFETY: the caller passes a valid out pointer (checked non-null above).
        unsafe { *pda = attribute() };
        Ok(())
    }
    fn SetAttributeInfo(&self, _pda: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        Ok(()) // fixed
    }
    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

/// Enumerates the one attribute. `Next` ends with S_FALSE, which windows-rs spells as an Err
/// carrying that (success) code.
#[implement(IEnumTfDisplayAttributeInfo)]
struct EnumInfo {
    done: Cell<bool>,
}

impl IEnumTfDisplayAttributeInfo_Impl for EnumInfo_Impl {
    fn Clone(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        Ok(EnumInfo {
            done: Cell::new(self.done.get()),
        }
        .into())
    }
    fn Next(
        &self,
        count: u32,
        out: *mut Option<ITfDisplayAttributeInfo>,
        fetched: *mut u32,
    ) -> Result<()> {
        let mut got = 0u32;
        if count > 0 && !self.done.get() && !out.is_null() {
            // SAFETY: `out` has room for `count >= 1` entries by the IEnum contract.
            unsafe { out.write(Some(info())) };
            self.done.set(true);
            got = 1;
        }
        if !fetched.is_null() {
            // SAFETY: non-null out pointer.
            unsafe { *fetched = got };
        }
        if got == count {
            Ok(())
        } else {
            Err(Error::from(S_FALSE))
        }
    }
    fn Reset(&self) -> Result<()> {
        self.done.set(false);
        Ok(())
    }
    fn Skip(&self, count: u32) -> Result<()> {
        if count > 0 {
            self.done.set(true);
        }
        Ok(())
    }
}
