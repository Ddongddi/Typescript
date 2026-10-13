//! The text input processor: sinks for keys and focus, lazy per-thread session.
//! TSF loads this DLL into every application, so everything heavy (models, files) waits for
//! the first key typed, and nothing may panic across the COM boundary.

use std::cell::{Cell, RefCell, RefMut};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::atomic::Ordering;
use std::time::SystemTime;

use typescript_core::session::NoopClient;
use typescript_core::{AppPolicies, InputSession, Key, LearningEngine, Settings};
use windows::core::{implement, Error, IUnknownImpl, Interface, Ref, Result, BOOL, GUID};
use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::TextServices::*;

use crate::client::{EditSession, TsfClient};
use crate::display;
use crate::ghost::Ghost;
use crate::guids::{CLSID_TYPESCRIPT, GUID_DISPLAY_ATTRIBUTE, PROFILES};
use crate::keymap::{key_for, VK_HANGUL};
use crate::shared::{converter, engine, history, reload_engines};

/// Whether the active keyboard profile is the one with Tab completion (None: it isn't ours, or
/// TSF won't say).
fn active_profile_has_completion() -> Option<bool> {
    // SAFETY: COM objects created and used on this thread; the struct is plain data.
    unsafe {
        let profiles: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER).ok()?;
        let mut profile: TF_INPUTPROCESSORPROFILE = std::mem::zeroed();
        profiles
            .GetActiveProfile(&GUID_TFCAT_TIP_KEYBOARD, &mut profile)
            .ok()?;
        if profile.clsid != CLSID_TYPESCRIPT {
            return None;
        }
        PROFILES
            .iter()
            .find(|(guid, _, _)| *guid == profile.guidProfile)
            .map(|(_, _, completion)| *completion)
    }
}

fn guard<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(Error::from(E_FAIL)))
}

fn guard_bool(f: impl FnOnce() -> bool) -> BOOL {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(false).into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamps {
    settings: Option<SystemTime>,
    apps: Option<SystemTime>,
    lexicon: Option<SystemTime>,
}

impl Stamps {
    fn read() -> Self {
        let modified = |p: Option<PathBuf>| {
            p.and_then(|p| std::fs::metadata(p).ok())
                .and_then(|m| m.modified().ok())
        };
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
    /// The Tab-completion hint under the word being typed.
    pub ghost: Ghost,
    /// The active profile is the one with Tab completion (the picker offers both).
    completion_profile: Cell<bool>,
    /// The document manager that last got focus (a raw pointer, only ever compared).
    last_focus: Cell<usize>,
    cookie: Cell<u32>,
    /// A key handled in OnTestKeyDown (OnKeyDown then only confirms it).
    eaten_in_test: Cell<Option<u32>>,
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
            ghost: Ghost::default(),
            completion_profile: Cell::new(true),
            last_focus: Cell::new(0),
            cookie: Cell::new(0),
            eaten_in_test: Cell::new(None),
            exe,
            stamps: Cell::new(Stamps::read()),
        }
    }

    /// The typing session, built on first use (an app that never types never pays for it).
    pub fn session(&self) -> RefMut<'_, InputSession> {
        let mut slot = self.session.borrow_mut();
        if slot.is_none() {
            let policies = AppPolicies::load(AppPolicies::store_path().as_deref());
            let settings = self.settings.borrow();
            let mut session = InputSession::new(
                engine(settings.japanese),
                policies,
                settings.return_confirms_only,
            );
            if settings.japanese {
                session = session.with_japanese(converter(), settings.live_conversion);
            }
            session.set_full_width_digits(settings.full_width_digits);
            session.set_full_width_space(settings.full_width_space);
            if settings.autocomplete && self.completion_profile.get() {
                session = session.with_history(history());
            }
            session.activate(Some(&self.exe));
            *slot = Some(session);
        }
        RefMut::map(slot, |s| s.as_mut().expect("just built"))
    }

    /// Reads which of our profiles the user picked. Returns whether it changed (the session is
    /// then rebuilt with or without completion).
    fn refresh_profile(&self) -> bool {
        let Some(with_completion) = active_profile_has_completion() else {
            return false;
        };
        self.completion_profile.replace(with_completion) != with_completion
    }

    /// Appends an event to %APPDATA%\Typescript\debug.log, only while a file named debug.on sits
    /// next to it. Event names only: never what was typed, nor which keys.
    pub fn trace(&self, what: &str) {
        use std::io::Write;
        let Some(dir) = typescript_core::support_dir() else {
            return;
        };
        if !dir.join("debug.on").exists() {
            return;
        }
        let ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("debug.log"))
        {
            let _ = writeln!(file, "{ms} {} {what}", self.exe);
        }
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
        matches!(key, Key::Letter(_))
            || session.is_composing()
            || session.has_flip_window()
            || session.claims_key(key)
    }

    /// TSF asks whether we take the key. Letters: yes, typed in `key_down`. A key that may end
    /// the word and then belong to the app (Space, punctuation, Enter, Tab, arrows) is handled
    /// right here, because an app that talks IMM32 (KakaoTalk, any RichEdit) never gets back a
    /// key we said we'd take: the answer has to be final now.
    fn test_key_down(self: &Rc<Self>, context: &ITfContext, vk: u32, lparam: isize) -> bool {
        if !self.wants(vk) {
            return false;
        }
        if self.is_pause_key(vk) || matches!(key_for(vk), Some(Key::Letter(_))) {
            return true;
        }
        let eaten = self.key_down(context, vk, lparam);
        self.eaten_in_test.set(eaten.then_some(vk));
        eaten
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
            let handled = match key {
                Some(key) => session.handle(key, client),
                None => {
                    session.toggle_pause(client);
                    true
                }
            };
            client.show_hint(session.suggestion().as_deref());
            handled
        })
    }

    /// The text field changed: what was being composed stays as shown, the sentence context
    /// starts over, and files edited meanwhile (settings, apps, lexicon) are picked up.
    fn focus_changed(&self) {
        self.ghost.hide();
        if let Ok(mut composition) = self.composition.try_borrow_mut() {
            *composition = None;
        }
        let Ok(mut slot) = self.session.try_borrow_mut() else {
            return;
        };
        if let Some(session) = slot.as_mut() {
            session.commit_for_new_context(&mut NoopClient);
        }
        let profile_changed = self.refresh_profile();
        let stamps = Stamps::read();
        if profile_changed {
            *slot = None;
        } else if stamps != self.stamps.get() {
            self.stamps.set(stamps);
            *self.settings.borrow_mut() = Settings::load(Settings::store_path().as_deref());
            reload_engines();
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
        let mut client = TsfClient {
            ec,
            context: &ctx,
            inner: &shared,
        };
        out.set(work(&shared, &mut client));
    })
    .into();
    // SAFETY: `edit` outlives the call; a synchronous session has finished when this returns.
    let outcome = unsafe {
        context.RequestEditSession(
            inner.client_id.get(),
            &edit,
            TF_CONTEXT_EDIT_CONTEXT_FLAGS(TF_ES_SYNC.0 | TF_ES_READWRITE.0),
        )
    };
    matches!(outcome, Ok(hr) if hr.is_ok()) && result.get()
}

