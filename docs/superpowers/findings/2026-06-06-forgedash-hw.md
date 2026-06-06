# ForgeDash Tracer Bullet — Hardware Findings

> Records the on-device run of the ForgeDash custom dashboard
> (`docs/superpowers/plans/2026-06-06-forgedash-tracer-bullet.md`).
> **Verdict: HW-PASSED** on real NES Classic (`dp-nes`). Our own Rust + GLES2
> binary renders a coverflow via Mali, navigates by the controller, launches NES
> ROMs into RetroArch, and returns on quit. Zero NAND writes (RAM-memboot only).

## Result — full acceptance PASS

Device: NES Classic `dp-nes`, brick-safe RAM-memboot (the SOLVED GPU+pad bring-up
already active: `mali`/`clovercon`/`evdev` loaded, clover-mcp kick done, event24
udev-tagged). RetroArch staged at `/tmp/ra` (ClusterM 1.1d), cores
`fceumm_libretro.so` + `nestopia_libretro.so`.

| Acceptance step | Result |
|---|---|
| ForgeDash gets a Mali GLES2 surface @1280×720 (no BAD_ALLOC) | ✅ `video driver = mali`, ctx created + made current |
| Coverflow renders (modern-dark, 4 cards, title/metadata) | ✅ user-confirmed on HDMI |
| D-pad Left/Right navigates the shelf | ✅ |
| A launches the selected ROM into RetroArch | ✅ NES game runs |
| SELECT+START → Quit RA → shell-loop relaunches ForgeDash | ✅ |
| Zero NAND writes (power-cycle = bone stock) | ✅ |

## The toolchain saga (the real work)

The code was right; getting a *runnable* armv7 binary against this device's old
userland was the hard part. Device = Buildroot **glibc 2.22**, `ld-2.22.so`,
Cortex-A7. Build host = headless Windows + Docker (no native ARM toolchain; no
CMake/C compiler; WSL present but passwordless-sudo unavailable, and WSL2 can't
reach the device's link-local RNDIS NIC).

What failed and why:
1. **Debian modern cross-gcc** (`gcc-arm-linux-gnueabihf`): links fine but pulls
   `GLIBC_2.28`–`2.34` refs → device 2.22 can't satisfy → loader errors.
2. **cargo-zigbuild, glibc pinned 2.17 / 2.19 / 2.22**: links, but the binary
   **segfaults pre-main** — `ld-2.22` faults in *relocation processing of the
   main binary* (zig/lld emits relocations this old loader mishandles). Confirmed
   with a trivial C hello (zig glibc → segfault; zig **musl static** → runs).
3. **musl static**: runs, but can't dynamically load the glibc libMali/libSDL2 →
   useless for GPU. Dead end.
4. **Linaro GCC 4.9-2016.02 (glibc 2.21)**: ✅ trivial hello runs; full ForgeDash
   runs. glibc 2.21 < device 2.22 (forward-compatible) and GNU `ld` emits
   relocations `ld-2.22` accepts. **This is the toolchain.**

Other build specifics (all in `forgedash/dockerbuild.sh`):
- Link the **device's own** `libSDL2.so`/`libMali.so` (untarred from
  `devroot.tar` = device `/usr/lib`+`/lib`) via `-L sysroot/usr/lib`.
- The `sdl2` 0.37 crate references macOS-only `SDL_Metal_DestroyView` (+ friends)
  absent from the 2017 device libSDL2 → satisfied by `sdl_stubs.c` compiled to an
  object and linked in (never called on Linux).
- Linker/rustflags passed as **target-scoped env** (`CARGO_TARGET_ARMV7_..._LINKER`
  / `_RUSTFLAGS`) so host proc-macros aren't poisoned. Cached toolchain image:
  `Dockerfile.build` → `forgedash-build`.

## Controller — raw evdev, not SDL

SDL2's joystick/gamecontroller layer reports **0 joysticks** on this device (it
looks for legacy `/dev/input/jsN`, which doesn't exist; the pad is `event24`).
ForgeDash bypasses SDL input and reads `/dev/input/event24` as raw evdev
(non-blocking, 16-byte `input_event` on 32-bit ARM). Clovercon codes learned
on-device with a tiny `evtest.c`:

| Button | EV_KEY code |
|---|---|
| A (BTN_SOUTH) | 304 |
| B (BTN_EAST) | 305 |
| D-pad **Left** | 704 (BTN_TRIGGER_HAPPY1) |
| D-pad **Right** | 705 (BTN_TRIGGER_HAPPY2) |
| D-pad Up / Down | 706 / 707 (unused — single shelf) |

(`app/src/platform.rs` maps 704→Left, 705→Right, 304→Confirm.)

## Deploy mechanics that worked

- Pull device sysroot from Windows side (`ssh root@169.254.13.37 'tar -cf - -C /
  usr/lib lib' > devroot.tar`; byte-safe via `cmd /c` redirection — PowerShell
  pipes corrupt binaries).
- Push binary/assets with `ssh '... cat > /tmp/forge/<f>'` (dropbear has no
  sftp-server). ROMs copied from stock `/newroot/usr/share/games/nes/kachikachi/`
  into `/tmp/forge/roms/` so relative paths resolve.
- Run via `forge-loop.sh` (shell-loop: ForgeDash → handoff → RA → relaunch).

## Follow-ups (not blockers)

- Productize: drive the memboot GPU+pad bring-up + ForgeDash staging from the
  RetroForge host app (currently manual SSH).
- Device pad enumeration is hardcoded `event24` (override `FORGE_PAD`); later,
  scan `/dev/input/event*` for `ID_INPUT_JOYSTICK`.
- Persistent NAND-install boot path (backup-gated, ADR-0003) — next slices.
- Covers/metadata via Sync data contract (this run used solid-card fallback +
  hand-staged `library.json`).
