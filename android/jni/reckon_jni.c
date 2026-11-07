/* JNI bridge between local.reckon.Native (Java) and the Rust engine's C API.
 * Strings cross as UTF-8 byte arrays: JNI's own "modified UTF-8" mangles
 * characters outside the BMP (emoji). */
#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "reckon.h"

static char *from_bytes(JNIEnv *env, jbyteArray a) {
    if (a == NULL) return NULL;
    jsize n = (*env)->GetArrayLength(env, a);
    char *s = malloc((size_t)n + 1);
    if (s == NULL) return NULL;
    (*env)->GetByteArrayRegion(env, a, 0, n, (jbyte *)s);
    s[n] = 0;
    return s;
}

/* Converts and frees a string returned by the engine. */
static jbyteArray to_bytes(JNIEnv *env, char *s) {
    if (s == NULL) return NULL;
    jsize n = (jsize)strlen(s);
    jbyteArray a = (*env)->NewByteArray(env, n);
    if (a != NULL) (*env)->SetByteArrayRegion(env, a, 0, n, (const jbyte *)s);
    reckon_string_free(s);
    return a;
}

#define ENGINE(h) ((ReckonEngine *)(intptr_t)(h))

JNIEXPORT jlong JNICALL Java_local_reckon_Native_engineNew(JNIEnv *env, jclass c, jbyteArray dir) {
    char *d = from_bytes(env, dir);
    ReckonEngine *e = reckon_engine_new(d);
    free(d);
    return (jlong)(intptr_t)e;
}

JNIEXPORT void JNICALL Java_local_reckon_Native_engineFree(JNIEnv *env, jclass c, jlong h) {
    reckon_engine_free(ENGINE(h));
}

JNIEXPORT void JNICALL Java_local_reckon_Native_setLocalZone(JNIEnv *env, jclass c, jlong h, jbyteArray iana, jint offset) {
    char *z = from_bytes(env, iana);
    reckon_set_local_zone(ENGINE(h), z, offset);
    free(z);
}

JNIEXPORT jbyteArray JNICALL Java_local_reckon_Native_evaluate(JNIEnv *env, jclass c, jlong h, jbyteArray doc) {
    char *d = from_bytes(env, doc);
    jbyteArray r = to_bytes(env, reckon_evaluate(ENGINE(h), d));
    free(d);
    return r;
}

JNIEXPORT jboolean JNICALL Java_local_reckon_Native_ratesStale(JNIEnv *env, jclass c, jlong h) {
    return reckon_rates_stale(ENGINE(h)) ? JNI_TRUE : JNI_FALSE;
}

JNIEXPORT jbyteArray JNICALL Java_local_reckon_Native_fiatUrl(JNIEnv *env, jclass c) {
    return to_bytes(env, reckon_fiat_url());
}

JNIEXPORT jbyteArray JNICALL Java_local_reckon_Native_cryptoUrl(JNIEnv *env, jclass c) {
    return to_bytes(env, reckon_crypto_url());
}

JNIEXPORT jbyteArray JNICALL Java_local_reckon_Native_applyRates(JNIEnv *env, jclass c, jlong h, jbyteArray fiat, jbyteArray crypto) {
    char *f = from_bytes(env, fiat);
    char *k = from_bytes(env, crypto);
    jbyteArray r = to_bytes(env, reckon_apply_rates(ENGINE(h), f, k));
    free(f);
    free(k);
    return r;
}

JNIEXPORT jbyteArray JNICALL Java_local_reckon_Native_ratesSource(JNIEnv *env, jclass c, jlong h) {
    return to_bytes(env, reckon_rates_source(ENGINE(h)));
}
