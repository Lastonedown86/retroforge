# RetroArch RGUI Dashboard Tracer Bullet — Findings

> Records the on-device spike run for
> `docs/superpowers/plans/2026-06-02-retroarch-rgui-dashboard-spike.md`.
> **Verdict: NO-GO for "RGUI over bare RAM-memboot" as specced** — see bottom.
> The blocker is architectural (memboot kernel has no GPU), not RetroArch.

## Device + SSH

- Device: **NES Classic** — confirmed by `/newroot/etc/clover/boardtype` =
  **`dp-nes`**. The `uname` codename `madmonkey` is just hakchi's **memboot
  kernel** label (`3.4.113.29-madmonkey`), not the board — it is the same on NES
  and SNES units. (Earlier "SNES Classic" guess from the codename was wrong; the
  board ID is authoritative.)
- `uname -a` = `Linux madmonkey 3.4.113.29-madmonkey #1 SMP PREEMPT Fri Mar 20
  11:15:28 GMT 2020 armv7l GNU/Linux`.
- Memboot succeeded (hakchi boot screen on HDMI). Device parked at the boot
  screen — **no UI process running** (`m2engage`/clover-* absent from `ps`); the
  membooted system does not auto-start the menu.
- RNDIS link up: host "Ethernet 2" (Samsung Remote NDIS) `169.254.18.2`, device
  `169.254.13.37`. SSH `none` auth. (Device ignores ICMP.)
- **RAM is not a constraint:** `MemTotal 253672 kB`, `MemFree ~197124 kB` even
  with RetroArch loaded. Memory headroom was never the issue.

## RetroArch binary + launch env

RetroArch is **not installed** on this unit (no `retroarch`/`launchRA`/cores in
`/newroot`, `/bin`, `/hakchi`; `/newroot` squashfs is read-only). It was staged
into tmpfs instead.

- **Source:** `ClusterM/retroarch-clover` release `1.1d`,
  `retroarch_with_cores.zip`
  (SHA256 `7B239B11D2C35686DD7778413744199BCB65F4EFC77DBDBD648F1AFD1BC8C659`).
  ClusterM is the hakchi2 author; this is the canonical hakchi RetroArch, armv7
  hard-float, purpose-built for the R16/Mali board (RPATH names `r16_mini`).
- **Staging:** pushed the extracted `ra` tree to device `/tmp/ra` via
  tar-over-ssh (dropbear has **no sftp-server**, so `scp` fails; used
  `tar -xf` from stdin). `chmod +x /tmp/ra/bin/*`.
