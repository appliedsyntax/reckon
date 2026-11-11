package local.reckon;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.res.Configuration;
import android.graphics.Color;
import android.graphics.Insets;
import android.os.Bundle;
import android.text.Editable;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.widget.LinearLayout;
import android.widget.TextView;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.List;
import java.util.TimeZone;

public final class MainActivity extends Activity {
    private static final String WELCOME = String.join("\n",
            "# Welcome to Reckon",
            "// Type math in plain words. Tap a result to copy it.",
            "",
            "Price: $20 + 56 EUR",
            "Discounted: prev - 5% discount",
            "20% of $75",
            "",
            "# Units",
            "1 meter 20 cm",
            "100 °F in celsius",
            "60 mph in km/h",
            "",
            "# Time",
            "today + 2 weeks",
            "New York time",
            "",
            "# Budget",
            "Rent: $1,200",
            "Food: $450",
            "Total: sum");

    private long engine;
    private CalcEditText editor;
    private TextView status;
    private File sheetsDir;
    /** File name of the open sheet. */
    private String current = "";
    /** Path of the last deleted sheet in the trash, for Undo. */
    private String trashed;
    private TextView sheetButton;
    private TextView undoButton;
    /** True while a sheet is being loaded, so loading does not save. */
    private boolean loading;
    private LinearLayout root;
    private LinearLayout statusBar;
    private TextView themeButton;
    private ReckonTheme.Store themes;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (getActionBar() != null) getActionBar().hide();
        engine = Native.engineNew(Native.utf8(getFilesDir().getAbsolutePath()));
        TimeZone tz = TimeZone.getDefault();
        Native.setLocalZone(engine, Native.utf8(tz.getID()), tz.getOffset(System.currentTimeMillis()) / 1000);

