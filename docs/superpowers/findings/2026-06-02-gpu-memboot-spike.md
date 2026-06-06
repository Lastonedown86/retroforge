# Brick-Safe GPU Memboot Tracer Bullet — Findings

> Records the full spike for
> `docs/superpowers/specs/2026-06-02-gpu-memboot-spike-design.md`.
> **Verdict: PARTIAL — GPU GO, controller is the next task.** vermagic-matched
> `mali.ko` (+ `clovercon.ko`, `input-polldev.ko`, `evdev.ko`) from the same
> `hakchi-latest.hmod` that supplies our `boot.img` were `insmod`'d under the
> live RAM-memboot; RetroArch renders the RGUI menu on HDMI via Mali-400 OpenGL
> ES 2.0 (user-confirmed). The prior spike's GPU wall (`memboot-no-gpu`) is
> cleared, brick-safe (zero NAND writes). Controller input does not yet work
> (clovercon probes but the I2C pad emits no events — needs board/MCU init).

## Module source + provenance

The gold-standard provenance: the modules, the boot image, and the hakchi
version stamp all live in **one archive** — the very file RetroForge downloads
at runtime.

- **Archive (= our boot.img source):**
  `https://hakchi.net/hakchi/hmods/hakchi-latest.hmod`
  - This is the exact URL RetroForge uses. See
    `src-tauri/src/device/hmod.rs` (`HMOD_URL`) and
    `src-tauri/src/device/blobs.rs` (`boot_img()` →
    `hmod::extract_entry(.., "boot/boot.img")`).
  - Format: gzip-compressed tar (magic `1f 8b`), matching the sniff in
    `hmod::extract_entry`.
  - Downloaded `2026-06-02`; `9 340 382` bytes;
    SHA256 `003994B46504959895F40B0B9E91B16951EF4A638FEB6312314C8FC4FDBFF592`.

- **Version stamp inside the archive** (`var/version`):
  ```
  bootVersion='1.0.3'
  hakchiVersion='v1.0.4-126'
  kernelVersion='3.4.113.29-madmonkey'
  ```
  So `hakchi-latest` currently resolves to **hakchi (ClusterM) `v1.0.4-126`**,
  kernel **`3.4.113.29-madmonkey`** — the exact kernel our memboot runs.

- **Modules inside the SAME archive** (all dated `2020-03-20 07:17`, same build
  batch as the `boot/boot.img` and `boot/uboot.bin`):
  | path in hmod | size |
  |---|---|
  | `lib/modules/3.4.113.29-madmonkey/extra/mali.ko` | 156 508 |
  | `lib/modules/3.4.113.29-madmonkey/extra/clovercon.ko` | 15 828 |
  | `lib/modules/3.4.113.29-madmonkey/kernel/drivers/input/input-polldev.ko` | 5 136 |

- **Provenance tie is mechanical, not just a string match.** The hmod's own
  `install` script copies `./lib/modules/` onto the rootfs **and** memboots
  `./boot/boot.img` in the same run — i.e. hakchi ships these `.ko` *as the
  module set for this exact boot.img/kernel*. Because they came out of the same
  `hakchi-latest.hmod` we fetch, the symbol CRCs are guaranteed to match the
  membooted kernel (no "same version string, different commit" risk). This is
  the best-case branch in the spec's module-source decision.

- **Companion modules:**
  - `mali.ko` — `depends:` is **empty**. UMP is built into this Utgard build;
    **no separate `ump.ko` exists anywhere in the archive** (the only "ump" hit
    is `ums-jumpshot.ko`, an unrelated USB-storage module). mali loads
    standalone.
  - `clovercon.ko` — `depends: input-polldev`. `input-polldev` is **NOT**
    built into the kernel (absent from `modules.builtin`); it ships as a
    separate `.ko` in the same tree. So the real companion in this set is
    **`input-polldev.ko`**, which must be `insmod`'d **before** `clovercon.ko`.
    It was extracted, verified, and staged alongside the others.

## Vermagic proof

