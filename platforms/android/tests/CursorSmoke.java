package io.github.iamallyouneed.tacit;

import android.content.Context;
import android.os.Looper;
import android.text.Editable;
import android.text.Selection;
import android.text.SpannableStringBuilder;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.InputConnection;

/** Runs the service against Android's real composing-span and selection behavior. */
public final class CursorSmoke {
    private static final class Service extends TacitIme {
        InputConnection connection;

        @Override
        public InputConnection getCurrentInputConnection() {
            return connection;
        }
    }

    private static void set(Service service, String name, Object value) throws Exception {
        java.lang.reflect.Field field = TacitIme.class.getDeclaredField(name);
        field.setAccessible(true);
        field.set(service, value);
    }

    private static void type(Service service, String keys) {
        for (char key : keys.toCharArray()) {
            int code = key == ' ' ? KeyEvent.KEYCODE_SPACE : KeyEvent.KEYCODE_A + key - 'a';
            service.onKeyDown(code, new KeyEvent(KeyEvent.ACTION_DOWN, code));
        }
    }

    private static void check(Context context, String dir, String prefix, String keys,
            int start, int end, boolean editorFinished) throws Exception {
        long handle = Native.nativeCreate(dir);
        if (handle == 0) throw new AssertionError("nativeCreate returned null");
        try {
            Native.nativeActivate(handle, "cursor-smoke");
            Editable text = new SpannableStringBuilder(prefix);
            Selection.setSelection(text, text.length());
            Service service = new Service();
            BaseInputConnection connection = new BaseInputConnection(new View(context), true) {
                @Override public Editable getEditable() { return text; }
            };
            service.connection = connection;
            set(service, "ime", handle);
            set(service, "active", true);
            type(service, keys);
            int old = text.length();
            String original = text.toString();
            int composingStart = BaseInputConnection.getComposingSpanStart(text);
            int composingEnd = BaseInputConnection.getComposingSpanEnd(text);
            service.onUpdateSelection(old - 1, old - 1, old, old, composingStart, composingEnd);
            if (Native.nativeReadings(handle).isEmpty()) {
                throw new AssertionError("normal typing ended the composition");
            }
            connection.setSelection(start, end);
            if (editorFinished) connection.finishComposingText();
            service.onUpdateSelection(old, old, start, end,
                    BaseInputConnection.getComposingSpanStart(text),
                    BaseInputConnection.getComposingSpanEnd(text));
            boolean retained = !Native.nativeReadings(handle).isEmpty();
            type(service, "hello ");
            String expected = original.substring(0, start) + "hello " + original.substring(end);
            if (!text.toString().equals(expected)) {
                throw new AssertionError("cursor " + start + ": expected " + expected + ", got " + text);
            }
            if (retained) throw new AssertionError("cursor move retained the old word");
        } finally {
            Native.nativeDestroy(handle);
        }
    }

    public static void main(String[] args) throws Exception {
        Thread.setDefaultUncaughtExceptionHandler((thread, error) -> {
            error.printStackTrace();
            System.exit(1);
        });
        Looper.prepareMainLooper();
        Class<?> thread = Class.forName("android.app.ActivityThread");
        Object activityThread = thread.getMethod("systemMain").invoke(null);
        Context context = (Context) thread.getMethod("getSystemContext").invoke(activityThread);
        check(context, args[0], "메모: ", "dkssud", 0, 0, false);
        check(context, args[0], "메모: ", "dkssud", 5, 5, false);
        check(context, args[0], "메모: ", "dkssud", 0, 2, false);
        check(context, args[0], "hello ", "hello", 5, 5, false);
        check(context, args[0], "hello ", "hello", 5, 5, true);
        System.out.println("PASS: Android service cursor moves/selection/repeated text/normal typing");
    }
}