- **Binary verified on-device:** RetroArch **1.7.0** (Git 7e9945cde9, built Jan
  2018), ARM ELF32. All NEEDED libs resolve via `/usr/lib` (loader trace, no "not
  found"): `libEGL`/`libSDL2`/`libasound`/`libudev` etc. `libEGL.so` →
  `libMali.so`. **The binary itself works** — it loads, parses config, and runs
  up to video init.
- Launch env (from the build's own `bin/retroarch-mini`): `HOME=/etc/libretro`,
  config `/etc/libretro/retroarch.cfg`, cores `/etc/libretro/core/`. Adapted to
  the tmpfs tree: `HOME=/tmp/ra/etc/libretro`, `-c
  /tmp/ra/etc/libretro/retroarch.cfg`. Launched detached with `setsid ... </dev/null`
  (RA grabs the controlling tty otherwise and kills the SSH session).

## Render observation — FAILED on every video path

RetroArch reaches `init_video()` on every driver and cannot obtain a render
surface:

1. **`gl` (default, = Mali GLES):** `[EGL] EGL_BAD_ALLOC` → `[Mali fbdev] EGL
   error 12288` → SDL_GL fallback `Could not initialize EGL` → exits.
2. **`sunxi` (Allwinner 2D display engine, no GPU):** opens `/dev/disp` then dies
   **silently** at "Video @ fullscreen". `/dev/g2d` (the sunxi G2D 2D
   accelerator the driver needs) **does not exist**; `dmesg` shows
   `[DISP] disp_ioctl ... para err`.
3. **`sdl2` (software):** SDL2 **finds the real display** (`Display #0
   1280x720@60`) and lists renderers `opengles2/opengles/software`, but RA fails:
   `Failed to initialize renderer: Couldn't find matching render driver` →
   `Failed to create menu texture: Invalid renderer`. SDL2's window backend on
   this device is the **Mali EGL** backend (strings: "Mali EGL Video Driver",
   "MALI: Can't create EGL surface"); with no working EGL surface there is no
   window for even the software renderer to draw into.

### Root cause (fully traced)

Every video-surface path on this device ultimately depends on **Mali EGL**, and
**Mali cannot initialize under the memboot kernel**:

- `insmod` of the NAND `mali.ko` fails: **`invalid module format`**. `dmesg`:
  `mali: version magic '3.4.113 ...' should be '3.4.113.29-madmonkey ...'`. The
  NAND module is built for the **stock** kernel; the memboot runs hakchi's
  **`3.4.113.29-madmonkey`** kernel.
- No matching `mali.ko` exists anywhere for the memboot kernel. The memboot
  ramdisk's module dir (`/lib/modules/3.4.113.29-madmonkey` → `/lib/modules-ramfs`)
  contains **only `sd_mod.ko` and `nand.ko`**. The hakchi **memboot kernel is a
  storage-only flash/recovery kernel with no GPU driver** — it never needs one,
  because hakchi's memboot does provisioning over USB, not graphics.
- The GPU-free 2D path (`sunxi`/`/dev/disp`) is unavailable too (`/dev/g2d`
  absent).

So under a **bare RAM-memboot**, RetroArch has no path to a render surface.

## Controller navigation — NOT REACHED

Blocked behind render. `clovercon.ko` (controller driver) also cannot load
(same vermagic mismatch as `mali.ko`; the NAND module is for the stock kernel).
RA's `udev` joypad driver found no `/dev/input/event*`. So even input depends on
loading NAND modules the memboot kernel rejects.

## Verdict: NO-GO (for non-persistent RAM-memboot) — redirect, not dead end

**The "customizable dashboard over non-persistent RAM-memboot" model is
fundamentally incompatible with a graphical frontend on this hardware.** The
hakchi memboot kernel ships no GPU (or controller) module and rejects the NAND
modules (vermagic mismatch), so no GPU-accelerated app — RetroArch or anything
else — can obtain a display surface from a bare memboot.

This does **not** contradict the boot-screen feature, which works under memboot
precisely because it is *not* GPU: it pipes a PNG through `decodepng` to the
framebuffer ([[custom-boot-screen]]). The dashboard needs the GPU; the boot
screen does not.

**Confirmed positives (carry forward):**
- The ClusterM armv7 RetroArch binary is correct and runs on the device (loads,
  configures, reaches video init). The ES-DE cross-compile detour is unnecessary.
- RAM is ample (~192MB free with RA loaded). Memory is not a constraint.
- The staging mechanism (tar-over-ssh into tmpfs, `setsid` detached launch) works.

**Recommended next step — choose the deploy model that gives RA a matching
kernel + GPU module:**
1. **Persistent NAND install** (the proven hakchi path): install RetroArch +
   boot from NAND, where the stock kernel matches its `mali.ko`. Drops the
   "non-persistent" constraint for the dashboard. Requires NAND-write tooling
   (gated by the mandatory NAND-backup ADR-0003) — bigger, but how every real
   hakchi RA deployment works.
2. **GPU-capable memboot image**: build/memboot a boot image whose kernel either
   has Mali built-in or ships a matching `mali.ko` (+ `clovercon.ko`) in its
   ramdisk. Keeps non-persistence but is essentially custom-kernel/boot work
   (the kernel track previously parked).
3. (Experiment, low-priority) force-load the NAND `mali.ko` with vermagic
   override — risky (ABI mismatch can hang the kernel), busybox `insmod` has no
   `--force`; would need a patched module. Not worth it vs option 1/2.

## Appendix — key commands (runbook)

```sh
# stage (host): tar -cf ra.tar --format ustar -C <ra> . ; ssh DEV 'cat >/tmp/ra.tar' < ra.tar
ssh DEV 'mkdir -p /tmp/ra && tar -xf /tmp/ra.tar -C /tmp/ra && chmod +x /tmp/ra/bin/*'
# dep check (no ldd on device):
LD_TRACE_LOADED_OBJECTS=1 LD_LIBRARY_PATH=/usr/lib /tmp/ra/bin/retroarch
# launch (detached; kill via pidof, NOT pkill -f <path> which matches your own ssh cmd):
setsid env HOME=/tmp/ra/etc/libretro LD_LIBRARY_PATH=/usr/lib /tmp/ra/bin/retroarch \
  -c /tmp/ra/etc/libretro/retroarch.cfg -v </dev/null >/tmp/ra.log 2>&1 &
# GPU module load attempt (fails, vermagic):
insmod /newroot/lib/modules/3.4.113/extra/mali.ko   # invalid module format
```
