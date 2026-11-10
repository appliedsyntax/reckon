package local.reckon;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Typeface;
import android.text.InputType;
import android.text.Layout;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.inputmethod.EditorInfo;
import android.widget.EditText;

/**
 * Multi-line editor that draws each line's result in a column on the right,
 * on the same baseline as the line it belongs to.
 */
final class CalcEditText extends EditText {
    interface OnResultTap {
        void onTap(String result);
    }

    private String[] results = new String[0];
    private final Paint resultPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint dividerPaint = new Paint();
    private final float resultsWidth;
    private final float gap;
    private OnResultTap onResultTap;

    CalcEditText(Context context) {
        super(context);
        float dp = getResources().getDisplayMetrics().density;
        resultsWidth = 130 * dp;
        gap = 14 * dp;
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 16);
        setTypeface(Typeface.MONOSPACE);
        setGravity(Gravity.TOP | Gravity.START);
        setBackground(null);
        // No autocorrect or suggestions: they fight with math input.
        setInputType(InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        // Keep the results visible while typing (no full-screen landscape keyboard).
        setImeOptions(EditorInfo.IME_FLAG_NO_EXTRACT_UI | EditorInfo.IME_FLAG_NO_FULLSCREEN);
        setLineSpacing(4 * dp, 1f);
        int pad = (int) (14 * dp);
        setPadding(pad, pad, (int) (resultsWidth + gap * 2), pad);

        resultPaint.setTextSize(getTextSize());
        resultPaint.setTypeface(Typeface.create(Typeface.MONOSPACE, Typeface.BOLD));
        resultPaint.setTextAlign(Paint.Align.RIGHT);
        dividerPaint.setStrokeWidth(Math.max(1f, dp / 2));
    }

    void setResultColors(int result, int divider) {
        resultPaint.setColor(result);
        dividerPaint.setColor(divider);
        invalidate();
    }

    void setResults(String[] r) {
        results = r;
        invalidate();
    }

    void setOnResultTap(OnResultTap l) {
        onResultTap = l;
    }

    /** Character offset where logical line `index` starts, or -1. */
    private int lineStart(CharSequence text, int index) {
        int line = 0;
        if (index == 0) return 0;
        for (int i = 0; i < text.length(); i++) {
            if (text.charAt(i) == '\n' && ++line == index) return i + 1;
        }
        return -1;
    }

    private float columnRight() {
        return getScrollX() + getWidth() - gap;
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        Layout layout = getLayout();
        if (layout == null) return;
        float dividerX = columnRight() - resultsWidth - gap / 2;
        float bottom = Math.max(getHeight(), layout.getHeight() + getTotalPaddingTop() + getTotalPaddingBottom());
        canvas.drawLine(dividerX, getScrollY(), dividerX, getScrollY() + bottom, dividerPaint);
        // Clip results to the same area TextView draws its text in.
        canvas.save();
        canvas.clipRect(getScrollX(), getScrollY() + getExtendedPaddingTop(),
                getScrollX() + getWidth(), getScrollY() + getHeight() - getExtendedPaddingBottom());
        CharSequence text = getText();
        for (int i = 0; i < results.length; i++) {
            String r = results[i];
            if (r == null || r.isEmpty()) continue;
            int start = lineStart(text, i);
            if (start < 0) break;
            int line = layout.getLineForOffset(start);
            float y = layout.getLineBaseline(line) + getTotalPaddingTop();
            String shown = r;
            if (resultPaint.measureText(shown) > resultsWidth) {
                int n = resultPaint.breakText(shown, true, resultsWidth - resultPaint.measureText("…"), null);
                shown = shown.substring(0, Math.max(0, n)) + "…";
            }
            canvas.drawText(shown, columnRight(), y, resultPaint);
        }
        canvas.restore();
    }

    @Override
    public boolean onTouchEvent(MotionEvent e) {
        Layout layout = getLayout();
        float x = e.getX() + getScrollX();
        if (layout != null && onResultTap != null && x > columnRight() - resultsWidth) {
            if (e.getAction() == MotionEvent.ACTION_UP) {
                int line = layout.getLineForVertical((int) (e.getY() + getScrollY() - getTotalPaddingTop()));
                int offset = layout.getLineStart(line);
                CharSequence text = getText();
                int logical = 0;
                for (int i = 0; i < offset && i < text.length(); i++) {
                    if (text.charAt(i) == '\n') logical++;
                }
                if (logical < results.length && results[logical] != null && !results[logical].isEmpty()) {
                    onResultTap.onTap(results[logical]);
                }
            }
            return true;
        }
        return super.onTouchEvent(e);
    }
}
