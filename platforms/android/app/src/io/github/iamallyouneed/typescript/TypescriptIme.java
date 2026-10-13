package io.github.iamallyouneed.typescript;

import android.inputmethodservice.InputMethodService;
import android.text.InputType;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

import java.util.HashSet;
import java.util.Set;

/** Hardware-keyboard input for now: keys go to Rust, its edit commands come back here. */
public class TypescriptIme extends InputMethodService {
    private static final int BEFORE_CARET = 64;

    private long ime;
    private boolean active;
    /** Keys whose down event was consumed: their up event is too. */
    private final Set<Integer> swallowed = new HashSet<>();

    @Override
    public void onCreate() {
        super.onCreate();
        ime = Native.nativeCreate(getFilesDir().getAbsolutePath());
    }

    @Override
    public void onDestroy() {
        Native.nativeDestroy(ime);
        ime = 0;
        super.onDestroy();
    }

    @Override
    public void onStartInput(EditorInfo info, boolean restarting) {
        super.onStartInput(info, restarting);
        active = ime != 0 && isTextField(info);
        swallowed.clear();
        if (active) Native.nativeActivate(ime, info.packageName);
    }

    @Override
    public void onFinishInput() {
        InputConnection ic = getCurrentInputConnection();
        if (ic != null) ic.finishComposingText();
        if (ime != 0) Native.nativeReset(ime);
        super.onFinishInput();
    }

    /** Passwords and fields that are not text are left to the system. */
    private static boolean isTextField(EditorInfo info) {
        int type = info.inputType;
        if ((type & InputType.TYPE_MASK_CLASS) != InputType.TYPE_CLASS_TEXT) return false;
        switch (type & InputType.TYPE_MASK_VARIATION) {
            case InputType.TYPE_TEXT_VARIATION_PASSWORD:
            case InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD:
            case InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD:
                return false;
            default:
                return true;
        }
    }

    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        InputConnection ic = getCurrentInputConnection();
        if (!active || ic == null || isModifier(keyCode)) return super.onKeyDown(keyCode, event);

        int kind = Native.OTHER;
        int ch = 0;
        String reply;
        if (keyCode == KeyEvent.KEYCODE_LANGUAGE_SWITCH) {
            reply = Native.nativeTogglePause(ime, before(ic));
        } else {
            kind = kindOf(keyCode, event);
            ch = charOf(keyCode, event, kind);
            reply = Native.nativeKey(ime, kind, ch, before(ic));
        }
        boolean consumed = apply(ic, reply);
        // A word just went in and the app is to get the key: the app would act on it before our
        // edit arrives (and put the caret before it), so the key follows the word on the same
        // connection instead.
        if (!consumed && reply.indexOf('\u0001') >= 0) {
            if (kind == Native.PRINTABLE) {
                ic.commitText(new String(Character.toChars(ch)), 1);
            } else {
                sendDownUpKeyEvents(keyCode);
            }
            consumed = true;
        }
        if (consumed) {
            swallowed.add(keyCode);
            return true;
        }
        return super.onKeyDown(keyCode, event);
    }

    @Override
    public boolean onKeyUp(int keyCode, KeyEvent event) {
        if (swallowed.remove(keyCode)) return true;
        return super.onKeyUp(keyCode, event);
    }

    private static boolean isModifier(int keyCode) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_SHIFT_LEFT:
            case KeyEvent.KEYCODE_SHIFT_RIGHT:
            case KeyEvent.KEYCODE_CTRL_LEFT:
            case KeyEvent.KEYCODE_CTRL_RIGHT:
            case KeyEvent.KEYCODE_ALT_LEFT:
            case KeyEvent.KEYCODE_ALT_RIGHT:
            case KeyEvent.KEYCODE_META_LEFT:
            case KeyEvent.KEYCODE_META_RIGHT:
            case KeyEvent.KEYCODE_CAPS_LOCK:
            case KeyEvent.KEYCODE_NUM_LOCK:
                return true;
            default:
                return false;
        }
    }

    private static int kindOf(int keyCode, KeyEvent event) {
        if (event.isCtrlPressed() || event.isAltPressed() || event.isMetaPressed()) {
            return Native.OTHER; // a shortcut: the word goes in, the app gets the key
        }
        if (keyCode >= KeyEvent.KEYCODE_A && keyCode <= KeyEvent.KEYCODE_Z) return Native.LETTER;
        switch (keyCode) {
            case KeyEvent.KEYCODE_DEL:
                return Native.DELETE;
            case KeyEvent.KEYCODE_ESCAPE:
                return event.isShiftPressed() ? Native.CYCLE : Native.ESCAPE;
            case KeyEvent.KEYCODE_ENTER:
            case KeyEvent.KEYCODE_NUMPAD_ENTER:
                return Native.ENTER;
            case KeyEvent.KEYCODE_TAB:
                return event.isShiftPressed() ? Native.OTHER : Native.TAB;
            default:
                int c = event.getUnicodeChar();
                boolean text = c >= 32 && (c & KeyCharacterMap.COMBINING_ACCENT) == 0;
                return text ? Native.PRINTABLE : Native.OTHER;
        }
    }

    private static int charOf(int keyCode, KeyEvent event, int kind) {
        if (kind == Native.LETTER) {
            char c = (char) ('a' + keyCode - KeyEvent.KEYCODE_A);
            return event.isShiftPressed() != event.isCapsLockOn() ? Character.toUpperCase(c) : c;
        }
        return kind == Native.PRINTABLE ? event.getUnicodeChar() : 0;
    }

    private static String before(InputConnection ic) {
        CharSequence text = ic.getTextBeforeCursor(BEFORE_CARET, 0);
        return text == null ? "" : text.toString();
    }

    /** Applies Rust's edit commands; true if the key was consumed. */
    private static boolean apply(InputConnection ic, String reply) {
        String[] parts = reply.split("\u0001", -1);
        if (parts.length > 1) {
            ic.beginBatchEdit();
            for (int i = 1; i < parts.length; i++) {
                String rest = parts[i].substring(1);
                switch (parts[i].charAt(0)) {
                    case 'M':
                        ic.setComposingText(rest, 1);
                        break;
                    case 'I':
                        ic.commitText(rest, 1);
                        break;
                    case 'B':
                        ic.deleteSurroundingText(Integer.parseInt(rest), 0);
                        break;
                    default:
                        break;
                }
            }
            ic.endBatchEdit();
        }
        return parts[0].equals("1");
    }
}
