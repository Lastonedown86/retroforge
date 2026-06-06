#!/bin/bash
set -euo pipefail
LINROOT=/work/forgedash/target/linaro
LINDIR=$LINROOT/gcc-linaro-4.9-2016.02-x86_64_arm-linux-gnueabihf
if [ ! -x "$LINDIR/bin/arm-linux-gnueabihf-gcc" ]; then
  echo "downloading linaro 4.9 (glibc 2.21)..."
  mkdir -p "$LINROOT"; cd "$LINROOT"
  curl -sSL https://releases.linaro.org/components/toolchain/binaries/4.9-2016.02/arm-linux-gnueabihf/gcc-linaro-4.9-2016.02-x86_64_arm-linux-gnueabihf.tar.xz | tar -xJ
fi
LIN=$LINDIR/bin
cd /work/forgedash
rm -rf sysroot && mkdir -p sysroot && tar -xf devroot.tar -C sysroot
echo "== stub (linaro gcc) =="
"$LIN/arm-linux-gnueabihf-gcc" -O2 -c sdl_stubs.c -o sdl_stubs.o
export SDL2_NO_PKG_CONFIG=1
export SDL2_LIB_DIR=/work/forgedash/sysroot/usr/lib
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER="$LIN/arm-linux-gnueabihf-gcc"
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="-L/work/forgedash/sysroot/usr/lib -Clink-arg=/work/forgedash/sdl_stubs.o -Clink-arg=-Wl,-rpath-link,/work/forgedash/sysroot/usr/lib -Clink-arg=-Wl,--allow-shlib-undefined"
echo "== cargo build (linaro linker) =="
cargo build -p forgedash --release --target armv7-unknown-linux-gnueabihf
BIN=target/armv7-unknown-linux-gnueabihf/release/forgedash
echo "== max GLIBC =="; readelf -V "$BIN" 2>/dev/null | grep -oE "GLIBC_[0-9.]+" | sort -u | tr "\n" " "; echo
readelf -d "$BIN" 2>/dev/null | grep NEEDED
ls -la "$BIN"; echo BUILD-OK
