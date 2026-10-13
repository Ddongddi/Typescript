set -e
R=$HOME/sysroot/root
mkdir -p "$R"
cd $HOME/sysroot/debs
for d in *.deb; do dpkg -x "$d" "$R"; done
U=$R/usr
cd /mnt/c/playground/Typescript/platforms/linux
gcc -fsyntax-only -Wall -I../ffi -I$U/include/ibus-1.0 -I$U/include/glib-2.0 -I$U/lib/x86_64-linux-gnu/glib-2.0/include engine.c 2>&1 | head -30
echo gcc-done
