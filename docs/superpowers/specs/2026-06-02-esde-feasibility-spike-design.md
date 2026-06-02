# ES-DE Feasibility Spike — Design

**Date:** 2026-06-02
**Status:** design
**Parent initiative:** Custom boot screen + customizable dashboard (deployed via RetroForge).
**This spec covers the dashboard feasibility spike only.** It is the go/no-go
gate in front of the much larger "replace `m2engage` with a ported frontend"
work. No dashboard is built here.

## Goal & boundary

Prove — or kill — the premise that a desktop-class emulator frontend can run on
the Nintendo Classic Mini hardware **before** any dashboard spec is written.

The target device is an Allwinner R16 (4× Cortex-A7), Mali-400 MP2 GPU,
**256MB DDR3 RAM**, 512MB NAND. That RAM ceiling and the Mali-400 GPU are the
whole reason this spike exists: ES-DE (SDL2 + OpenGL/GLES, C++) and Pegasus
(Qt5 + QML) are built for desktops, and the hakchi community avoids them on this
class of hardware. We do not assume the port is viable; we measure it.

**Done = a verdict backed by evidence.** ES-DE launches on a membooted device,
renders its UI to HDMI through the device's Mali-400 GLES blobs, and
`/proc/meminfo` shows whether enough RAM is left to be worth pursuing. The
output is a one-line GO / NO-GO / INCONCLUSIVE call plus the supporting numbers.

### In scope
- Get **one prebuilt armhf ES-DE build** running once on the membooted device.
- Render to HDMI via the device's existing Mali GLES/EGL libraries.
- Read free RAM before and during the run.
- Capture a findings doc with the evidence and the verdict.

### Out of scope
- Pegasus (only spiked if ES-DE is a GO and we later compare).
- Cross-compiling ES-DE (only if the prebuilt path is INCONCLUSIVE — see below).
- Navigation polish, themes beyond the minimum, dummy ROM libraries.
- Actually launching a game from ES-DE into a core.
- Persistence / NAND flash.
- Any RetroForge UI or committed app code.

## Decisions locked in brainstorm

| Question | Decision | Why |
|---|---|---|
| Approach | Feasibility spike before any dashboard spec | Lowest risk of wasted spec work on an unbuildable target |
| Frontend | **ES-DE** first (not Pegasus) | SDL2 + GLES, lighter runtime; if it can't fit, Pegasus can't either — good worst-case probe |
| Success bar | **Renders + free RAM headroom** | Proves the hard part (GPU + memory) fastest; navigation/launch deferred |
| Binary source | **Reuse a prebuilt armhf build** | Cheap probe before committing to a cross-compile toolchain |
| Integration | **Manual + scripted** | Throwaway runbook over the existing SSH path; nothing to maintain |

## Mechanism / data flow

Reuses the proven memboot + SSH-over-RNDIS path already in the app. All steps
are manual/scripted — a runbook plus throwaway shell snippets driven through the
existing SSH command path. No new Rust modules, no UI.

1. **Memboot** the device (existing app / `memboot` command). Device comes up on
   the RNDIS link at `169.254.13.37`, dropbear listening, SSH auth = `none`
   method (empty root password). See memory note `slice3-clovershell-pending-hw`.
2. **Pull GLES deps from the device.** The Mali-400 EGL/GLESv2 blobs are already
   in the stock rootfs (stock `m2engage` uses them). Locate them
   (`libMali.so` / `libEGL.so` / `libGLESv2.so`) and any SDL2 the device carries;
   record their paths for `LD_LIBRARY_PATH`.
3. **Stage ES-DE.** Push the prebuilt armhf binary + a minimal theme/config into
   device tmpfs (`/tmp`, RAM-backed) over SSH/SCP. Expected footprint ~30–40MB,
   fits once `m2engage` is stopped.
   **First action: confirm the build is GLES-linked** — `ldd` / `readelf -d` must
   show `libGLESv2` / `libEGL`, NOT desktop `libGL`. A desktop-GL build cannot
   bind to Mali-400 and is the wrong artifact (RPi / Batocera builds are GLES).
4. **Free the framebuffer + RAM.** Stop the stock UI (`pkill m2engage` / stop the
   `moon-game` init job) so ES-DE owns the display and the RAM baseline is clean.
5. **Baseline RAM.** `cat /proc/meminfo` over SSH; record `MemFree` /
   `MemAvailable`.
6. **Launch ES-DE** over SSH with `LD_LIBRARY_PATH` pointed at the Mali blobs,
   rendering via EGL/GLES to the framebuffer.
7. **Observe + measure.** Photograph the HDMI output; read `/proc/meminfo` again
   while ES-DE runs. The delta is the headroom verdict.

## Verdict criteria

- **GO** — ES-DE renders its UI to HDMI **and** free RAM with ES-DE running
  leaves comfortable headroom (rule of thumb: ≥40–50MB `MemAvailable` on top of
  the running frontend — enough that adding a RetroArch core later won't OOM).
- **NO-GO** — won't bind to Mali GLES, OOM-killed on launch, black screen / no
  render, or renders but leaves near-zero free RAM.
- **INCONCLUSIVE** (escalate, do not kill the initiative) — the prebuilt build
  fails on a libc / dependency mismatch that is plausibly fixable by building
  against the device's own sysroot. This moves the next step to "cross-compile
  ES-DE ourselves," NOT to abandoning the dashboard. Prebuilt failing ≠
  feasibility failing.

## Deliverable

A short findings doc: `docs/superpowers/findings/2026-06-02-esde-spike.md`,
containing:
- `/proc/meminfo` numbers — baseline (m2engage stopped) and with ES-DE running.
- A photo of the HDMI output.
- The exact prebuilt build used and proof of its GLES linkage.
- A one-line **GO / NO-GO / INCONCLUSIVE** call with the recommended next step.

No code is committed to the app; throwaway snippets live in the runbook inside
the findings doc. This spec lives in `docs/superpowers/specs/`.

## Risks & notes

- **Wrong GL flavor** (primary): mitigated by the step-3 GLES linkage check
  before anything else.
- **tmpfs pressure**: 256MB total; pushing ~30–40MB into `/tmp` while the rest of
  the system runs is tight. Stop `m2engage` before staging to free both RAM and
  the framebuffer.
- **libc / SDL2 mismatch**: the most likely prebuilt-path failure; routes to
  INCONCLUSIVE → cross-compile, not NO-GO.
- **Hardware-only**: like both prior features, the real verdict only comes from
  the device — no CI coverage applies to a manual spike.
