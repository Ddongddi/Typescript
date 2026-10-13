//! `TextClient` over a TSF edit session: this is where the session's UTF-16 ranges and
//! "marked text" meet ITfRange / ITfComposition. Everything here runs inside one edit
//! session (`ec` is only valid until it returns).

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use typescript_core::session::{Range, TextClient, NOT_FOUND};
use windows::core::{implement, Error, Interface, Result, BOOL};
use windows::Win32::Foundation::{E_FAIL, RECT};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::TextServices::*;

use crate::service::Inner;

type Work = Box<dyn FnOnce(u32)>;

/// An `ITfEditSession` that runs one closure.
#[implement(ITfEditSession)]
pub struct EditSession {
    work: RefCell<Option<Work>>,
}

impl EditSession {
    pub fn new(work: impl FnOnce(u32) + 'static) -> Self {
        Self {
            work: RefCell::new(Some(Box::new(work))),
        }
    }
}

impl ITfEditSession_Impl for EditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let work = self.work.borrow_mut().take();
        if let Some(work) = work {
            // A panic must not cross into the host application.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(ec)))
                .map_err(|_| Error::from(E_FAIL))?;
        }
        Ok(())
    }
}

pub struct TsfClient<'a> {
    pub ec: u32,
    pub context: &'a ITfContext,
    pub inner: &'a Inner,
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl TsfClient<'_> {
    /// Notes a failed step in the debug log (the step's name and HRESULT, never text) and passes
    /// the result on: a call an app refuses is otherwise invisible.
    fn logged<T>(&self, step: &str, result: Result<T>) -> Result<T> {
        if let Err(e) = &result {
            self.inner
                .trace(&format!("{step} failed: {:#010x}", e.code().0));
        }
        result
    }

    /// The selection as a (copy of a) range.
    fn selection(&self) -> Result<ITfRange> {
        // SAFETY: TF_SELECTION is plain data; GetSelection fills it when it reports a fetch.
        let mut sel = [unsafe { std::mem::zeroed::<TF_SELECTION>() }];
        let mut fetched = 0u32;
        unsafe {
            self.context
                .GetSelection(self.ec, TF_DEFAULT_SELECTION, &mut sel, &mut fetched)?
        };
        let [first] = sel;
        if fetched == 0 {
            return Err(E_FAIL.into());
        }
        ManuallyDrop::into_inner(first.range).ok_or_else(|| Error::from(E_FAIL))
    }

    fn extent(range: &ITfRange) -> Result<(i32, i32)> {
        let acp: ITfRangeACP = range.cast()?;
        let (mut start, mut len) = (0i32, 0i32);
        unsafe { acp.GetExtent(&mut start, &mut len)? };
        Ok((start, len))
    }

    /// Puts the caret after `range`.
    fn select_end(&self, range: &ITfRange) -> Result<()> {
        unsafe {
            range.Collapse(self.ec, TF_ANCHOR_END)?;
            let sel = TF_SELECTION {
                range: ManuallyDrop::new(Some(range.clone())),
                style: TF_SELECTIONSTYLE {
                    ase: TF_AE_NONE,
                    fInterimChar: false.into(),
                },
            };
            let result = self
                .context
                .SetSelection(self.ec, std::slice::from_ref(&sel));
            drop(ManuallyDrop::into_inner(sel.range)); // SetSelection took its own reference
            result
        }
    }

    /// Shows what Tab would complete the word to under it, or hides the hint (best effort).
    pub fn show_hint(&self, hint: Option<&str>) {
        let rect = hint.and_then(|_| self.word_rect());
        match (hint, rect) {
            (Some(text), Some(rect)) => self.inner.ghost.show(text, rect),
            _ => self.inner.ghost.hide(),
        }
    }

    /// Where the word being typed is on screen.
    fn word_rect(&self) -> Option<RECT> {
        let range = unsafe { self.inner.composition.borrow().as_ref()?.GetRange().ok()? };
        let view = unsafe { self.context.GetActiveView().ok()? };
        let (mut rect, mut clipped) = (RECT::default(), BOOL::default());
        unsafe {
            view.GetTextExt(self.ec, &range, &mut rect, &mut clipped)
                .ok()?
        };
        Some(rect)
    }

    /// Dotted underline while composing (best effort: not every app honors it).
    fn mark(&self, range: &ITfRange) {
        let atom = self.inner.da_atom.get();
        if atom == 0 {
            return;
        }
        let _ = self.logged("mark", unsafe {
            self.context
                .GetProperty(&GUID_PROP_ATTRIBUTE)
                .and_then(|p| p.SetValue(self.ec, range, &VARIANT::from(atom as i32)))
        });
    }

    /// Committed text must not keep the composition's underline.
    fn unmark(&self, range: &ITfRange) {
        if self.inner.da_atom.get() == 0 {
            return;
        }
        let _ = unsafe {
            self.context
                .GetProperty(&GUID_PROP_ATTRIBUTE)
                .and_then(|p| p.Clear(self.ec, range))
        };
    }

    fn set_marked(&self, text: &str) -> Result<()> {
        let chars = utf16(text);
        let mut slot = self.inner.composition.borrow_mut();
        if chars.is_empty() {
            if let Some(composition) = slot.take() {
                let range = unsafe { composition.GetRange()? };
                unsafe { range.SetText(self.ec, 0, &[])? };
                unsafe { composition.EndComposition(self.ec)? };
            }
            return Ok(());
        }
        if slot.is_none() {
            let insert: ITfInsertAtSelection =
                self.logged("cast ITfInsertAtSelection", self.context.cast())?;
            let at = self.logged("query selection", unsafe {
                insert.InsertTextAtSelection(self.ec, TF_IAS_QUERYONLY, &[])
            })?;
            let composing: ITfContextComposition =
                self.logged("cast ITfContextComposition", self.context.cast())?;
            let sink = self.inner.comp_sink.borrow().clone();
            *slot = Some(self.logged("StartComposition", unsafe {
                composing.StartComposition(self.ec, &at, sink.as_ref())
            })?);
            self.inner.trace("composition started");
        }
        let Some(composition) = slot.as_ref() else {
            return Err(E_FAIL.into());
        };
        let range = self.logged("GetRange", unsafe { composition.GetRange() })?;
        self.logged("SetText", unsafe { range.SetText(self.ec, 0, &chars) })?;
        self.mark(&range);
        self.logged("SetSelection", self.select_end(&range))
    }

    fn insert(&self, text: &str, replacing: Option<Range>) -> Result<()> {
        let chars = utf16(text);
        if let Some(r) = replacing {
            let range = self.selection()?;
            let acp: ITfRangeACP = range.cast()?;
            unsafe { acp.SetExtent(r.location as i32, r.length as i32)? };
            unsafe { range.SetText(self.ec, 0, &chars)? };
            return self.select_end(&range);
        }
        let composition = self.inner.composition.borrow_mut().take();
        if let Some(composition) = composition {
            let range = unsafe { composition.GetRange()? };
            unsafe { range.SetText(self.ec, 0, &chars)? };
            self.unmark(&range);
            let end = self.select_end(&range);
            unsafe { composition.EndComposition(self.ec)? };
            return end;
        }
        let insert: ITfInsertAtSelection = self.context.cast()?;
        let range = unsafe {
            insert.InsertTextAtSelection(self.ec, TF_IAS_NO_DEFAULT_COMPOSITION, &chars)?
        };
        self.select_end(&range)
    }
}

