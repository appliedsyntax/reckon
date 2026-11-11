package local.reckon;

import android.app.Dialog;
import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowManager;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.util.List;

/**
 * Bottom sheet listing the themes. Tapping one applies and saves it at once;
 * the sheet covers only the lower part of the screen and does not dim it, so
 * the document above shows each theme as it is chosen.
 */
final class ThemePicker {
    interface Listener {
        void onChanged();
        boolean systemDark();
    }

    private final Context ctx;
    private final ReckonTheme.Store store;
    private final Listener listener;
    private final float dp;
    private Dialog dialog;

    ThemePicker(Context ctx, ReckonTheme.Store store, Listener listener) {
        this.ctx = ctx;
        this.store = store;
        this.listener = listener;
        dp = ctx.getResources().getDisplayMetrics().density;
    }

    void show() {
        dialog = new Dialog(ctx);
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
        rebuild();
        Window w = dialog.getWindow();
        if (w != null) {
            w.setGravity(Gravity.BOTTOM);
            w.setLayout(ViewGroup.LayoutParams.MATCH_PARENT,
                    (int) (ctx.getResources().getDisplayMetrics().heightPixels * 0.55f));
            w.clearFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND);
            w.setBackgroundDrawable(new ColorDrawable(0));
        }
        dialog.show();
    }

    /** Redraws the sheet in the current theme (after every change). */
    private void rebuild() {
        ReckonTheme t = store.resolve(listener.systemDark());
        LinearLayout sheet = new LinearLayout(ctx);
        sheet.setOrientation(LinearLayout.VERTICAL);
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(t.color("surface"));
        float r = 18 * dp;
        bg.setCornerRadii(new float[]{r, r, r, r, 0, 0, 0, 0});
        bg.setStroke(Math.max(1, (int) dp), t.color("border"));
        sheet.setBackground(bg);
        sheet.setElevation(12 * dp);

        LinearLayout header = new LinearLayout(ctx);
        header.setGravity(Gravity.CENTER_VERTICAL);
        int pad = (int) (18 * dp);
        header.setPadding(pad, (int) (14 * dp), (int) (8 * dp), (int) (6 * dp));
        TextView title = text("Theme", 17, t.color("text"), true);
        header.addView(title, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1));
        TextView done = text("Done", 15, t.color("result"), true);
        done.setPadding(pad, (int) (8 * dp), pad, (int) (8 * dp));
        done.setOnClickListener(v -> dialog.dismiss());
        header.addView(done);
        sheet.addView(header);

        LinearLayout list = new LinearLayout(ctx);
        list.setOrientation(LinearLayout.VERTICAL);
        list.setPadding(0, 0, 0, (int) (24 * dp));
        list.addView(row(t, "Match System",
                store.light.name + " by day, " + store.dark.name + " by night",
                new ReckonTheme[]{store.light, store.dark}, store.followSystem,
                v -> { store.matchSystem(); changed(); }));
        List<ReckonTheme> all = ReckonTheme.all();
        boolean sawDark = false;
        list.addView(section(t, "LIGHT"));
        for (ReckonTheme theme : all) {
            if (theme.dark && !sawDark) {
                list.addView(section(t, "DARK"));
                sawDark = true;
            }
            boolean current = !store.followSystem && store.fixed == theme;
            list.addView(row(t, theme.name, theme.blurb, new ReckonTheme[]{theme}, current,
                    v -> { store.pick(theme); changed(); }));
        }
        ScrollView scroll = new ScrollView(ctx);
        scroll.addView(list);
        sheet.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1));

        // Keep the scroll position when redrawing after a tap.
        int y = 0;
        if (dialog.getWindow() != null && dialog.getWindow().getDecorView() != null) {
            View old = dialog.getWindow().getDecorView().findViewWithTag("themes");
            if (old instanceof ScrollView) y = old.getScrollY();
        }
        scroll.setTag("themes");
        final int keep = y;
        scroll.post(() -> scroll.scrollTo(0, keep));
        dialog.setContentView(sheet);
    }

    private void changed() {
        listener.onChanged();
        rebuild();
    }

    private TextView text(String s, int sp, int color, boolean bold) {
        TextView v = new TextView(ctx);
        v.setText(s);
        v.setTextSize(TypedValue.COMPLEX_UNIT_SP, sp);
        v.setTextColor(color);
        v.setSingleLine(true);
        v.setEllipsize(android.text.TextUtils.TruncateAt.END);
        if (bold) v.setTypeface(Typeface.create(Typeface.DEFAULT, 600, false));
        return v;
    }

    private View section(ReckonTheme t, String title) {
        TextView v = text(title, 12, t.color("muted"), true);
        v.setLetterSpacing(0.06f);
        v.setPadding((int) (22 * dp), (int) (16 * dp), 0, (int) (6 * dp));
        return v;
    }

    private View row(ReckonTheme t, String name, String blurb, ReckonTheme[] swatch,
                     boolean current, View.OnClickListener click) {
        LinearLayout row = new LinearLayout(ctx);
        row.setGravity(Gravity.CENTER_VERTICAL);
        int h = (int) (12 * dp);
        row.setPadding(h, (int) (10 * dp), h, (int) (10 * dp));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(current ? t.color("highlight") : 0);
        bg.setCornerRadius(10 * dp);
        row.setBackground(bg);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins((int) (10 * dp), 0, (int) (10 * dp), 0);
        row.setLayoutParams(lp);

        row.addView(new Swatch(swatch, t.color("border")),
                new LinearLayout.LayoutParams((int) (44 * dp), (int) (26 * dp)));
        LinearLayout labels = new LinearLayout(ctx);
        labels.setOrientation(LinearLayout.VERTICAL);
        labels.setPadding((int) (12 * dp), 0, (int) (8 * dp), 0);
        labels.addView(text(name, 16, t.color("text"), true));
        labels.addView(text(blurb, 12, t.color("muted"), false));
        row.addView(labels, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1));
        TextView check = text(current ? "✓" : "", 18, t.color("result"), true);
        row.addView(check);
        row.setOnClickListener(click);
        row.setContentDescription(name + (current ? ", selected" : ""));
        return row;
    }

    /** A theme's background with a number-coloured dot and a result-coloured bar. */
    private final class Swatch extends View {
        private final ReckonTheme[] halves;
        private final int border;
        private final Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);

        Swatch(ReckonTheme[] halves, int border) {
            super(ctx);
            this.halves = halves;
            this.border = border;
        }

        @Override
        protected void onDraw(Canvas c) {
            float w = getWidth(), h = getHeight(), r = 6 * dp;
            RectF box = new RectF(0, 0, w, h);
            c.save();
            android.graphics.Path clip = new android.graphics.Path();
            clip.addRoundRect(box, r, r, android.graphics.Path.Direction.CW);
            c.clipPath(clip);
            float part = w / halves.length;
            for (int i = 0; i < halves.length; i++) {
                ReckonTheme t = halves[i];
                float x0 = i * part, cx = x0 + part / 2, cy = h / 2;
                p.setStyle(Paint.Style.FILL);
                p.setColor(t.color("bg"));
                c.drawRect(x0, 0, x0 + part, h, p);
                p.setColor(t.color("number"));
                c.drawCircle(cx - 6 * dp, cy, 3 * dp, p);
                p.setColor(t.color("result"));
                c.drawRoundRect(new RectF(cx - 1.5f * dp, cy - 2.5f * dp, cx + 9 * dp, cy + 2.5f * dp),
                        2.5f * dp, 2.5f * dp, p);
            }
            c.restore();
            p.setStyle(Paint.Style.STROKE);
            p.setStrokeWidth(dp);
            p.setColor(border);
            c.drawRoundRect(new RectF(dp / 2, dp / 2, w - dp / 2, h - dp / 2), r, r, p);
        }
    }
}
