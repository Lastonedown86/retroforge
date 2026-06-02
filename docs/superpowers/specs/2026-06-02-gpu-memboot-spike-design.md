# Brick-Safe GPU Memboot Tracer Bullet — Design

**Date:** 2026-06-02
**Status:** design
**Parent initiative:** Custom boot screen + customizable dashboard (deployed via RetroForge).
**This spec covers one tracer bullet:** clearing the GPU wall the RetroArch RGUI
spike hit, while staying 100% RAM-only (zero NAND writes). No dashboard, no
persistence, no boot-ramdisk changes here.

## Why this, and why now

The RetroArch RGUI spike proved the RA binary works on-device but **cannot get a
render surface under a bare RAM-memboot**: every video path needs Mali EGL, and
Mali cannot initialize because the hakchi memboot kernel
(`3.4.113.29-madmonkey`) ships no `mali.ko`, while the NAND `mali.ko` is built
for the stock kernel (`3.4.113`) and `insmod` rejects it (`invalid module
format` / vermagic mismatch). See
`docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md` and memory note
`memboot-no-gpu`.

The user's binding constraint is **no brick risk**. A persistent NAND install
would give RA a matching kernel + GPU module but requires NAND writes (brick-
adjacent, gated by ADR-0003). This spec takes the **RAM-only** route instead:
keep hakchi's memboot kernel (its USB gadget is what provides RNDIS/SSH +
clovershell — confirmed: `/proc/cmdline` has `hakchi-shell`, gadget exposes
`f_rndis`/`f_clover`), and `insmod` the **matching** `mali.ko` + `clovercon.ko`
built for that kernel into RAM. Every action is RAM-only; power-cycle returns the
unit to bone-stock, exactly like the boot-screen and RA spikes (both wrote
nothing).

## Goal & boundary

Prove that with hakchi's matching kernel modules `insmod`'d under the live
memboot, the already-staged RetroArch renders RGUI on HDMI and the controller
navigates it — all RAM-only.

**Done =** under the current hakchi memboot (kernel `3.4.113.29-madmonkey`,
gadget/SSH intact), `mali.ko` + `clovercon.ko` built for that kernel load cleanly
(no vermagic error), RA's `gl` driver obtains a Mali EGL surface, **RGUI renders
on HDMI**, and the **NES controller navigates** the menu.

**Brick-safety (the whole point):** every action is `insmod` into RAM + run from
`/tmp`. **Zero NAND writes.** Power-cycle → bone-stock.

### In scope
- Source `mali.ko` + `clovercon.ko` for kernel `3.4.113.29-madmonkey`.
- Push to tmpfs; `insmod` (clovercon with the `dp-nes` module params).
- Re-run the already-staged RA (`/tmp/ra`) on the `gl` driver.
- Observe RGUI render + controller navigation.
- Findings doc with the verdict.

### Out of scope
- Boot-ramdisk autostart (folding module-load into the memboot flow) — next cycle.
- Playlists, theme, launching a game — the real dashboard, later.
- Any NAND write / persistence.
- Building modules from source (fallback only, if no prebuilt matches).
- Any RetroForge UI or committed app code.

## Decisions locked in brainstorm

| Question | Decision | Why |
|---|---|---|
| Deploy model | **Non-persistent RAM-only memboot** | User priority = no brick risk; RAM-only writes nothing to NAND |
| Keep which kernel | **hakchi memboot kernel** (`3.4.113.29-madmonkey`) | It provides the USB gadget (RNDIS/SSH + clovershell); stock kernel would lose SSH |
| Scope | **GPU tracer bullet** (insmod modules → RA renders + navigates) | Smallest proof the GPU wall is cleared |
| Module source | **Reuse hakchi prebuilt** modules | They exist (hakchi ships them on install); cheap probe before building from source |
| Integration | **Manual + scripted** | Throwaway runbook over the existing SSH path |

## Mechanism / data flow

Builds on the RA spike state (RA already staged at `/tmp/ra`). No new app code.

1. **Source the modules (host-side).** Find `mali.ko` + `clovercon.ko` built for
   kernel `3.4.113.29-madmonkey`. **Critical constraint:** they must match the
   *exact* hakchi kernel build RetroForge memboots, which comes from
   `hakchi-latest.hmod`. Pull the modules from the **same hakchi
   distribution/version** that supplies our `boot.img` — a vermagic *string*
   match is not enough; a different hakchi build with the same version string can
   have incompatible module CRCs/symbols. First action: locate where hakchi ships
   these `.ko` for the membooted kernel and confirm provenance ties to
   `hakchi-latest`.
