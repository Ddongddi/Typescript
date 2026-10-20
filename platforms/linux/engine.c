/* tacit for IBus: the engine process ibus-daemon starts (see tacit.xml).
   All the typing logic is in libtacit_ffi; this file maps IBus key events to it and its
   commands (preedit / commit) back to IBus. */
#include <ibus.h>
#include <stdlib.h>
#include <string.h>
#include "tacit.h"

#define ENGINE_NAME "tacit"
#define SEP '\x01'

/* Kinds, as in tacit.h. */
enum { LETTER, PRINTABLE, DELETE, ESCAPE, CYCLE, ENTER, TAB, OTHER };

typedef struct { IBusEngine parent; } TsEngine;
typedef struct { IBusEngineClass parent; } TsEngineClass;

static TacitHandle *ime;
static GString *composing; /* the preedit shown now */
static gboolean secure;    /* the focused field is a password / PIN field: keys go to the app */

G_DEFINE_TYPE(TsEngine, ts_engine, IBUS_TYPE_ENGINE)

static void show_preedit(IBusEngine *e, const char *text) {
  g_string_assign(composing, text);
  if (!*text) {
    ibus_engine_hide_preedit_text(e);
    return;
  }
  IBusText *t = ibus_text_new_from_string(text);
  ibus_text_append_attribute(t, IBUS_ATTR_TYPE_UNDERLINE, IBUS_ATTR_UNDERLINE_SINGLE, 0, -1);
  ibus_engine_update_preedit_text(e, t, g_utf8_strlen(text, -1), TRUE);
}

/* Plays the commands of ts_key / ts_toggle_pause; returns whether the key was consumed. */
static gboolean apply(IBusEngine *e, char *out) {
  if (!out) return FALSE;
  gboolean consumed = out[0] == '1';
  char **ops = g_strsplit(out + 1, "\x01", -1);
  for (char **op = ops; *op; op++) {
    const char *text = *op + 1;
    switch (**op) {
      case 'M': show_preedit(e, text); break;
      case 'I': {
        show_preedit(e, "");
        IBusText *t = ibus_text_new_from_string(text);
        ibus_engine_commit_text(e, t);
        break;
      }
      case 'B': ibus_engine_delete_surrounding_text(e, -atoi(text), atoi(text)); break;
      default: break;
    }
  }
  g_strfreev(ops);
  ts_string_free(out);
  return consumed;
}

static int kind_of(guint keyval, guint32 *ch) {
  *ch = 0;
  if ((keyval >= 'a' && keyval <= 'z') || (keyval >= 'A' && keyval <= 'Z')) {
    *ch = keyval;
    return LETTER;
  }
  if (keyval >= 0x20 && keyval <= 0x7e) {
    *ch = keyval;
    return PRINTABLE;
  }
  switch (keyval) {
    case IBUS_KEY_BackSpace: return DELETE;
    case IBUS_KEY_Escape: return ESCAPE;
    case IBUS_KEY_Return: case IBUS_KEY_KP_Enter: return ENTER;
    case IBUS_KEY_Tab: return TAB;
    default: return OTHER;
  }
}

static gboolean process_key(IBusEngine *e, guint keyval, guint keycode, guint state) {
  if (state & IBUS_RELEASE_MASK || secure) return FALSE;
  /* The pause key (한/영) turns tacit off and on in this text field. */
  if (keyval == IBUS_KEY_Hangul || keyval == IBUS_KEY_Pause)
    return apply(e, ts_toggle_pause(ime, composing->str, 0));
  /* Shift+Esc cycles the reading of the word, like on Android. */
  guint32 ch;
  int kind = kind_of(keyval, &ch);
  if (kind == ESCAPE && (state & IBUS_SHIFT_MASK)) kind = CYCLE;
  if (state & (IBUS_CONTROL_MASK | IBUS_MOD1_MASK | IBUS_SUPER_MASK)) {
    kind = OTHER; /* shortcuts end the word and go to the app */
    ch = 0;
  }
  return apply(e, ts_key(ime, kind, ch, composing->str, 0));
}

static void forget(IBusEngine *e) {
  ts_reset(ime);
  show_preedit(e, "");
}

static void focus_in(IBusEngine *e) {
  ts_activate(ime, ""); /* IBus does not tell which program it is */
  g_string_truncate(composing, 0);
}
static void focus_out(IBusEngine *e) { forget(e); }
/* The app tells what the field is for; a password is never read, counted or learned. */
static void set_content_type(IBusEngine *e, guint purpose, guint hints) {
  secure = purpose == IBUS_INPUT_PURPOSE_PASSWORD || purpose == IBUS_INPUT_PURPOSE_PIN;
  if (secure) forget(e);
}
static void reset(IBusEngine *e) { forget(e); }
static void disable(IBusEngine *e) { forget(e); }

static void ts_engine_class_init(TsEngineClass *klass) {
  IBusEngineClass *c = IBUS_ENGINE_CLASS(klass);
  c->process_key_event = process_key;
  c->focus_in = focus_in;
  c->focus_out = focus_out;
  c->set_content_type = set_content_type;
  c->reset = reset;
  c->disable = disable;
}
static void ts_engine_init(TsEngine *self) {}

static void bus_disconnected(IBusBus *bus, gpointer data) { ibus_quit(); }

int main(int argc, char **argv) {
  gboolean from_ibus = argc > 1 && strcmp(argv[1], "--ibus") == 0;
  ibus_init();
  ime = ts_new(NULL); /* the user's usual settings folder */
  if (!ime) return 1;
  composing = g_string_new("");
  IBusBus *bus = ibus_bus_new();
  g_signal_connect(bus, "disconnected", G_CALLBACK(bus_disconnected), NULL);
  IBusFactory *factory = ibus_factory_new(ibus_bus_get_connection(bus));
  ibus_factory_add_engine(factory, ENGINE_NAME, ts_engine_get_type());
  if (from_ibus) {
    ibus_bus_request_name(bus, "io.github.iamallyouneed.tacit", 0);
  } else {
    IBusComponent *component = ibus_component_new(
        "io.github.iamallyouneed.tacit", "Tacit", "0", "AGPL-3.0-only", "Necessity",
        "https://github.com/iamallyouneed/tacit", "", "ibus-tacit");
    ibus_component_add_engine(component,
        ibus_engine_desc_new(ENGINE_NAME, "Tacit", "English / Korean / Japanese by word",
                             "en", "AGPL-3.0-only", "Necessity", "", "us"));
    ibus_bus_register_component(bus, component);
  }
  ibus_main();
  ts_free(ime);
  return 0;
}
