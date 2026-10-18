package io.github.iamallyouneed.tacit;

import android.inputmethodservice.InputMethodService;
import android.inputmethodservice.Keyboard;
import android.inputmethodservice.KeyboardView;
import android.text.InputType;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

import java.util.HashSet;
import java.util.Set;

/**
 * Input from a hardware keyboard and from the on-screen one: keys go to Rust, its edit
 * commands come back here. The on-screen keyboard is the framework's (deprecated, but in every
 * Android version since 1.5 and not a dependency).
 */
@SuppressWarnings("deprecation")
public class TacitIme extends InputMethodService implements KeyboardView.OnKeyboardActionListener {
    private static final int BEFORE_CARET = 64;
    private static final int CODE_CYCLE = -100;
    private static final int CODE_PAUSE = -101;
    private static final int CODE_SYMBOLS = -102;
    private static final int CODE_LETTERS = -103;

    private long ime;
    private boolean active;
    /** Keys whose down event was consumed: their up event is too. */
    private final Set<Integer> swallowed = new HashSet<>();

    private KeyboardView keyboardView;
    private Keyboard letters;
    private Keyboard symbols;

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
    public View onCreateInputView() {
        letters = new Keyboard(this, R.xml.qwerty);
        symbols = new Keyboard(this, R.xml.symbols);
        keyboardView = new DualKeyboardView(this);
        keyboardView.setOnKeyboardActionListener(this);
        keyboardView.setPreviewEnabled(false);
        keyboardView.setKeyboard(letters);

        return keyboardView;
    }

    @Override
    public void onStartInput(EditorInfo info, boolean restarting) {
        super.onStartInput(info, restarting);
        active = ime != 0 && isTextField(info);
        swallowed.clear();
        if (active) Native.nativeActivate(ime, info.packageName);
    }

    @Override
    public void onStartInputView(EditorInfo info, boolean restarting) {
        super.onStartInputView(info, restarting);
        if (keyboardView != null) {
            letters.setShifted(false);
            keyboardView.setKeyboard(letters);
        }
        refresh();
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

    // MARK: - Hardware keyboard

    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        InputConnection ic = getCurrentInputConnection();
        if (!active || ic == null || isModifier(keyCode)) return super.onKeyDown(keyCode, event);

        boolean consumed;
        if (keyCode == KeyEvent.KEYCODE_LANGUAGE_SWITCH) {
            apply(ic, Native.nativeTogglePause(ime, before(ic)));
            consumed = true;
        } else {
            int kind = kindOf(keyCode, event);
            consumed = send(ic, kind, charOf(keyCode, event, kind), keyCode, false);
        }
        refresh();
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

    // MARK: - On-screen keyboard

    @Override
    public void onKey(int code, int[] keyCodes) {
        switch (code) {
            case Keyboard.KEYCODE_SHIFT:
                letters.setShifted(!letters.isShifted());
                keyboardView.invalidateAllKeys();
                return;
            case CODE_SYMBOLS:
                keyboardView.setKeyboard(symbols);
                refresh();
                return;
            case CODE_LETTERS:
                keyboardView.setKeyboard(letters);
                refresh();
                return;
            case CODE_PAUSE:
                InputConnection ic = getCurrentInputConnection();
                if (ic != null && active) apply(ic, Native.nativeTogglePause(ime, before(ic)));
                refresh();
                return;
            case Keyboard.KEYCODE_DELETE:
                soft(Native.DELETE, 0, KeyEvent.KEYCODE_DEL);
                return;
            case 10:
                soft(Native.ENTER, 0, KeyEvent.KEYCODE_ENTER);
                return;
            case CODE_CYCLE:
                soft(Native.CYCLE, 0, 0);
                return;
            default:
                break;
        }
        char c = (char) code;
        if (Character.isLetter(c)) {
            boolean shifted = keyboardView.getKeyboard() == letters && letters.isShifted();
            soft(Native.LETTER, shifted ? Character.toUpperCase(c) : c, 0);
            if (shifted) {
                letters.setShifted(false); // one capital at a time
                keyboardView.invalidateAllKeys();
            }
        } else {
            soft(Native.PRINTABLE, c, 0);
        }
    }

    private void soft(int kind, int ch, int keyCode) {
        InputConnection ic = getCurrentInputConnection();
        if (ic == null) return;
        if (active) {
            send(ic, kind, ch, keyCode, true);
        } else {
            passThrough(ic, kind, ch, keyCode); // passwords and the like: no input method logic
        }
        refresh();
    }

    @Override
    public void onPress(int primaryCode) {}

    @Override
    public void onRelease(int primaryCode) {}

    @Override
    public void onText(CharSequence text) {}

    @Override
    public void swipeLeft() {}

    @Override
    public void swipeRight() {}

    @Override
    public void swipeDown() {}

    @Override
    public void swipeUp() {}

    /** The Han/Eng key shows whether tacit is paused. */
    private void refresh() {
        if (keyboardView == null || ime == 0) return;
        String label = active && Native.nativePaused(ime) ? "OFF" : "한/영";
        for (Keyboard board : new Keyboard[] {letters, symbols}) {
            for (Keyboard.Key key : board.getKeys()) {
                if (key.codes[0] == CODE_PAUSE) key.label = label;
            }
        }
        keyboardView.invalidateAllKeys();
    }

    // MARK: - Shared

    private static String before(InputConnection ic) {
        CharSequence text = ic.getTextBeforeCursor(BEFORE_CARET, 0);
        return text == null ? "" : text.toString();
    }

    /**
     * One key through Rust. True if it was handled here; false (hardware keys only) when the app
     * is to handle it itself.
     */
    private boolean send(InputConnection ic, int kind, int ch, int keyCode, boolean soft) {
        String reply = Native.nativeKey(ime, kind, ch, before(ic));
        if (apply(ic, reply)) return true;
        // A word just went in and the app is to get the key: the app would act on it before our
        // edit arrives (and put the caret before it), so the key follows the word on the same
        // connection instead.
        boolean wordWentIn = reply.indexOf('\u0001') >= 0;
        if (!soft && !wordWentIn) return false;
        passThrough(ic, kind, ch, keyCode);
        return true;
    }

    /** What the key does when the input method has no use for it. */
    private void passThrough(InputConnection ic, int kind, int ch, int keyCode) {
        switch (kind) {
            case Native.LETTER:
            case Native.PRINTABLE:
                ic.commitText(new String(Character.toChars(ch)), 1);
                break;
            case Native.ENTER:
                enter(ic);
                break;
            default:
                if (keyCode != 0) sendDownUpKeyEvents(keyCode);
                break;
        }
    }

    private void enter(InputConnection ic) {
        EditorInfo info = getCurrentInputEditorInfo();
        int action = info == null ? EditorInfo.IME_ACTION_NONE : info.imeOptions & EditorInfo.IME_MASK_ACTION;
        boolean plain = info == null
                || (info.imeOptions & EditorInfo.IME_FLAG_NO_ENTER_ACTION) != 0
                || action == EditorInfo.IME_ACTION_NONE
                || action == EditorInfo.IME_ACTION_UNSPECIFIED;
        if (plain) {
            sendDownUpKeyEvents(KeyEvent.KEYCODE_ENTER);
        } else {
            ic.performEditorAction(action);
        }
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
