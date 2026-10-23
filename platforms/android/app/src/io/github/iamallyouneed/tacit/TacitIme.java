package io.github.iamallyouneed.tacit;

import android.inputmethodservice.InputMethodService;
import android.inputmethodservice.Keyboard;
import android.inputmethodservice.KeyboardView;
import android.text.InputType;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.KeyCharacterMap;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.LinearLayout;
import android.widget.TextView;

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
    private static final int CODE_MODE = -101;
    private static final int CODE_SYMBOLS = -102;
    private static final int CODE_LETTERS = -103;

    private long ime;
    private boolean active;
    private boolean composing;
    /** Keys whose down event was consumed: their up event is too. */
    private final Set<Integer> swallowed = new HashSet<>();

    private DualKeyboardView keyboardView;
    private LinearLayout readingsView;
    /** What the chips were built from, so refresh() rebuilds them only when it changes. */
    private String shownReadings;
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
        View root = getLayoutInflater().inflate(R.layout.input, null);
        readingsView = root.findViewById(R.id.readings);
        keyboardView = root.findViewById(R.id.keyboard);
        keyboardView.setOnKeyboardActionListener(this);
        keyboardView.setPreviewEnabled(false);
        keyboardView.setKeyboard(letters);

        return root;
    }

    @Override
    public void onStartInput(EditorInfo info, boolean restarting) {
        super.onStartInput(info, restarting);
        active = ime != 0 && isTextField(info);
        composing = false;
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
        composing = false;
        InputConnection ic = getCurrentInputConnection();
        if (ic != null) ic.finishComposingText();
        if (ime != 0) Native.nativeReset(ime);
        super.onFinishInput();
    }

    @Override
    public void onUpdateSelection(int oldSelStart, int oldSelEnd, int newSelStart, int newSelEnd,
            int candidatesStart, int candidatesEnd) {
        super.onUpdateSelection(oldSelStart, oldSelEnd, newSelStart, newSelEnd,
                candidatesStart, candidatesEnd);
        if (!active || !composing) return;
        if (newSelStart == candidatesEnd && newSelEnd == candidatesEnd) return;
        // A moved caret must detach both the editor's composing span and Rust's old word.
        composing = false;
        Native.nativeReset(ime);
        InputConnection ic = getCurrentInputConnection();
        if (ic != null) ic.finishComposingText();
        refresh();
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
            apply(ic, Native.nativeCycleMode(ime, before(ic)));
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
            case CODE_MODE:
                InputConnection ic = getCurrentInputConnection();
                if (ic != null && active) apply(ic, Native.nativeCycleMode(ime, before(ic)));
                refresh();
                return;
            case Keyboard.KEYCODE_DELETE:
                soft(Native.DELETE, 0, KeyEvent.KEYCODE_DEL);
                return;
            case 10:
                soft(Native.ENTER, 0, KeyEvent.KEYCODE_ENTER);
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

    /** The mode key shows the mode; the strip above the keys shows the word's readings. */
    private void refresh() {
        if (keyboardView == null || ime == 0) return;
        int mode = active ? Native.nativeMode(ime) : Native.MODE_AUTO;
        String label = modeLabel(mode);
        for (Keyboard board : new Keyboard[] {letters, symbols}) {
            for (Keyboard.Key key : board.getKeys()) {
                if (key.codes[0] == CODE_MODE) key.label = label;
            }
        }
        keyboardView.setMode(mode);
        showReadings();
    }

    private static String modeLabel(int mode) {
        switch (mode) {
            case Native.MODE_KOREAN:
                return "한";
            case Native.MODE_ENGLISH:
                return "영";
            default:
                return "tacit";
        }
    }

    private void showReadings() {
        String readings = active ? Native.nativeReadings(ime) : "";
        if (readings.equals(shownReadings)) return; // same chips: no rebuild per key
        shownReadings = readings;
        readingsView.removeAllViews();
        if (!active) return;
        String[] parts = readings.split("\u0001");
        float density = getResources().getDisplayMetrics().density;
        for (int i = 1; i < parts.length; i++) {
            final int index = i - 1;
            TextView chip = new TextView(this);
            chip.setText(parts[i].substring(1));
            chip.setSelected(parts[i].charAt(0) == '1');
            chip.setTextColor(getColor(R.color.keyboard_text));
            chip.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18);
            chip.setGravity(Gravity.CENTER);
            chip.setBackgroundResource(R.drawable.reading_chip);
            chip.setPadding((int) (14 * density), 0, (int) (14 * density), 0);
            chip.setOnClickListener(v -> chooseReading(index));
            readingsView.addView(chip, new LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.MATCH_PARENT));
        }
    }

    private void chooseReading(int index) {
        InputConnection ic = getCurrentInputConnection();
        if (ic == null || !active) return;
        apply(ic, Native.nativeChooseReading(ime, index, before(ic)));
        refresh();
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
    private boolean apply(InputConnection ic, String reply) {
        String[] parts = reply.split("\u0001", -1);
        if (parts.length > 1) {
            ic.beginBatchEdit();
            for (int i = 1; i < parts.length; i++) {
                String rest = parts[i].substring(1);
                switch (parts[i].charAt(0)) {
                    case 'M':
                        composing = !rest.isEmpty();
                        ic.setComposingText(rest, 1);
                        break;
                    case 'I':
                        composing = false;
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
