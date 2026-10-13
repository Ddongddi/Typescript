# Builds ibus-engine-typescript. Needs libibus-1.0-dev and a release build of
# platforms/ffi (cargo build --release -p typescript-ffi).
CFLAGS ?= -O2 -Wall
FFI = ../ffi
LIB = ../../target/release/libtypescript_ffi.a

ibus-engine-typescript: engine.c $(FFI)/typescript.h $(LIB)
	$(CC) $(CFLAGS) -I$(FFI) $$(pkg-config --cflags ibus-1.0) engine.c $(LIB) \
	  $$(pkg-config --libs ibus-1.0) -lpthread -ldl -lm -o $@

clean:
	rm -f ibus-engine-typescript
