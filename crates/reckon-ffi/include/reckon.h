/* C interface to the Reckon calculator engine.
 *
 * Strings are UTF-8. Every `char *` returned by this API must be released
 * with reckon_string_free(). An engine is not thread-safe: use it from one
 * thread at a time.
 */
#ifndef RECKON_H
#define RECKON_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ReckonEngine ReckonEngine;

/* Creates an engine. `data_dir` holds the rate cache and plugins; NULL uses
 * the platform default. Loads cached rates and plugins from it. */
ReckonEngine *reckon_engine_new(const char *data_dir);
void reckon_engine_free(ReckonEngine *engine);

/* Sets the local time zone (IANA name such as "Europe/Berlin") and its
 * current UTC offset in seconds, used if no zone database is available. */
void reckon_set_local_zone(ReckonEngine *engine, const char *iana, int offset_seconds);

/* Evaluates a document. Returns one output line per input line: the
 * formatted result, or an empty line when the line has no result. */
char *reckon_evaluate(ReckonEngine *engine, const char *document);

/* Like reckon_evaluate, plus syntax highlighting. Each output line is
 *   <result> TAB <span>;<span>;...
 * where a span is "start,length,kind". start and length count UTF-16 code
 * units from the start of that input line (what NSString and Java String
 * index by). kind is one of: number unit keyword variable function
 * operator label comment header error. */
char *reckon_evaluate_highlighted(ReckonEngine *engine, const char *document);

/* All colour themes as a JSON array: name, blurb, dark, pattern ("none",
 * "grid", "ruled", "scanlines"), double_divider, result_weight (400-700),
 * block_cursor and colors (an object of "#rrggbbaa" strings). */
char *reckon_themes(void);

/* Sheets: a folder of .reckon files (see reckon_core::sheets). A sheet's
 * title is its first non-empty line. Returned names are file names inside
 * `dir`; NULL means the operation failed. */

/* JSON array of {file, title, preview, modified}, newest first. */
char *reckon_sheets_list(const char *dir);
/* Creates `dir` if needed, moves `legacy` (an old single document; may be
 * NULL) into it, or adds a first sheet holding `welcome` when the folder is
 * empty. Returns the sheet to open. */
char *reckon_sheets_prepare(const char *dir, const char *legacy, const char *welcome);
/* New sheet named after its first line. */
char *reckon_sheet_create(const char *dir, const char *text);
/* Renames the file to match its current title (call when leaving a sheet). */
char *reckon_sheet_tidy(const char *dir, const char *file);
/* Moves the sheet to dir/.trash; returns its path there for restore. */
char *reckon_sheet_trash(const char *dir, const char *file);
char *reckon_sheet_restore(const char *dir, const char *trashed_path);

/* 1 when exchange rates are built-in or older than 12 hours. */
int reckon_rates_stale(const ReckonEngine *engine);

/* URLs the host app should download to refresh rates. */
char *reckon_fiat_url(void);
char *reckon_crypto_url(void);

/* Applies downloaded rate JSON (`crypto_json` may be NULL) and saves the
 * cache. Returns NULL on success, or an error message. */
char *reckon_apply_rates(ReckonEngine *engine, const char *fiat_json, const char *crypto_json);

/* Short description of where rates come from, e.g. "ExchangeRate-API". */
char *reckon_rates_source(const ReckonEngine *engine);

void reckon_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif
