package io.github.iamallyouneed.tacit;

/** The Rust side (platforms/android/native). All logic lives there; this only declares it. */
final class Native {
    static {
        System.loadLibrary("tacit_android");
    }

    // Key kinds, as in crates/core/src/session/bridge.rs.
    static final int LETTER = 0;
    static final int PRINTABLE = 1;
    static final int DELETE = 2;
    static final int ESCAPE = 3;
    static final int CYCLE = 4;
    static final int ENTER = 5;
    static final int TAB = 6;
    static final int OTHER = 7;

    // Modes, as nativeMode reports them (same file).
    static final int MODE_AUTO = 0;
    static final int MODE_KOREAN = 1;
    static final int MODE_ENGLISH = 2;

    static native long nativeCreate(String dir);

    static native void nativeDestroy(long handle);

    static native void nativeActivate(long handle, String packageName);

    /** "1" or "0" (key consumed), then each edit command preceded by U+0001. */
    static native String nativeKey(long handle, int kind, int ch, String before);

    static native void nativeReset(long handle);

    /** Next mode: tacit, Korean, English. Same reply as nativeKey. */
    static native String nativeCycleMode(long handle, String before);

    static native int nativeMode(long handle);

    /** Each reading of the word being typed preceded by U+0001 and "1" if shown, else "0". */
    static native String nativeReadings(long handle);

    /** Reads the word as the index-th reading. Same reply as nativeKey. */
    static native String nativeChooseReading(long handle, int index, String before);
}