impl TextClient for TsfClient<'_> {
    fn set_marked_text(&mut self, text: &str) {
        let _ = self.logged("set_marked", self.set_marked(text));
    }

    fn insert_text(&mut self, text: &str, replacing: Option<Range>) {
        let _ = self.logged("insert", self.insert(text, replacing));
    }

    fn selected_range(&self) -> Range {
        match self.selection().and_then(|r| Self::extent(&r)) {
            Ok((start, len)) if start >= 0 && len >= 0 => Range {
                location: start as usize,
                length: len as usize,
            },
            _ => Range {
                location: NOT_FOUND,
                length: 0,
            },
        }
    }

    fn marked_range(&self) -> Range {
        let slot = self.inner.composition.borrow();
        let found = slot
            .as_ref()
            .and_then(|c| unsafe { c.GetRange().ok() })
            .and_then(|r| Self::extent(&r).ok());
        match found {
            Some((start, len)) if start >= 0 && len >= 0 => Range {
                location: start as usize,
                length: len as usize,
            },
            _ => Range {
                location: NOT_FOUND,
                length: 0,
            },
        }
    }

    fn text_in(&self, range: Range) -> Option<String> {
        let probe = self.selection().ok()?;
        let acp: ITfRangeACP = probe.cast().ok()?;
        unsafe {
            acp.SetExtent(range.location as i32, range.length as i32)
                .ok()?
        };
        let mut buffer = vec![0u16; range.length + 1];
        let mut count = 0u32;
        unsafe { probe.GetText(self.ec, 0, &mut buffer, &mut count).ok()? };
        String::from_utf16(&buffer[..count as usize]).ok()
    }
}
