package io.github.iamallyouneed.typescript;

/** The Rust side (platforms/android/native). All logic lives there; this only declares it. */
final class Native {
    static {
        System.loadLibrary("typescript_android");
    }

    // Key kinds, as in ime.rs.
    static final int LETTER = 0;
    static final int PRINTABLE = 1;
    static final int DELETE = 2;
    static final int ESCAPE = 3;
    static final int CYCLE = 4;
    static final int ENTER = 5;
    static final int TAB = 6;
    static final int OTHER = 7;

    static native long nativeCreate(String dir);

    static native void nativeDestroy(long handle);

    static native void nativeActivate(long handle, String packageName);

    /** "1" or "0" (key consumed), then each edit command preceded by U+0001. */
    static native String nativeKey(long handle, int kind, int ch, String before);

    static native void nativeReset(long handle);

    static native String nativeTogglePause(long handle, String before);

    static native boolean nativePaused(long handle);
}
