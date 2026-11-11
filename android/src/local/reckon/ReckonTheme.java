package local.reckon;

import android.content.SharedPreferences;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * One colour theme. The themes are defined once, in the Rust engine
 * (reckon_core::themes), and read here through reckon_themes().
 */
final class ReckonTheme {
    final int index;
    final String name;
    final String blurb;
    final boolean dark;
    /** "none", "grid", "ruled" or "scanlines". */
    final String pattern;
    final boolean doubleDivider;
    final int resultWeight;
    final boolean blockCursor;
    private final Map<String, Integer> colors;

    private ReckonTheme(int index, JSONObject o) throws JSONException {
        this.index = index;
        name = o.getString("name");
        blurb = o.optString("blurb");
        dark = o.optBoolean("dark");
        pattern = o.optString("pattern", "none");
        doubleDivider = o.optBoolean("double_divider");
        resultWeight = o.optInt("result_weight", 600);
        blockCursor = o.optBoolean("block_cursor");
        colors = new HashMap<>();
        JSONObject c = o.getJSONObject("colors");
        for (java.util.Iterator<String> it = c.keys(); it.hasNext(); ) {
            String k = it.next();
            colors.put(k, parse(c.getString(k)));
        }
    }

    /** A colour by its engine name ("bg", "text", "result", "number", …) as ARGB. */
    int color(String key) {
        Integer v = colors.get(key);
        return v == null ? 0xff808080 : v;
    }

    /** "#rrggbbaa" to Android's 0xAARRGGBB. */
    private static int parse(String hex) {
        long v = Long.parseLong(hex.startsWith("#") ? hex.substring(1) : hex, 16);
        int rgb = (int) (v >>> 8) & 0xffffff;
        int a = (int) (v & 0xff);
        return (a << 24) | rgb;
    }

    private static List<ReckonTheme> all;

    static synchronized List<ReckonTheme> all() {
        if (all == null) {
            List<ReckonTheme> list = new ArrayList<>();
            try {
                JSONArray a = new JSONArray(Native.str(Native.themes()));
                for (int i = 0; i < a.length(); i++) list.add(new ReckonTheme(i, a.getJSONObject(i)));
            } catch (JSONException ignored) {
            }
            all = Collections.unmodifiableList(list);
        }
        return all;
    }

    static ReckonTheme named(String name, ReckonTheme fallback) {
        if (name != null) {
            for (ReckonTheme t : all()) if (t.name.equalsIgnoreCase(name)) return t;
        }
        return fallback;
    }

    /**
     * Which theme to show, saved in SharedPreferences. Same model as the
     * desktop app: follow the system with a light and a dark theme, or always
     * use one.
     */
    static final class Store {
        private final SharedPreferences prefs;
        boolean followSystem;
        ReckonTheme fixed, light, dark;

        Store(SharedPreferences prefs) {
            this.prefs = prefs;
            List<ReckonTheme> all = all();
            ReckonTheme paper = named("Paper", all.get(0));
            ReckonTheme graphite = named("Graphite", all.get(all.size() - 1));
            followSystem = !"fixed".equals(prefs.getString("themeMode", "system"));
            light = named(prefs.getString("lightTheme", null), paper);
            dark = named(prefs.getString("darkTheme", null), graphite);
            fixed = named(prefs.getString("theme", null), graphite);
        }

        ReckonTheme resolve(boolean systemDark) {
            return followSystem ? (systemDark ? dark : light) : fixed;
        }

        /** Always use `t`; it also becomes the light or dark theme for "Match System". */
        void pick(ReckonTheme t) {
            followSystem = false;
            fixed = t;
            if (t.dark) dark = t; else light = t;
            save();
        }

        void matchSystem() {
            followSystem = true;
            save();
        }

        private void save() {
            prefs.edit()
                    .putString("themeMode", followSystem ? "system" : "fixed")
                    .putString("theme", fixed.name)
                    .putString("lightTheme", light.name)
                    .putString("darkTheme", dark.name)
                    .apply();
        }
    }
}
