//! The text input processor: sinks for keys and focus, lazy per-thread session.
//! TSF loads this DLL into every application, so everything heavy (models, files) waits for
//! the first key typed, and nothing may panic across the COM boundary.

use std::cell::{Cell, RefCell, RefMut};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use typescript_core::session::NoopClient;
use typescript_core::{make_engine, AppPolicies, InputSession, Key, LearningEngine, Settings};
use windows::core::{implement, Error, IUnknownImpl, Interface, Ref, Result, BOOL, GUID};
use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::TextServices::*;

use crate::client::{EditSession, TsfClient};
use crate::display;
use crate::guids::GUID_DISPLAY_ATTRIBUTE;

const VK_HANGUL: u32 = 0x15;
const VK_BACK: u32 = 0x08;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: i32 = 0x12;
const VK_CAPITAL: i32 = 0x14;

static ENGINE: OnceLock<Arc<LearningEngine>> = OnceLock::new();

/// One engine per process: models and learned words are shared by every text service in it.
fn engine() -> Arc<LearningEngine> {
    ENGINE.get_or_init(|| make_engine(LearningEngine::default_store_path())).clone()
}

fn guard<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(Error::from(E_FAIL)))
}

fn guard_bool(f: impl FnOnce() -> bool) -> BOOL {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(false).into()
}

fn is_down(vk: i32) -> bool {
    // SAFETY: plain Win32 query.
    unsafe { GetKeyState(vk) < 0 }
}

/// The characters of a US QWERTY keyboard, which is what the original assumes too.
fn printable(vk: u32, shift: bool) -> Option<char> {
    let pick = |a: char, b: char| Some(if shift { b } else { a });
    match vk {
        0x20 => Some(' '),
        0x30 => pick('0', ')'),
        0x31 => pick('1', '!'),
        0x32 => pick('2', '@'),
        0x33 => pick('3', '#'),
        0x34 => pick('4', '$'),
        0x35 => pick('5', '%'),
        0x36 => pick('6', '^'),
        0x37 => pick('7', '&'),
        0x38 => pick('8', '*'),
        0x39 => pick('9', '('),
        0xBA => pick(';', ':'),
        0xBB => pick('=', '+'),
        0xBC => pick(',', '<'),
        0xBD => pick('-', '_'),
        0xBE => pick('.', '>'),
        0xBF => pick('/', '?'),
        0xC0 => pick('`', '~'),
        0xDB => pick('[', '{'),
        0xDC => pick('\\', '|'),
        0xDD => pick(']', '}'),
        0xDE => pick('\'', '"'),
        0x60..=0x69 => Some((b'0' + (vk - 0x60) as u8) as char),
        0x6A => Some('*'),
        0x6B => Some('+'),
        0x6D => Some('-'),
        0x6E => Some('.'),
        0x6F => Some('/'),
        _ => None,
    }
}

/// Keys that mean nothing to a word being typed: modifiers, locks, IME keys.
fn is_ignored(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0..=0xA5 | 0x5B | 0x5C | 0x14 | 0x90 | 0x91 | 0x15 | 0x17 | 0x19 | 0xE5 | 0xFF)
}