2. **Verify host-side** (WSL `readelf` / `modinfo`): ARM ELF32 and
   `vermagic = 3.4.113.29-madmonkey ...`. Record it.
3. **Push to tmpfs** via tar-over-ssh into `/tmp/mods/` (RAM, non-persistent).
4. **insmod by full path** (`modprobe` is broken on this image — no
   `modules.dep`; the module tree is `/lib/modules/3.4.113.29-madmonkey` →
   `/lib/modules-ramfs`, storage modules only):
   - `insmod /tmp/mods/mali.ko` → expect `/dev/mali` appears, `lsmod` shows
     `mali`, no `invalid module format`.
   - `insmod /tmp/mods/clovercon.ko module_params=1,195,2,194` (the `dp-nes`
     values from stock `/newroot/etc/init.d/S79clovercon`) → expect
     `/dev/input/event*` appears.
5. **Re-run staged RA on `gl`** (its stock `retroarch.cfg`, `video_driver=gl`):
   `setsid env HOME=/tmp/ra/etc/libretro LD_LIBRARY_PATH=/usr/lib
   /tmp/ra/bin/retroarch -c /tmp/ra/etc/libretro/retroarch.cfg -v </dev/null
   >/tmp/ra.log 2>&1 &`. Expect the Mali EGL surface to succeed (no
   `EGL_BAD_ALLOC`) and RGUI to render.
6. **Observe + navigate.** Photograph RGUI on HDMI; press D-pad / A-B on the
   controller and confirm the menu moves.

### Risks
- **Exact-build match (primary):** if the only downloadable `mali.ko` is from a
  different hakchi build than our `boot.img`, `insmod` may reject or misbehave.
  Mitigation = source modules + boot.img from the same hakchi version; if they
  cannot be matched, route to the build-from-source fallback (not a NO-GO).
- **mali.ko self-sufficiency:** mali may need `ump.ko` or specific `/dev` nodes;
  if `insmod` reports unresolved symbols, source the companion module from the
  same set.
- **Loading a wrong-build module:** worst case is a kernel oops/hang — recovered
  by power-cycle (RAM-only). No NAND risk.

## Verdict criteria

- **GO** — `mali.ko` loads clean, RA `gl` gets a Mali EGL surface, RGUI renders
  on HDMI, controller navigates. GPU wall cleared; brick-safe RAM-only path
  proven → next cycle = boot-ramdisk autostart, then the real dashboard.
- **PARTIAL** — mali loads + RGUI renders, but the controller doesn't (clovercon
  param/wiring). GO on the GPU question; input is the next task. Capture
  `/dev/input` state + RA's joypad log.
- **INCONCLUSIVE → build-from-source** — no downloadable `mali.ko` matches the
  exact memboot kernel build. Not a NO-GO: routes to cross-compiling the modules
  from hakchi's kernel source. Capture what was tried + the vermagic/CRC mismatch.
- **NO-GO** — a correctly-matched `mali.ko` loads but Mali EGL still fails to
  produce a surface under memboot. Means the GPU genuinely can't init in this
  environment; capture the EGL error → escalate.

## Deliverable

A findings doc: `docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md`,
containing:
- Module source + provenance (tie to `hakchi-latest`).
- `modinfo` / vermagic proof for both modules.
- `insmod` results + `lsmod` / `/dev` (mali node, input event) state.
- The RA `gl` video log (EGL success or failure).
- A photo of RGUI on HDMI.
- The controller-navigation result.
- A one-line **GO / PARTIAL / INCONCLUSIVE / NO-GO** call with the next step.

No app code is committed; throwaway snippets live in the findings runbook. This
spec lives in `docs/superpowers/specs/`. Manual/scripted, RAM-only.

## Notes carried from prior spikes (apply during execution)
- dropbear has **no sftp-server** → `scp` fails; use **tar-over-ssh** stdin.
- no `ldd` on device → use `LD_TRACE_LOADED_OBJECTS=1`.
- launch RA with `setsid ... </dev/null` (it grabs the tty and kills SSH).
- **never** `pkill -f /tmp/ra/...` — the pattern matches your own SSH command
  line and SIGKILLs the remote shell (exit 255); use `pidof retroarch`.
- `modprobe` is broken (no `modules.dep`); `insmod` by full path.
- The ClusterM `retroarch-clover` 1.1d binary is already staged at `/tmp/ra` and
  verified working up to video init.
