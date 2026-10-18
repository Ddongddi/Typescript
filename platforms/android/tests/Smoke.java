package io.github.iamallyouneed.tacit;

public final class Smoke {
    private static String field = "", composing = "";
    private static void apply(String reply) {
        for (String op : reply.split("\u0001")) {
            if (op.startsWith("M") || op.startsWith("I")) {
                field = field.substring(0, field.length() - composing.length()) + op.substring(1);
                composing = op.startsWith("M") ? op.substring(1) : "";
            } else if (op.startsWith("B")) {
                field = field.substring(0, field.length() - Integer.parseInt(op.substring(1)));
                composing = "";
            }
        }
    }
    private static void press(long handle, int kind, char key) {
        String reply = Native.nativeKey(handle, kind, key, field);
        apply(reply);
        if (reply.startsWith("0")) field += key;
    }
    public static void main(String[] args) {
        long handle = Native.nativeCreate(args[0]);
        if (handle == 0) throw new AssertionError("nativeCreate returned null");
        try {
            Native.nativeActivate(handle, "smoke");
            for (char key : "dkssud".toCharArray()) {
                press(handle, Native.LETTER, key);
            }
            press(handle, Native.PRINTABLE, ' ');
            if (!field.equals("안녕 ")) throw new AssertionError("Korean: " + field);
            for (char key : "hello".toCharArray()) {
                press(handle, Native.LETTER, key);
            }
            press(handle, Native.PRINTABLE, ' ');
            if (!field.equals("안녕 hello ")) throw new AssertionError("English: " + field);
            apply(Native.nativeTogglePause(handle, field));
            if (!Native.nativePaused(handle)) throw new AssertionError("pause did not toggle");
            String passed = Native.nativeKey(handle, Native.LETTER, 'x', field);
            if (!passed.startsWith("0")) throw new AssertionError("paused key was consumed");
            apply(Native.nativeTogglePause(handle, field));
            if (Native.nativePaused(handle)) throw new AssertionError("pause did not resume");
            Native.nativeReset(handle);
            System.out.println("PASS: Android JNI create/activate/Korean/English/pause/reset/destroy");
        } finally {
            Native.nativeDestroy(handle);
        }
    }
}
