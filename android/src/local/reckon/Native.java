package local.reckon;

import java.nio.charset.StandardCharsets;

/** Thin wrapper over the Rust engine (libreckon.so, see android/jni). */
final class Native {
    static {
        System.loadLibrary("reckon");
    }

    private Native() {}

    static native long engineNew(byte[] dataDir);
    static native void engineFree(long engine);
    static native void setLocalZone(long engine, byte[] iana, int offsetSeconds);
    static native byte[] evaluate(long engine, byte[] document);
    static native boolean ratesStale(long engine);
    static native byte[] fiatUrl();
    static native byte[] cryptoUrl();
    static native byte[] applyRates(long engine, byte[] fiat, byte[] crypto);
    static native byte[] ratesSource(long engine);

    static byte[] utf8(String s) {
        return s == null ? null : s.getBytes(StandardCharsets.UTF_8);
    }

    static String str(byte[] b) {
        return b == null ? null : new String(b, StandardCharsets.UTF_8);
    }
}