#[implement(
    ITfTextInputProcessorEx,
    ITfThreadMgrEventSink,
    ITfKeyEventSink,
    ITfDisplayAttributeProvider
)]
pub struct TextService {
    inner: Rc<Inner>,
}

impl TextService {
    pub fn new() -> Self {
        crate::OBJECTS.fetch_add(1, Ordering::SeqCst);
        Self {
            inner: Rc::new(Inner::new()),
        }
    }
}

impl TextService_Impl {
    fn activate(&self, tm: &ITfThreadMgr, tid: u32) -> Result<()> {
        let inner = &self.inner;
        *inner.thread_mgr.borrow_mut() = Some(tm.clone());
        inner.client_id.set(tid);
        inner.refresh_profile();
        let sink: ITfCompositionSink = CompositionSink {
            inner: Rc::downgrade(inner),
        }
        .into();
        *inner.comp_sink.borrow_mut() = Some(sink);

        let keys: ITfKeystrokeMgr = tm.cast()?;
        let key_sink: ITfKeyEventSink = self.to_interface();
        // SAFETY: plain COM calls with live interface pointers.
        unsafe { keys.AdviseKeyEventSink(tid, &key_sink, true)? };

        let source: ITfSource = tm.cast()?;
        let thread_sink: ITfThreadMgrEventSink = self.to_interface();
        inner
            .cookie
            .set(unsafe { source.AdviseSink(&ITfThreadMgrEventSink::IID, &thread_sink)? });

        // An app set to use the system's Korean IME: step aside once this callback is over.
        if AppPolicies::load(AppPolicies::store_path().as_deref())
            .policy(Some(&inner.exe))
            .use_system_ime
        {
            inner.trace("handing over to the system Korean IME");
            crate::delegate::hand_over();
        }

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
            inner.ghost.destroy();
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
    fn OnSetFocus(&self, focus: Ref<ITfDocumentMgr>, _previous: Ref<ITfDocumentMgr>) -> Result<()> {
        guard(|| {
            // Focus "moving" to the document that already has it is no text field change.
            let raw = focus.ok().map_or(0, |d| d.as_raw() as usize);
            if raw != 0 && self.inner.last_focus.replace(raw) == raw {
                self.inner.trace("focus: same document");
                return Ok(());
            }
            self.inner.last_focus.set(raw);
            self.inner.trace("focus: new document");
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

    fn OnTestKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let Ok(context) = pic.ok() else {
            return Ok(BOOL(0));
        };
        Ok(guard_bool(|| {
            self.inner.test_key_down(context, wparam.0 as u32, lparam.0)
        }))
    }

    fn OnTestKeyUp(&self, _pic: Ref<ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn OnKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let vk = wparam.0 as u32;
        if self.inner.eaten_in_test.take() == Some(vk) {
            return Ok(BOOL(1)); // already handled when TSF asked
        }
        let Ok(context) = pic.ok() else {
            return Ok(BOOL(0));
        };
        Ok(guard_bool(|| self.inner.key_down(context, vk, lparam.0)))
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
                inner.trace("composition ended by app");
                inner.ghost.hide();
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
