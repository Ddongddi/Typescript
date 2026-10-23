/*
 * Hint positioning adapted from HeliBoard KeyboardView (v4.2).
 * Copyright (C) 2010 The Android Open Source Project
 * Modified by the HeliBoard contributors and Tacit contributors.
 * SPDX-License-Identifier: Apache-2.0 AND GPL-3.0-only
 */
package io.github.iamallyouneed.tacit;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.util.AttributeSet;
import android.inputmethodservice.Keyboard;
import android.inputmethodservice.KeyboardView;

/**
 * The framework's keyboard view. tacit mode: letters with the 2-set Hangul jamo of each key in
 * its corner. Korean mode: the jamo are the keys. English mode: plain letters.
 */
@SuppressWarnings("deprecation")
public class DualKeyboardView extends KeyboardView {
    private static final String LETTERS = "qwertyuiopasdfghjklzxcvbnm";
    private static final String JAMO = "ㅂㅈㄷㄱㅅㅛㅕㅑㅐㅔㅁㄴㅇㄹㅎㅗㅓㅏㅣㅋㅌㅊㅍㅠㅜㅡ";
    /** What Shift gives: the letters without a shifted jamo keep theirs. */
    private static final String JAMO_SHIFTED = "ㅃㅉㄸㄲㅆㅛㅕㅑㅒㅖㅁㄴㅇㄹㅎㅗㅓㅏㅣㅋㅌㅊㅍㅠㅜㅡ";

    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private int mode = Native.MODE_AUTO;

    public DualKeyboardView(Context context, AttributeSet attrs) {
        super(context, attrs);
        paint.setTextSize(10 * getResources().getDisplayMetrics().scaledDensity);
        paint.setColor(context.getColor(R.color.keyboard_hint));
        paint.setTextAlign(Paint.Align.CENTER);
    }

    /** One of Native.MODE_*. */
    public void setMode(int mode) {
        if (this.mode == mode) return; // refresh() runs on every key: redraw only on a change
        this.mode = mode;
        invalidateAllKeys();
    }

    private static int letterIndex(Keyboard.Key key) {
        return key.codes[0] < 128 ? LETTERS.indexOf((char) key.codes[0]) : -1;
    }

    /**
     * The framework draws key.label (and upper-cases a letter when shifted): the jamo in Korean
     * mode, the letter otherwise. Set here, not in onDraw: invalidateKey draws without it.
     */
    @Override
    public void invalidateAllKeys() {
        Keyboard board = getKeyboard();
        if (board != null) {
            String jamo = board.isShifted() ? JAMO_SHIFTED : JAMO;
            for (Keyboard.Key key : board.getKeys()) {
                int at = letterIndex(key);
                if (at < 0) continue;
                key.label = String.valueOf(mode == Native.MODE_KOREAN ? jamo.charAt(at) : LETTERS.charAt(at));
            }
        }
        super.invalidateAllKeys();
    }

    @Override
    public void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        Keyboard board = getKeyboard();
        if (board == null || mode != Native.MODE_AUTO) return;
        String jamo = board.isShifted() ? JAMO_SHIFTED : JAMO;
        float density = getResources().getDisplayMetrics().density;
        for (Keyboard.Key key : board.getKeys()) {
            int at = letterIndex(key);
            if (at < 0) continue;
            String hint = String.valueOf(jamo.charAt(at));
            // HeliBoard's phone hint: keep the entire glyph inside the top-right corner.
            float width = Math.max(paint.measureText("0"), paint.measureText(hint));
            float x = getPaddingLeft() + key.x + key.width - 5 * density - width / 2;
            float y = getPaddingTop() + key.y + 5 * density - paint.ascent();
            canvas.drawText(hint, x, y, paint);
        }
    }
}