/// The session's view of a virtual key, or None for keys it must not see at all.
fn key_for(vk: u32) -> Option<Key> {
    if is_ignored(vk) {
        return None;
    }
    let (shift, ctrl, alt) = (is_down(VK_SHIFT), is_down(VK_CONTROL), is_down(VK_MENU));
    if ctrl || alt {
        return Some(Key::Other); // a shortcut: the word goes in, the app gets the key
    }
    Some(match vk {
        0x41..=0x5A => {
            let upper = shift != (unsafe { GetKeyState(VK_CAPITAL) } & 1 != 0);
            let c = vk as u8 as char;
            Key::Letter(if upper { c } else { c.to_ascii_lowercase() })
        }
        VK_BACK => Key::Delete,
        VK_ESCAPE if shift => Key::Cycle,
        VK_ESCAPE => Key::Escape,
        VK_RETURN => Key::Enter,
        _ => match printable(vk, shift) {
            Some(c) => Key::Printable(c),
            None => Key::Other,
        },
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamps {
    settings: Option<SystemTime>,
    apps: Option<SystemTime>,
    lexicon: Option<SystemTime>,
}

impl Stamps {
    fn read() -> Self {
        let modified = |p: Option<PathBuf>| p.and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        Self {
            settings: modified(Settings::store_path()),
            apps: modified(AppPolicies::store_path()),
            lexicon: modified(LearningEngine::default_store_path()),
        }
    }
}

/// State shared by the text service and its helper COM objects.
pub struct Inner {
    thread_mgr: RefCell<Option<ITfThreadMgr>>,
    pub client_id: Cell<u32>,
    session: RefCell<Option<InputSession>>,
    settings: RefCell<Settings>,
    /// The composition being typed into, while there is one.
    pub composition: RefCell<Option<ITfComposition>>,
    pub comp_sink: RefCell<Option<ITfCompositionSink>>,
    /// Atom of the display attribute (0: not registered).
    pub da_atom: Cell<u32>,
    cookie: Cell<u32>,
    exe: String,
    stamps: Cell<Stamps>,
}

impl Inner {
    fn new() -> Self {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
            .unwrap_or_default();
        Self {
            thread_mgr: RefCell::new(None),
            client_id: Cell::new(0),
            session: RefCell::new(None),
            settings: RefCell::new(Settings::load(Settings::store_path().as_deref())),
            composition: RefCell::new(None),
            comp_sink: RefCell::new(None),
            da_atom: Cell::new(0),
            cookie: Cell::new(0),
            exe,
            stamps: Cell::new(Stamps::read()),
        }
    }

    /// The typing session, built on first use (an app that never types never pays for it).
    pub fn session(&self) -> RefMut<'_, InputSession> {
        let mut slot = self.session.borrow_mut();
        if slot.is_none() {
            let policies = AppPolicies::load(AppPolicies::store_path().as_deref());
            let mut session =
                InputSession::new(engine(), policies, self.settings.borrow().return_confirms_only);
            session.activate(Some(&self.exe));
            *slot = Some(session);
        }
        RefMut::map(slot, |s| s.as_mut().expect("just built"))
    }

    fn is_pause_key(&self, vk: u32) -> bool {
        vk == VK_HANGUL && self.settings.borrow().pause_key == "hangul"
    }

    /// Whether the key is ours to look at (TSF asks before it delivers).
    fn wants(&self, vk: u32) -> bool {
        if self.is_pause_key(vk) {
            return true;
        }
        let Some(key) = key_for(vk) else { return false };
        let session = self.session();
        if !session.policy().enabled || session.paused() {
            return false;
        }
        matches!(key, Key::Letter(_)) || session.is_composing() || session.has_flip_window()
    }

    fn key_down(self: &Rc<Self>, context: &ITfContext, vk: u32, lparam: isize) -> bool {
        let pause = self.is_pause_key(vk);
        if pause && (lparam >> 30) & 1 == 1 {
            return true; // auto-repeat of the pause key
        }
        if !pause && !self.wants(vk) {
            return false;
        }
        let key = if pause { None } else { key_for(vk) };
        run_edit(self, context, move |inner, client| {
            let mut session = inner.session();
            match key {
                Some(key) => session.handle(key, client),
                None => {
                    session.toggle_pause(client);
                    true
                }
            }
        })
    }

    /// The text field changed: what was being composed stays as shown, the sentence context
    /// starts over, and files edited meanwhile (settings, apps, lexicon) are picked up.
    fn focus_changed(&self) {
        if let Ok(mut composition) = self.composition.try_borrow_mut() {
            *composition = None;
        }
        let Ok(mut slot) = self.session.try_borrow_mut() else { return };
        if let Some(session) = slot.as_mut() {
            session.commit_for_new_context(&mut NoopClient);
        }
        let stamps = Stamps::read();
        if stamps != self.stamps.get() {
            self.stamps.set(stamps);
            *self.settings.borrow_mut() = Settings::load(Settings::store_path().as_deref());
            engine().reload();
            *slot = None; // rebuilt with the new files on the next key
        } else if let Some(session) = slot.as_mut() {
            session.activate(Some(&self.exe));
        }
    }
}

/// Runs `work` in a synchronous read/write edit session on `context`. False when the app
/// refused the session (the key then goes to the app as typed).
fn run_edit(
    inner: &Rc<Inner>,
    context: &ITfContext,
    work: impl FnOnce(&Inner, &mut TsfClient) -> bool + 'static,
) -> bool {
    let result = Rc::new(Cell::new(false));
    let (shared, ctx, out) = (inner.clone(), context.clone(), result.clone());
    let edit: ITfEditSession = EditSession::new(move |ec| {
        let mut client = TsfClient { ec, context: &ctx, inner: &shared };
        out.set(work(&shared, &mut client));
    })
    .into();
    // SAFETY: `edit` outlives the call; a synchronous session has finished when this returns.
    let outcome = unsafe {
        context.RequestEditSession(inner.client_id.get(), &edit, TF_CONTEXT_EDIT_CONTEXT_FLAGS(TF_ES_SYNC.0 | TF_ES_READWRITE.0))
    };
    matches!(outcome, Ok(hr) if hr.is_ok()) && result.get()
}

#[implement(ITfTextInputProcessorEx, ITfThreadMgrEventSink, ITfKeyEventSink, ITfDisplayAttributeProvider)]
pub struct TextService {
    inner: Rc<Inner>,
}

impl TextService {
    pub fn new() -> Self {
        crate::OBJECTS.fetch_add(1, Ordering::SeqCst);
        Self { inner: Rc::new(Inner::new()) }
    }

}

impl TextService_Impl {
    fn activate(&self, tm: &ITfThreadMgr, tid: u32) -> Result<()> {
        let inner = &self.inner;
        *inner.thread_mgr.borrow_mut() = Some(tm.clone());
        inner.client_id.set(tid);
        let sink: ITfCompositionSink = CompositionSink { inner: Rc::downgrade(inner) }.into();
        *inner.comp_sink.borrow_mut() = Some(sink);

        let keys: ITfKeystrokeMgr = tm.cast()?;
        let key_sink: ITfKeyEventSink = self.to_interface();
        // SAFETY: plain COM calls with live interface pointers.
        unsafe { keys.AdviseKeyEventSink(tid, &key_sink, true)? };

        let source: ITfSource = tm.cast()?;
        let thread_sink: ITfThreadMgrEventSink = self.to_interface();
        inner.cookie.set(unsafe { source.AdviseSink(&ITfThreadMgrEventSink::IID, &thread_sink)? });

        // The underline is cosmetic: a failure here just means no underline.
        if let Ok(categories) = unsafe {
            CoCreateInstance::<_, ITfCategoryMgr>(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)
        } {
            if let Ok(atom) = unsafe { categories.RegisterGUID(&GUID_DISPLAY_ATTRIBUTE) } {
                inner.da_atom.set(atom);
            }
        }
        Ok(())
    }
}

impl Drop for TextService {
    fn drop(&mut self) {
        crate::OBJECTS.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        self.ActivateEx(ptim, tid, 0)
    }

    fn Deactivate(&self) -> Result<()> {
        guard(|| {
            let inner = &self.inner;
            if let Ok(mut slot) = inner.session.try_borrow_mut() {
                if let Some(session) = slot.as_mut() {
                    session.commit_for_new_context(&mut NoopClient);
                }
            }
            if let Some(tm) = inner.thread_mgr.borrow_mut().take() {
                if let Ok(keys) = tm.cast::<ITfKeystrokeMgr>() {
                    let _ = unsafe { keys.UnadviseKeyEventSink(inner.client_id.get()) };
                }
                if let Ok(source) = tm.cast::<ITfSource>() {
                    let _ = unsafe { source.UnadviseSink(inner.cookie.get()) };
                }
            }
            *inner.composition.borrow_mut() = None;
            *inner.comp_sink.borrow_mut() = None;
            inner.client_id.set(0);
            Ok(())
        })
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(&self, ptim: Ref<ITfThreadMgr>, tid: u32, _flags: u32) -> Result<()> {
        guard(|| {
            let tm = ptim.ok()?.clone();
            self.activate(&tm, tid)
        })
    }
}

impl ITfThreadMgrEventSink_Impl for TextService_Impl {
    fn OnInitDocumentMgr(&self, _pdim: Ref<ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }
    fn OnUninitDocumentMgr(&self, _pdim: Ref<ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }
    fn OnSetFocus(&self, _focus: Ref<ITfDocumentMgr>, _previous: Ref<ITfDocumentMgr>) -> Result<()> {
        guard(|| {
            self.inner.focus_changed();
            Ok(())
        })
    }
    fn OnPushContext(&self, _pic: Ref<ITfContext>) -> Result<()> {
        Ok(())
    }
    fn OnPopContext(&self, _pic: Ref<ITfContext>) -> Result<()> {
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, _foreground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(&self, _pic: Ref<ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(guard_bool(|| self.inner.wants(wparam.0 as u32)))
    }

    fn OnTestKeyUp(&self, _pic: Ref<ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn OnKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let Ok(context) = pic.ok() else { return Ok(BOOL(0)) };
        Ok(guard_bool(|| self.inner.key_down(context, wparam.0 as u32, lparam.0)))
    }

    fn OnKeyUp(&self, _pic: Ref<ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn OnPreservedKey(&self, _pic: Ref<ITfContext>, _guid: *const GUID) -> Result<BOOL> {
        Ok(BOOL(0))
    }
}

impl ITfDisplayAttributeProvider_Impl for TextService_Impl {
    fn EnumDisplayAttributeInfo(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        Ok(display::enumerate())
    }

    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        // SAFETY: TSF passes a valid GUID pointer.
        if guid.is_null() || unsafe { *guid } != GUID_DISPLAY_ATTRIBUTE {
            return Err(E_INVALIDARG.into());
        }
        Ok(display::info())
    }
}

/// Told when the app ends our composition itself (a click elsewhere, say).
#[implement(ITfCompositionSink)]
struct CompositionSink {
    inner: Weak<Inner>,
}

impl ITfCompositionSink_Impl for CompositionSink_Impl {
    fn OnCompositionTerminated(&self, _ec: u32, _composition: Ref<ITfComposition>) -> Result<()> {
        guard(|| {
            if let Some(inner) = self.inner.upgrade() {
                if let Ok(mut composition) = inner.composition.try_borrow_mut() {
                    *composition = None;
                }
                // The text on screen stays as the app has it; only our state starts over.
                if let Ok(mut slot) = inner.session.try_borrow_mut() {
                    if let Some(session) = slot.as_mut() {
                        session.commit_for_new_context(&mut NoopClient);
                    }
                }
            }
            Ok(())
        })
    }
}
