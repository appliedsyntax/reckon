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
    private File docFile;
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
        statusBar.addView(status, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));
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

        docFile = new File(getFilesDir(), "document.reckon");
        String text = WELCOME;
        try {
            if (docFile.exists()) text = new String(Files.readAllBytes(docFile.toPath()), StandardCharsets.UTF_8);
        } catch (IOException ignored) {
        }
        editor.setText(text);
        recompute();
        editor.addTextChangedListener(new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void onTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void afterTextChanged(Editable s) {
                recompute();
                save(s.toString());
            }
        });
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

    private void save(String text) {
        try (FileOutputStream f = new FileOutputStream(docFile)) {
            f.write(text.getBytes(StandardCharsets.UTF_8));
        } catch (IOException ignored) {
        }
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
