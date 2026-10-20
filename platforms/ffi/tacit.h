/* C interface of the tacit input method (libtacit_ffi). See src/lib.rs. */
#ifndef TACIT_H
#define TACIT_H
#include <stdint.h>

typedef struct TacitHandle TacitHandle;

/* dir: where settings.json, apps.json and lexicon.json live. NULL on failure. */
TacitHandle *ts_new(const char *dir);
void ts_free(TacitHandle *h);
void ts_string_free(char *s);

/* A new text field in the program `app` (NULL or "" when unknown). Call when changing fields. */
void ts_activate(TacitHandle *h, const char *app);

/* kind: 0 letter, 1 printable, 2 delete, 3 escape, 4 cycle, 5 enter, 6 tab, else other.
   before (for ts_key and ts_toggle_pause): the text before the caret, ending with the composing
   text. before_start is its first UTF-16 coordinate in the host's stable text-field/document
   coordinate domain, not an offset relative to this supplied substring. When before is NULL,
   before_start is the bridge-owned composition's start. Negative before_start (normally -1)
   means the coordinate identity is unknown. A later matching suffix does not establish the
   identity of a commit made with an unknown origin and cannot restore its correction window.
   NULL before retains only the bridge's current composition; a non-NULL value, including "", is
   authoritative and a mismatch ends the old composition. Keep one coordinate domain for calls
   on a text field; call ts_activate for a new field, or ts_reset when the host ends the mark or
   changes coordinate domains. Temporary loss of document access may use NULL/-1 without reset.
   Returns "1" (consumed) or "0", then each command preceded by U+0001:
   M<text> set the composing text, I<text> commit it (replacing the composing text),
   B<n> delete n UTF-16 units before the caret. Free with ts_string_free. */
char *ts_key(TacitHandle *h, int kind, uint32_t ch, const char *before, int64_t before_start);
void ts_reset(TacitHandle *h);
char *ts_toggle_pause(TacitHandle *h, const char *before, int64_t before_start);
int ts_paused(TacitHandle *h);

#endif
