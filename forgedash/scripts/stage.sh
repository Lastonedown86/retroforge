#!/bin/sh
# Host-side: stage ForgeDash + runtime assets into device tmpfs over SSH.
# dropbear has no sftp-server, so we pipe a tar over stdin.
# Run from the forgedash/ directory AFTER a device build.
set -eu
DEV="${DEV:-169.254.13.37}"
BIN="target/armv7-unknown-linux-gnueabihf/release/forgedash"

[ -f "$BIN" ] || { echo "build first: cargo build -p forgedash --release --target armv7-unknown-linux-gnueabihf"; exit 1; }
[ -d app/art ]  || echo "warning: app/art missing (covers will fall back to solid cards)"
[ -d app/roms ] || { echo "error: app/roms missing — add real .nes ROMs referenced by app/library.json"; exit 1; }

ROOT="$(pwd)"
tar -cf - \
    -C "$(dirname "$ROOT/$BIN")" forgedash \
    -C "$ROOT/app" library.json \
    -C "$ROOT/app" art \
    -C "$ROOT/app" roms \
    -C "$ROOT/scripts" forge-loop.sh \
  | ssh "root@$DEV" 'mkdir -p /tmp/forge && tar -xf - -C /tmp/forge && chmod +x /tmp/forge/forgedash /tmp/forge/forge-loop.sh'

echo "staged to /tmp/forge on $DEV"
echo "on device: bring up GPU+pad (controller-memboot recipe) + stage RA into /tmp/ra, then: sh /tmp/forge/forge-loop.sh"
