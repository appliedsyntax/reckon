package local.reckon;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.text.Editable;
import android.text.InputType;
import android.text.Layout;
import android.text.Spanned;
import android.text.style.ForegroundColorSpan;
import android.text.style.StyleSpan;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.inputmethod.EditorInfo;
import android.widget.EditText;

/**
 * Multi-line editor that draws each line's result in a column on the right,
 * on the same baseline as the line it belongs to, and paints the current
 * theme: highlight colours, the results column, the theme's texture and the
 * active line.
 */
final class CalcEditText extends EditText {
    interface OnResultTap {
        void onTap(String result);
    }

    /** Our highlight spans, so they can be removed without touching the IME's. */
    private static final class Hl extends ForegroundColorSpan {
        Hl(int color) { super(color); }
    }

    private static final class Bold extends StyleSpan {
        Bold() { super(Typeface.BOLD); }
    }

    private String[] results = new String[0];
    private String[] spans = new String[0];
    private ReckonTheme theme;
    private final Paint resultPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint fill = new Paint();
    private final float resultsWidth;
    private final float gap;
    private final float dp;
    private OnResultTap onResultTap;

    CalcEditText(Context context) {
        super(context);
        dp = getResources().getDisplayMetrics().density;
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
        // Every line is the same height, so results, rules and the grid line up.
        setLineHeight((int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, 28,
                getResources().getDisplayMetrics()));
        int pad = (int) (14 * dp);
        setPadding(pad, pad, (int) (resultsWidth + gap * 2), pad);

        resultPaint.setTextSize(getTextSize());
        resultPaint.setTextAlign(Paint.Align.RIGHT);
    }

    void setTheme(ReckonTheme t) {
        theme = t;
        setTextColor(t.color("text"));
        setHighlightColor(t.color("selection"));
        resultPaint.setColor(t.color("result"));
        // The system monospace font has regular and bold only, so 600+ is bold.
        resultPaint.setTypeface(Typeface.create(Typeface.MONOSPACE,
                t.resultWeight >= 600 ? Typeface.BOLD : Typeface.NORMAL));
        // Phosphor gets a translucent block cursor as wide as a character.
        GradientDrawable cursor = new GradientDrawable();
        cursor.setColor(t.color("cursor"));
        float charWidth = getPaint().measureText("0");
        cursor.setSize(t.blockCursor ? Math.round(charWidth) : Math.round(2 * dp), getLineHeight());
        setTextCursorDrawable(cursor);
        applyHighlights();
        invalidate();
    }

    /** Results and highlights for each line, from Native.evaluateHighlighted. */
    void setOutput(String[] results, String[] spans) {
        this.results = results;
        this.spans = spans;
        applyHighlights();
        invalidate();
    }

    void setOnResultTap(OnResultTap l) {
        onResultTap = l;
    }

    /** Re-colours the document from `spans` ("start,len,kind;…" per line). */
    private void applyHighlights() {
        Editable text = getText();
        if (theme == null || text == null) return;
        for (Hl s : text.getSpans(0, text.length(), Hl.class)) text.removeSpan(s);
        for (Bold s : text.getSpans(0, text.length(), Bold.class)) text.removeSpan(s);
        int lineStart = 0;
        for (int line = 0; line < spans.length && lineStart <= text.length(); line++) {
            if (!spans[line].isEmpty()) {
                for (String span : spans[line].split(";")) {
                    String[] f = span.split(",");
                    if (f.length != 3) continue;
                    int start = lineStart + Integer.parseInt(f[0]);
                    int end = start + Integer.parseInt(f[1]);
                    if (end > text.length()) continue;
                    String kind = f[2].equals("error") ? "muted" : f[2];
                    text.setSpan(new Hl(theme.color(kind)), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
                    if (kind.equals("header")) {
                        text.setSpan(new Bold(), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
                    }
                }
            }
            int nl = indexOf(text, '\n', lineStart);
            if (nl < 0) break;
            lineStart = nl + 1;
        }
    }

    private static int indexOf(CharSequence s, char c, int from) {
        for (int i = from; i < s.length(); i++) if (s.charAt(i) == c) return i;
        return -1;
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
    protected void onSelectionChanged(int selStart, int selEnd) {
        super.onSelectionChanged(selStart, selEnd);
        invalidate();  // moves the active-line highlight
    }

    @Override
    protected void onDraw(Canvas canvas) {
        Layout layout = getLayout();
        if (theme != null && layout != null) drawBackdrop(canvas, layout);
        super.onDraw(canvas);
        if (layout == null) return;
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

    /** The results column, the theme's texture, the active line and the divider. */
    private void drawBackdrop(Canvas canvas, Layout layout) {
        float left = getScrollX(), right = left + getWidth();
        float top = getScrollY(), bottom = top + getHeight();
        float dividerX = columnRight() - resultsWidth - gap / 2;
        float resultsLeft = dividerX + (theme.doubleDivider ? 3 * dp : 0);
        float hair = Math.max(1f, dp / 2);
        float textTop = getTotalPaddingTop();
        float lh = getLineHeight();

        fill.setColor(theme.color("results_bg"));
        canvas.drawRect(resultsLeft, top, right, bottom, fill);

        int rule = theme.color("rule");
        int minor = (rule & 0x00ffffff) | (Math.round(((rule >>> 24) & 0xff) * 0.55f) << 24);
        switch (theme.pattern) {
            case "grid": {
                float step = lh / 2;
                int k = (int) Math.floor((top - textTop) / step);
                for (float y = textTop + k * step; y <= bottom; y += step, k++) {
                    fill.setColor(k % 2 == 0 ? rule : minor);
                    canvas.drawRect(left, y, right, y + hair, fill);
                }
                int j = 0;
                for (float x = getTotalPaddingLeft(); x < right; x += step, j++) {
                    fill.setColor(j % 2 == 0 ? rule : minor);
                    canvas.drawRect(x, top, x + hair, bottom, fill);
                }
                break;
            }
            case "ruled": {
                // A rule under every line, continuing down the empty page.
                fill.setColor(rule);
                int k = Math.max(0, (int) Math.floor((top - textTop) / lh));
                for (float y = textTop + (k + 1) * lh - hair; y <= bottom; y += lh) {
                    canvas.drawRect(left, y, right, y + hair, fill);
                }
                break;
            }
            case "scanlines": {
                fill.setColor(rule);
                float step = 3 * dp / 2;
                for (float y = (float) Math.floor(top / step) * step; y < bottom; y += step) {
                    canvas.drawRect(left, y, right, y + hair, fill);
                }
                break;
            }
            default:
                break;
        }

        // Active line: every wrapped row of the logical line holding the cursor.
        int sel = getSelectionStart();
        if (hasFocus() && sel >= 0 && sel == getSelectionEnd()) {
            CharSequence text = getText();
            int start = sel, end = sel;
            while (start > 0 && text.charAt(start - 1) != '\n') start--;
            while (end < text.length() && text.charAt(end) != '\n') end++;
            float y0 = layout.getLineTop(layout.getLineForOffset(start)) + textTop;
            float y1 = layout.getLineBottom(layout.getLineForOffset(end)) + textTop;
            fill.setColor(theme.color("active_line"));
            canvas.drawRect(left, y0, right, y1, fill);
        }

        fill.setColor(theme.color("divider"));
        canvas.drawRect(dividerX, top, dividerX + hair, bottom, fill);
        if (theme.doubleDivider) canvas.drawRect(dividerX + 3 * dp, top, dividerX + 3 * dp + hair, bottom, fill);
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
