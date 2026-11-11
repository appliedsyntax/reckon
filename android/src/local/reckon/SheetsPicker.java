package local.reckon;

import android.app.Dialog;
import android.content.Context;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.text.Editable;
import android.text.TextUtils;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowManager;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Bottom sheet listing the sheets: search, open (tap), delete (long press,
 * with Undo in the status bar) and New.
 */
final class SheetsPicker {
    interface Host {
        File dir();
        String current();
        ReckonTheme theme();
        void open(String file);
        void createSheet();
        void delete(String file);
    }

    static final class Info {
        final String file, title, preview;

        Info(String file, String title, String preview) {
            this.file = file;
            this.title = title;
            this.preview = preview;
        }
    }

    static List<Info> list(File dir) {
        List<Info> out = new ArrayList<>();
        try {
            JSONArray a = new JSONArray(Native.str(Native.sheetsList(Native.utf8(dir.getAbsolutePath()))));
            for (int i = 0; i < a.length(); i++) {
                JSONObject o = a.getJSONObject(i);
                out.add(new Info(o.getString("file"), o.optString("title"), o.optString("preview")));
            }
        } catch (JSONException ignored) {
        }
        return out;
    }

    private final Context ctx;
    private final Host host;
    private final float dp;
    private Dialog dialog;
    private LinearLayout rows;
    private String query = "";

    SheetsPicker(Context ctx, Host host) {
        this.ctx = ctx;
        this.host = host;
        dp = ctx.getResources().getDisplayMetrics().density;
    }

    void show() {
        ReckonTheme t = host.theme();
        dialog = new Dialog(ctx);
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);

        LinearLayout sheet = new LinearLayout(ctx);
        sheet.setOrientation(LinearLayout.VERTICAL);
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(t.color("surface"));
        float r = 18 * dp;
        bg.setCornerRadii(new float[]{r, r, r, r, 0, 0, 0, 0});
        bg.setStroke(Math.max(1, (int) dp), t.color("border"));
        sheet.setBackground(bg);

        LinearLayout header = new LinearLayout(ctx);
        header.setGravity(Gravity.CENTER_VERTICAL);
        int pad = (int) (18 * dp);
        header.setPadding((int) (8 * dp), (int) (12 * dp), (int) (8 * dp), (int) (4 * dp));
        TextView create = text("New", 15, t.color("result"), true);
        create.setPadding(pad / 2 + pad / 4, (int) (8 * dp), pad, (int) (8 * dp));
        create.setOnClickListener(v -> {
            dialog.dismiss();
            host.createSheet();
        });
        header.addView(create);
        TextView title = text("Sheets", 17, t.color("text"), true);
        title.setGravity(Gravity.CENTER);
        header.addView(title, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1));
        TextView done = text("Done", 15, t.color("result"), true);
        done.setPadding(pad, (int) (8 * dp), pad / 2 + pad / 4, (int) (8 * dp));
        done.setOnClickListener(v -> dialog.dismiss());
        header.addView(done);
        sheet.addView(header);

        EditText search = new EditText(ctx);
        search.setHint("Search sheets");
        search.setSingleLine(true);
        search.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15);
        search.setTextColor(t.color("text"));
        search.setHintTextColor(t.color("muted"));
        GradientDrawable field = new GradientDrawable();
        field.setColor(t.color("bg"));
        field.setCornerRadius(10 * dp);
        field.setStroke(Math.max(1, (int) dp), t.color("border"));
        search.setBackground(field);
        search.setPadding((int) (12 * dp), (int) (8 * dp), (int) (12 * dp), (int) (8 * dp));
        LinearLayout.LayoutParams sp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        sp.setMargins((int) (14 * dp), (int) (4 * dp), (int) (14 * dp), (int) (8 * dp));
        sheet.addView(search, sp);
        search.addTextChangedListener(new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void onTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void afterTextChanged(Editable s) {
                query = s.toString().trim().toLowerCase(Locale.ROOT);
                fill();
            }
        });

        rows = new LinearLayout(ctx);
        rows.setOrientation(LinearLayout.VERTICAL);
        rows.setPadding(0, 0, 0, (int) (24 * dp));
        ScrollView scroll = new ScrollView(ctx);
        scroll.addView(rows);
        sheet.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1));
        fill();

        dialog.setContentView(sheet);
        Window w = dialog.getWindow();
        if (w != null) {
            w.setGravity(Gravity.BOTTOM);
            w.setLayout(ViewGroup.LayoutParams.MATCH_PARENT,
                    (int) (ctx.getResources().getDisplayMetrics().heightPixels * 0.6f));
            w.clearFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND);
            w.setBackgroundDrawable(new ColorDrawable(0));
        }
        dialog.show();
    }

    private void fill() {
        ReckonTheme t = host.theme();
        rows.removeAllViews();
        int shown = 0;
        for (Info s : list(host.dir())) {
            if (!query.isEmpty() && !s.title.toLowerCase(Locale.ROOT).contains(query)
                    && !read(new File(host.dir(), s.file)).toLowerCase(Locale.ROOT).contains(query)) {
                continue;
            }
            rows.addView(row(t, s));
            shown++;
        }
        if (shown == 0) {
            TextView empty = text(query.isEmpty() ? "No sheets" : "No matching sheets", 14, t.color("muted"), false);
            empty.setPadding((int) (22 * dp), (int) (12 * dp), 0, 0);
            rows.addView(empty);
        }
    }

    private static String read(File f) {
        try {
            return new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8);
        } catch (Exception e) {
            return "";
        }
    }

    private View row(ReckonTheme t, Info s) {
        boolean current = s.file.equals(host.current());
        LinearLayout row = new LinearLayout(ctx);
        row.setGravity(Gravity.CENTER_VERTICAL);
        int h = (int) (14 * dp);
        row.setPadding(h, (int) (10 * dp), h, (int) (10 * dp));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(current ? t.color("highlight") : 0);
        bg.setCornerRadius(10 * dp);
        row.setBackground(bg);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins((int) (10 * dp), 0, (int) (10 * dp), 0);
        row.setLayoutParams(lp);

        LinearLayout labels = new LinearLayout(ctx);
        labels.setOrientation(LinearLayout.VERTICAL);
        labels.addView(text(s.title, 16, t.color("text"), current));
        if (!s.preview.isEmpty()) {
            TextView p = text(s.preview, 12, t.color("muted"), false);
            p.setTypeface(Typeface.MONOSPACE);
            labels.addView(p);
        }
        row.addView(labels, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1));
        row.addView(text(current ? "✓" : "", 18, t.color("result"), true));
        row.setOnClickListener(v -> {
            dialog.dismiss();
            host.open(s.file);
        });
        row.setOnLongClickListener(v -> {
            host.delete(s.file);
            fill();
            return true;
        });
        row.setContentDescription(s.title + (current ? ", open" : "") + ". Long press to delete.");
        return row;
    }

    private TextView text(String s, int sp, int color, boolean bold) {
        TextView v = new TextView(ctx);
        v.setText(s);
        v.setTextSize(TypedValue.COMPLEX_UNIT_SP, sp);
        v.setTextColor(color);
        v.setSingleLine(true);
        v.setEllipsize(TextUtils.TruncateAt.END);
        if (bold) v.setTypeface(Typeface.create(Typeface.DEFAULT, 600, false));
        return v;
    }
}