Verified in WSL Ubuntu (`kmod` v31 `modinfo`, GNU `readelf` 2.42). Two
independent methods agree. Run against the **staged copies in `%TEMP%\mods\`**.

### `modinfo` (kmod v31)

```
===== mali.ko =====
filename:  .../mods/mali.ko
depends:
vermagic:  3.4.113.29-madmonkey SMP preempt mod_unload ARMv7

===== clovercon.ko =====
filename:  .../mods/clovercon.ko
depends:   input-polldev
vermagic:  3.4.113.29-madmonkey SMP preempt mod_unload ARMv7

===== input-polldev.ko =====
filename:  .../mods/input-polldev.ko
depends:
vermagic:  3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
```

### `readelf -h` (arch / class)

All three modules:
```
Class:    ELF32
Data:     2's complement, little endian
Type:     REL (Relocatable file)
Machine:  ARM
```

### `readelf -p .modinfo` (cross-check of the vermagic string)

```
mali.ko          vermagic=3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
clovercon.ko     depends=input-polldev
                 vermagic=3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
input-polldev.ko vermagic=3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
```

**Result:** all three show `vermagic` **exactly** `3.4.113.29-madmonkey SMP
preempt mod_unload ARMv7`, `Class: ELF32`, `Machine: ARM`. Required criteria met.

### Staged files (`%TEMP%\mods\`)

| file | bytes | SHA256 |
|---|---|---|
| `mali.ko` | 156 508 | `FD33317B37E0CD495541029AD6534B0E8759E6E1E41D67CC2393D4DCFB0C764C` |
| `clovercon.ko` | 15 828 | `D3FB3AEA3E4FE6924CB58A91E30FF36E05D67DA7F3CF918D6266E852BAFB0A41` |
| `input-polldev.ko` | 5 136 | `BF402B7E2F469ED9D9493DF6266D38DCCC522146F8F8309A908DD2DEFF515393` |

`mali` parm of note for later: `mali_dedicated_mem_start` / `mali_dedicated_mem_size`
(stock dp-nes uses dedicated GPU mem). `clovercon` takes `module_params=` array
(the `con0_i2c_bus, con0_detect_gpio, ...` form) — stock dp-nes values come from
`/newroot/etc/init.d/S79clovercon`.

## insmod result — ALL MODULES LOADED CLEAN

Pushed `%TEMP%\mods\*.ko` → `/tmp/mods/` via tar-over-ssh. `insmod` by full path
(load order `input-polldev` → `mali` → `clovercon`):

```
insmod /tmp/mods/input-polldev.ko   → POLLDEV_OK
insmod /tmp/mods/mali.ko            → MALI_OK
insmod /tmp/mods/clovercon.ko module_params=1,195,2,194  → CC_OK   (dp-nes values)
```

`lsmod`:
```
clovercon        8805  0
mali           115993  0
input_polldev    2042  1 clovercon
```

dmesg (the win — contrast the prior spike's `invalid module format`):
```
Get mali parameter successfully
Init Mali gpu successfully
Mali: Mali device driver loaded
added device for controller 1
input: Nintendo Clovercon - controller1 as /devices/platform/twi.1/i2c-1/1-0052/input/input2
probed controller 1
```

`/dev/mali` (char 10,52) created. **No vermagic error** — the same-archive
modules matched the running kernel exactly, as predicted.

**Extra module the plan didn't anticipate — `evdev.ko`.** clovercon registered an
input device (`input2`) but `/proc/bus/input/handlers` had only `kbd` +
`ddrfreq_dsm` — **no `evdev` handler**, so no `/dev/input/event*` node was
created and RetroArch's joypad driver saw nothing. `evdev` is a loadable module
in the same hmod (`kernel/drivers/input/evdev.ko`, vermagic-matched). After
`insmod /tmp/mods/evdev.ko`: handler `evdev Minor=64` appeared and
`/dev/input/event0,1,24` were created — the Clovercon maps to **`event24`**
(`/proc/bus/input/devices`: `Handlers=event24`). (The full hmod input tree also
ships `joydev.ko`? no — but `mousedev.ko`, `uinput.ko`, `joystick/xpad.ko`,
`ff-memless.ko` etc. are there if ever needed.)

## RA gl render — GO (renders on HDMI via Mali GLES2)

Re-ran staged RA (`/tmp/ra`) on stock `video_driver=gl`. The Mali EGL surface
now succeeds — `EGL_BAD_ALLOC` is gone:

```
[EGL]: EGL version: 1.4
[GL]: Found GL context: mali-fbdev
[GL]: Vendor: ARM, Renderer: Mali-400 MP
[GL]: Version: OpenGL ES 2.0
[GL]: Using resolution 1280x720
[Shader driver]: Using GLSL shader backend
[EGL]: eglSwapInterval(1)         ← actively swapping frames
```

**User confirmed on HDMI: the RetroArch RGUI menu is displayed.** The GPU wall
from the prior spike (`memboot-no-gpu`) is **cleared** under a brick-safe,
RAM-only memboot.

## Controller — NOT WORKING (kernel-level dead; needs board init)

clovercon loads and `probed controller 1`, the pad maps to `event24`, RA was
switched to the `linuxraw` joypad driver (the `udev` driver needs `udevd`, which
this memboot lacks). But:

- Raw capture: `timeout 20 cat /dev/input/event24` while the user mashed every
  button → **0 bytes**. The controller emits nothing at the kernel level.
- dmesg: `twi1 has no twi_regulator`; `/dev/i2c*` absent (no `i2c-dev`);
  **`clover-mcp` present (`/newroot/usr/bin/clover-mcp`) but NOT running.**

The NES Classic pad is an I2C (Wii-style) device at `i2c-1/1-0052`. clovercon
probes it, but the port delivers no data — the bare memboot skips the board
bring-up stock boot does (notably **`clover-mcp`**, the onboard-MCU manager that
powers/enables the controller ports, and the twi1 regulator). This is a deeper
hardware-init gap, not an RA-config issue.

## Verdict — PARTIAL (GPU **GO**; controller is the next task)

**The spike's core question — can a graphical RetroArch run under a brick-safe,
non-persistent RAM-memboot? — is answered YES.** `mali.ko` + companions
`insmod`'d from the same `hakchi-latest.hmod` clear the GPU wall; RA renders the
RGUI menu on HDMI via Mali-400 OpenGL ES 2.0. **Zero NAND writes**; power-cycle
returns the unit to bone-stock. This unblocks the dashboard initiative on the
RAM-only path the user requires.

**PARTIAL** only because controller input does not yet work: clovercon (+ evdev)
load and the device probes, but the I2C pad emits no events — the memboot is
missing stock boot's port/MCU bring-up.

**Recommended next task (its own cycle):** controller bring-up under memboot —
investigate running/replicating `clover-mcp` (MCU port power) and the twi1
regulator init so `event24` actually emits, then re-test RGUI navigation. All
still RAM-only / brick-safe.

**Module set proven loadable under memboot (RAM-only):** `input-polldev.ko`,
`mali.ko`, `clovercon.ko` (params `1,195,2,194` for dp-nes), `evdev.ko` — all
vermagic `3.4.113.29-madmonkey`, all from `hakchi-latest.hmod`.

### Appendix — working device sequence (RAM-only, brick-safe)
```sh
# (modules staged at /tmp/mods via tar-over-ssh; RA staged at /tmp/ra)
insmod /tmp/mods/input-polldev.ko
insmod /tmp/mods/mali.ko
insmod /tmp/mods/clovercon.ko module_params=1,195,2,194
insmod /tmp/mods/evdev.ko                      # creates /dev/input/event* (Clovercon=event24)
setsid env HOME=/tmp/ra/etc/libretro LD_LIBRARY_PATH=/usr/lib \
  /tmp/ra/bin/retroarch -c /tmp/ra/etc/libretro/retroarch.cfg \
  --appendconfig /tmp/ra-input.cfg -v </dev/null >/tmp/ra.log 2>&1 &
#   /tmp/ra-input.cfg: input_joypad_driver = "linuxraw"  (udevd absent)
# → RGUI renders via Mali GLES2 @1280x720. Controller: event24 emits 0 bytes (next task).
```
