package io.github.iamallyouneed.tacit;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.inputmethodservice.Keyboard;
import android.inputmethodservice.KeyboardView;

/** The framework's keyboard view, plus the 2-set Hangul jamo of each letter key in its corner. */
@SuppressWarnings("deprecation")
public class DualKeyboardView extends KeyboardView {
    private static final String LETTERS = "qwertyuiopasdfghjklzxcvbnm";
    private static final String JAMO = "ㅂㅈㄷㄱㅅㅛㅕㅑㅐㅔㅁㄴㅇㄹㅎㅗㅓㅏㅣㅋㅌㅊㅍㅠㅜㅡ";
    /** What Shift gives: the letters without a shifted jamo keep theirs. */
    private static final String JAMO_SHIFTED = "ㅃㅉㄸㄲㅆㅛㅕㅑㅒㅖㅁㄴㅇㄹㅎㅗㅓㅏㅣㅋㅌㅊㅍㅠㅜㅡ";

    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);

    public DualKeyboardView(Context context) {
        super(context, null);
        float density = getResources().getDisplayMetrics().density;
        paint.setTextSize(11 * density);
        paint.setColor(Color.rgb(0x9E, 0xC8, 0xFF));
        paint.setTextAlign(Paint.Align.RIGHT);
    }

    @Override
    public void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        Keyboard board = getKeyboard();
        if (board == null) return;
        float density = getResources().getDisplayMetrics().density;
        String jamo = board.isShifted() ? JAMO_SHIFTED : JAMO;
        for (Keyboard.Key key : board.getKeys()) {
            int at = key.codes[0] < 128 ? LETTERS.indexOf((char) key.codes[0]) : -1;
            if (at < 0) continue;
            float x = getPaddingLeft() + key.x + key.width - 5 * density;
            float y = getPaddingTop() + key.y + 15 * density;
            canvas.drawText(String.valueOf(jamo.charAt(at)), x, y, paint);
        }
    }
}