        themes = new ReckonTheme.Store(getSharedPreferences("reckon", MODE_PRIVATE));

        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);

        editor = new CalcEditText(this);
        editor.setOnResultTap(this::copy);
        root.addView(editor, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

        statusBar = new LinearLayout(this);
        statusBar.setGravity(Gravity.CENTER_VERTICAL);
        int pad = (int) (10 * getResources().getDisplayMetrics().density);
        statusBar.setPadding(pad + pad / 2, 0, pad / 2, 0);
        status = new TextView(this);
        status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        status.setSingleLine(true);
        sheetButton = new TextView(this);
        sheetButton.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        sheetButton.setTypeface(android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT, 600, false));
        sheetButton.setSingleLine(true);
        sheetButton.setEllipsize(android.text.TextUtils.TruncateAt.END);
        sheetButton.setMaxWidth((int) (150 * getResources().getDisplayMetrics().density));
        sheetButton.setPadding(0, pad, pad, pad);
        sheetButton.setOnClickListener(v -> new SheetsPicker(this, sheetsHost()).show());
        statusBar.addView(sheetButton);
        statusBar.addView(status, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));
        undoButton = new TextView(this);
        undoButton.setText("Undo");
        undoButton.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        undoButton.setPadding(pad, pad, pad, pad);
        undoButton.setVisibility(android.view.View.GONE);
        undoButton.setOnClickListener(v -> undoDelete());
        statusBar.addView(undoButton);
        themeButton = new TextView(this);
        themeButton.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        themeButton.setPadding(pad, pad, pad, pad);
        themeButton.setOnClickListener(v -> new ThemePicker(this, themes, new ThemePicker.Listener() {
            @Override public void onChanged() { applyTheme(); }
            @Override public boolean systemDark() { return isSystemDark(); }
        }).show());
        statusBar.addView(themeButton);
        root.addView(statusBar);
        applyTheme();

        // Edge-to-edge: keep content clear of the system bars and keyboard.
        root.setOnApplyWindowInsetsListener((v, insets) -> {
            Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.ime());
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom);
            return WindowInsets.CONSUMED;
        });
        setContentView(root);

        sheetsDir = new File(getFilesDir(), "sheets");
        File legacy = new File(getFilesDir(), "document.reckon");
        String first = Native.str(Native.sheetsPrepare(Native.utf8(sheetsDir.getAbsolutePath()),
                Native.utf8(legacy.getAbsolutePath()), Native.utf8(WELCOME)));
        String saved = getSharedPreferences("reckon", MODE_PRIVATE).getString("currentSheet", null);
        String start = saved != null && new File(sheetsDir, saved).exists() ? saved : first;
        editor.addTextChangedListener(new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void onTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void afterTextChanged(Editable s) {
                recompute();
                if (!loading) save(s.toString());
            }
        });
        if (start != null) load(start);
        status.setText("Rates: " + Native.str(Native.ratesSource(engine)));
        if (Native.ratesStale(engine)) refreshRates();
    }

    private boolean isSystemDark() {
        return (getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK)
                == Configuration.UI_MODE_NIGHT_YES;
    }

    /** Paints the window, editor and status bar in the current theme. */
    private void applyTheme() {
        ReckonTheme t = themes.resolve(isSystemDark());
        root.setBackgroundColor(t.color("bg"));
        editor.setTheme(t);
        statusBar.setBackgroundColor(t.color("status_bg"));
        status.setTextColor(t.color("muted"));
        themeButton.setTextColor(t.color("muted"));
        sheetButton.setTextColor(t.color("text"));
        undoButton.setTextColor(t.color("text"));
        themeButton.setText("◐  " + t.name);
        themeButton.setContentDescription("Theme: " + t.name);
        // Dark icons in the system bars on light themes, light icons on dark ones.
        WindowInsetsController c = getWindow().getInsetsController();
        if (c != null) {
            int light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                    | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
            c.setSystemBarsAppearance(t.dark ? 0 : light, light);
        }
    }

    @Override
    public void onConfigurationChanged(Configuration config) {
        super.onConfigurationChanged(config);
        applyTheme();  // the system switched light/dark (uiMode is handled here)
    }

    @Override
    protected void onDestroy() {
        Native.engineFree(engine);
        engine = 0;
        super.onDestroy();
    }

    private void recompute() {
        String out = Native.str(Native.evaluateHighlighted(engine, Native.utf8(editor.getText().toString())));
        String[] lines = out == null ? new String[0] : out.split("\n", -1);
        String[] results = new String[lines.length];
        String[] spans = new String[lines.length];
        for (int i = 0; i < lines.length; i++) {
            int tab = lines[i].indexOf('\t');
            results[i] = tab < 0 ? lines[i] : lines[i].substring(0, tab);
            spans[i] = tab < 0 ? "" : lines[i].substring(tab + 1);
        }
        editor.setOutput(results, spans);
    }

    /** Writes the open sheet through a temporary file, so it is never half-written. */
    private void save(String text) {
        if (current.isEmpty()) return;
        File tmp = new File(sheetsDir, "." + current + ".tmp");
        try (FileOutputStream f = new FileOutputStream(tmp)) {
            f.write(text.getBytes(StandardCharsets.UTF_8));
        } catch (IOException e) {
            return;
        }
        if (tmp.renameTo(new File(sheetsDir, current))) updateSheetButton(text);
    }

    private void updateSheetButton(String text) {
        String title = "Untitled";
        for (String line : text.split("\n")) {
            String l = line.trim().replaceFirst("^#+", "").replaceFirst("^//", "").trim();
            if (!l.isEmpty()) {
                title = l;
                break;
            }
        }
        sheetButton.setText("▤  " + title);
        sheetButton.setContentDescription("Sheets. Current: " + title);
    }

    /** Opens `file`, first renaming the sheet being left to match its title. */
    private void load(String file) {
        if (!current.isEmpty() && !current.equals(file)) {
            Native.sheetTidy(Native.utf8(sheetsDir.getAbsolutePath()), Native.utf8(current));
        }
        String text = "";
        try {
            text = new String(Files.readAllBytes(new File(sheetsDir, file).toPath()), StandardCharsets.UTF_8);
        } catch (IOException ignored) {
        }
        current = file;
        getSharedPreferences("reckon", MODE_PRIVATE).edit().putString("currentSheet", file).apply();
        loading = true;
        editor.setText(text);
        loading = false;
        editor.setSelection(text.length());
        recompute();
        updateSheetButton(text);
    }

    private void newSheet() {
        String file = Native.str(Native.sheetCreate(Native.utf8(sheetsDir.getAbsolutePath()), Native.utf8("# Untitled\n")));
        if (file == null) return;
        load(file);
        // Select the title so typing replaces it.
        editor.requestFocus();
        editor.setSelection(2, "# Untitled".length());
        getSystemService(android.view.inputmethod.InputMethodManager.class)
                .showSoftInput(editor, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);
    }

    private void deleteSheet(String file) {
        String title = file.replaceFirst("\\.reckon$", "");
        String path = Native.str(Native.sheetTrash(Native.utf8(sheetsDir.getAbsolutePath()), Native.utf8(file)));
        if (path == null) return;
        trashed = path;
        if (file.equals(current)) {
            current = "";
            List<SheetsPicker.Info> rest = SheetsPicker.list(sheetsDir);
            if (rest.isEmpty()) newSheet(); else load(rest.get(0).file);
        }
        status.setText("Deleted “" + title + "”");
        undoButton.setVisibility(android.view.View.VISIBLE);
    }

    private void undoDelete() {
        if (trashed == null) return;
        String file = Native.str(Native.sheetRestore(Native.utf8(sheetsDir.getAbsolutePath()), Native.utf8(trashed)));
        trashed = null;
        undoButton.setVisibility(android.view.View.GONE);
        if (file != null) {
            load(file);
            status.setText("Restored");
        }
    }

    private SheetsPicker.Host sheetsHost() {
        return new SheetsPicker.Host() {
            @Override public File dir() { return sheetsDir; }
            @Override public String current() { return current; }
            @Override public ReckonTheme theme() { return themes.resolve(isSystemDark()); }
            @Override public void open(String file) { load(file); }
            @Override public void createSheet() { newSheet(); }
            @Override public void delete(String file) { deleteSheet(file); }
        };
    }

    private void copy(String result) {
        ClipboardManager cm = getSystemService(ClipboardManager.class);
        cm.setPrimaryClip(ClipData.newPlainText("Reckon result", result));
        status.setText("Copied " + result);
    }

    private void refreshRates() {
        status.setText("Updating exchange rates…");
        final String fiatUrl = Native.str(Native.fiatUrl());
        final String cryptoUrl = Native.str(Native.cryptoUrl());
        new Thread(() -> {
            String fiat = download(fiatUrl);
            String crypto = download(cryptoUrl);
            // The engine is used from the UI thread only.
            runOnUiThread(() -> {
                if (engine == 0) return;
                if (fiat == null) {
                    status.setText("Rates not updated (offline?)");
                    return;
                }
                String err = Native.str(Native.applyRates(engine, Native.utf8(fiat), Native.utf8(crypto)));
                status.setText(err == null ? "Rates: " + Native.str(Native.ratesSource(engine)) : "Rates not updated: " + err);
                recompute();
            });
        }, "rates").start();
    }

    private static String download(String url) {
        HttpURLConnection c = null;
        try {
            c = (HttpURLConnection) URI.create(url).toURL().openConnection();
            c.setConnectTimeout(10000);
            c.setReadTimeout(15000);
            if (c.getResponseCode() != 200) return null;
            try (InputStream in = c.getInputStream()) {
                ByteArrayOutputStream out = new ByteArrayOutputStream();
                byte[] buf = new byte[8192];
                int n;
                while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
                return out.toString(StandardCharsets.UTF_8.name());
            }
        } catch (IOException e) {
            return null;
        } finally {
            if (c != null) c.disconnect();
        }
    }
}
