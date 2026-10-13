//! JNI surface of the Android input method: a handful of functions, no logic. The Java side
//! (`Native.java`) passes keys and the text before the caret, and applies the commands returned.

pub mod ime;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jlong, jstring};
use jni::JNIEnv;

use ime::Ime;

fn text(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(String::from).unwrap_or_default()
}

/// Runs `f` on the instance behind `handle`; a panic must not unwind into the JVM.
fn with<R>(handle: jlong, fallback: R, f: impl FnOnce(&mut Ime) -> R) -> R {
    // SAFETY: `handle` comes from `nativeCreate` and is used until `nativeDestroy`.
    let ime = unsafe { &*(handle as *const Mutex<Ime>) };
    catch_unwind(AssertUnwindSafe(|| {
        let mut ime = ime.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut ime)
    }))
    .unwrap_or(fallback)
}

fn reply(env: &JNIEnv, s: &str) -> jstring {
    env.new_string(s)
        .map_or(std::ptr::null_mut(), |s| s.into_raw())
}

#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativeCreate(
    mut env: JNIEnv,
    _: JClass,
    dir: JString,
) -> jlong {
    let dir = PathBuf::from(text(&mut env, &dir));
    let ime = catch_unwind(|| Ime::new(&dir));
    ime.map_or(0, |ime| Box::into_raw(Box::new(Mutex::new(ime))) as jlong)
}

#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativeDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    if handle != 0 {
        // SAFETY: the handle is not used after this call.
        drop(unsafe { Box::from_raw(handle as *mut Mutex<Ime>) });
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativeActivate(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    package: JString,
) {
    let package = text(&mut env, &package);
    with(handle, (), |ime| ime.activate(&package));
}

#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativeKey(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    kind: jint,
    ch: jint,
    before: JString,
) -> jstring {
    let before = text(&mut env, &before);
    let ch = char::from_u32(ch as u32).unwrap_or('\0');
    let out = with(handle, "0".to_string(), |ime| ime.key(kind, ch, &before));
    reply(&env, &out)
}

#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativeReset(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    with(handle, (), |ime| ime.reset());
}

#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativeTogglePause(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    before: JString,
) -> jstring {
    let before = text(&mut env, &before);
    let out = with(handle, "0".to_string(), |ime| ime.toggle_pause(&before));
    reply(&env, &out)
}

#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativePaused(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jboolean {
    with(handle, false, |ime| ime.paused()) as jboolean
}

/// The word Tab would complete to, or an empty string.
#[no_mangle]
pub extern "system" fn Java_io_github_iamallyouneed_typescript_Native_nativeSuggestion(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jstring {
    let word = with(handle, None, |ime| ime.suggestion()).unwrap_or_default();
    reply(&env, &word)
}
