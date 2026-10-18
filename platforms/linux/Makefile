# Builds ibus-engine-tacit. Needs libibus-1.0-dev and a release build of
# platforms/ffi (cargo build --release -p tacit-ffi).
CFLAGS ?= -O2 -Wall
FFI = ../ffi
LIB = ../../target/release/libtacit_ffi.a

ibus-engine-tacit: engine.c $(FFI)/tacit.h $(LIB)
	$(CC) $(CFLAGS) -I$(FFI) $$(pkg-config --cflags ibus-1.0) engine.c $(LIB) \
	  $$(pkg-config --libs ibus-1.0) -lpthread -ldl -lm -o $@

clean:
	rm -f ibus-engine-tacit
