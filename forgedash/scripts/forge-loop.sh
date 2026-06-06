#!/bin/sh
# ForgeDash supervisor: one GLES app holds the Mali surface at a time.
# Runs the dashboard; if it writes a handoff, launches RetroArch with that ROM,
# then loops back to the dashboard.
set -u
FORGE_ROOT="${FORGE_ROOT:-/tmp/forge}"
RA_ROOT="${RA_ROOT:-/tmp/ra}"
HANDOFF="$FORGE_ROOT/handoff"

cd "$FORGE_ROOT" || exit 1

while true; do
    rm -f "$HANDOFF"
    # Dashboard reads library.json from the current dir.
    ./forgedash "$FORGE_ROOT/library.json" >"$FORGE_ROOT/forge.log" 2>&1

    if [ -f "$HANDOFF" ]; then
        CORE=$(cut -f1 "$HANDOFF")
        ROM=$(cut -f2 "$HANDOFF")
        APPEND=""
        [ -f "$FORGE_ROOT/ra-override.cfg" ] && APPEND="--appendconfig $FORGE_ROOT/ra-override.cfg"
        setsid env HOME="$RA_ROOT/etc/libretro" LD_LIBRARY_PATH=/usr/lib \
            "$RA_ROOT/bin/retroarch" \
            -c "$RA_ROOT/etc/libretro/retroarch.cfg" \
            $APPEND \
            -L "$RA_ROOT/etc/libretro/core/${CORE}_libretro.so" \
            "$FORGE_ROOT/$ROM" \
            </dev/null >"$FORGE_ROOT/ra.log" 2>&1
        RA_STATUS=$?
        [ "$RA_STATUS" -ne 0 ] && echo "$RA_STATUS" > "$FORGE_ROOT/ra_exit"
        # RA exits (user picked Quit RetroArch); loop relaunches the dashboard.
    else
        # Dashboard quit without a selection -> exit the loop.
        break
    fi
done
