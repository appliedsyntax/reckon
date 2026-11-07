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
import android.view.WindowInsets;
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

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (getActionBar() != null) getActionBar().hide();
        engine = Native.engineNew(Native.utf8(getFilesDir().getAbsolutePath()));
        TimeZone tz = TimeZone.getDefault();
        Native.setLocalZone(engine, Native.utf8(tz.getID()), tz.getOffset(System.currentTimeMillis()) / 1000);

        boolean dark = (getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK)
                == Configuration.UI_MODE_NIGHT_YES;
        int bg = dark ? Color.rgb(0x1c, 0x1c, 0x1e) : Color.rgb(0xfb, 0xfb, 0xfa);
        int fg = dark ? Color.rgb(0xe4, 0xe4, 0xe6) : Color.rgb(0x2f, 0x33, 0x37);
        int muted = dark ? Color.rgb(0x7f, 0x84, 0x8e) : Color.rgb(0x8b, 0x90, 0x98);
        int green = dark ? Color.rgb(0x7e, 0xe0, 0xa1) : Color.rgb(0x1f, 0x8a, 0x4c);
        int divider = dark ? Color.rgb(0x2c, 0x2c, 0x2f) : Color.rgb(0xe7, 0xe7, 0xe4);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(bg);

        editor = new CalcEditText(this);
        editor.setTextColor(fg);
        editor.setResultColors(green, divider);
        editor.setOnResultTap(this::copy);
        root.addView(editor, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

        status = new TextView(this);
        status.setTextColor(muted);
        status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
        int pad = (int) (10 * getResources().getDisplayMetrics().density);
        status.setPadding(pad + pad / 2, pad / 2, pad, pad / 2);
        status.setSingleLine(true);
        root.addView(status);

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

    @Override
    protected void onDestroy() {
        Native.engineFree(engine);
        engine = 0;
        super.onDestroy();
    }

    private void recompute() {
        String out = Native.str(Native.evaluate(engine, Native.utf8(editor.getText().toString())));
        editor.setResults(out == null ? new String[0] : out.split("\n", -1));
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
