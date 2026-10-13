/* C interface of the Typescript input method (libtypescript_ffi). See src/lib.rs. */
#ifndef TYPESCRIPT_H
#define TYPESCRIPT_H
#include <stdint.h>

typedef struct Handle Handle;

/* dir: where settings.json, apps.json and lexicon.json live. NULL on failure. */
Handle *ts_new(const char *dir);
void ts_free(Handle *h);
void ts_string_free(char *s);

/* A new text field in the program `app` (NULL or "" when unknown). */
void ts_activate(Handle *h, const char *app);

/* kind: 0 letter, 1 printable, 2 delete, 3 escape, 4 cycle, 5 enter, 6 tab, else other.
   before: the text before the caret, ending with the composing text.
   Returns "1" (consumed) or "0", then each command preceded by U+0001:
   M<text> set the composing text, I<text> commit it (replacing the composing text),
   B<n> delete n UTF-16 units before the caret. Free with ts_string_free. */
char *ts_key(Handle *h, int kind, uint32_t ch, const char *before);
void ts_reset(Handle *h);
char *ts_toggle_pause(Handle *h, const char *before);
int ts_paused(Handle *h);

#endif
