# ES-DE Feasibility Spike — Findings

> Living document. Task 1 (host-side prebuilt acquisition + GLES-linkage proof) recorded here.
> Later tasks append device measurements (`/proc/meminfo`, render observation) and the final verdict.

Target hardware: Nintendo Classic Mini — Allwinner R16 (4× Cortex-A7, **32-bit ARMv7 / armhf**),
**Mali-400 MP2** GPU (**OpenGL ES 2.0 only**, no desktop OpenGL), **256MB RAM**.

A usable ES-DE build for this device MUST link `libGLESv2` / `libEGL` (the on-device Mali GLES/EGL
blobs). A build that links desktop `libGL` is the wrong artifact and cannot bind to Mali-400.

---

## Build identity

**Candidate evaluated (only official 32-bit ARM ES-DE prebuilt that exists):**

- Project: ES-DE Frontend (EmulationStation Desktop Edition), official GitLab.
- Package: `emulationstation-de-1.2.6-armv7l.deb` (Raspberry Pi OS / Raspbian, armv7l/armhf).
- Version: **1.2.6** (Aug 2022 build date on the binary).
- Source URL: <https://gitlab.com/es-de/emulationstation-de/-/package_files/48239061/download>
- Downloaded to: `%TEMP%\esde-armhf.deb` (36.29 MB, `!<arch>` Debian package magic confirmed).
- Extracted via WSL `dpkg-deb -x` to `%TEMP%\esde-armhf\`.
- Inspected binary: `%TEMP%\esde-armhf\usr\bin\emulationstation` (2,602,444 bytes).

ES-DE versions ≥ 2.x provide **no** official 32-bit ARM package at all — official ARM support is
**AArch64 only**, and the AArch64 AppImage is **desktop-GL** (per ES-DE USERGUIDE: "it does not include
support for the OpenGL ES API so you will need to explicitly tell Mesa to use regular desktop OpenGL
instead"). So 1.2.6-armv7l is the newest official armhf ES-DE prebuilt in existence.

### Candidates ruled out (no GLES armhf ES-DE prebuilt anywhere)

- **ES-DE ≥ 2.x official ARM** — AArch64 only, desktop-GL. Wrong arch and wrong GL.
- **ROCKNIX / Knulli** (ship ES-DE) — builds are **aarch64** (Rockchip RK35xx, Allwinner H700). No
  armv7/armhf target. Wrong arch.
- **muOS** — pure RetroArch, no ES-DE frontend at all. N/A.
- **Batocera** — ships its **own legacy EmulationStation fork** (`batocera-emulationstation`), not
  ES-DE. Different codebase; out of scope for an ES-DE spike.

The Allwinner R16 (armv7 + Mali-400) is essentially the only armv7-GLES target of interest, and no
distribution ships an ES-DE prebuilt for it. The prebuilt path is exhausted.

---

## GLES linkage proof

ELF inspection performed with `readelf` from WSL (Ubuntu, `/usr/bin/readelf`). No `readelf` on the
Windows PATH; WSL binutils used as the inspection tool. The DEB ar/data members were unpacked with
WSL `dpkg-deb -x`.

### `readelf -h` (ELF header) — architecture

```
Class:    ELF32
Data:     2's complement, little endian
OS/ABI:   UNIX - GNU
Type:     EXEC (Executable file)
Machine:  ARM
```

Verdict: **armhf / 32-bit ARM — CORRECT architecture.** (`Class: ELF32`, `Machine: ARM`.)

### `readelf -d` NEEDED shared libraries — GL linkage

```
[libatomic.so.1]
[libcurl-gnutls.so.4]
[libavcodec.so.58]
[libavfilter.so.7]
[libavformat.so.58]
[libavutil.so.56]
[libfreeimage.so.3]
[libfreetype.so.6]
[libpugixml.so.1]
[libSDL2-2.0.so.0]
[libpthread.so.0]
[libasound.so.2]
[libbcm_host.so]        <- Broadcom VideoCore (legacy Raspberry Pi only)
[libvchiq_arm.so]       <- Broadcom VideoCore (legacy Raspberry Pi only)
[libGL.so.1]            <- DESKTOP OpenGL  (WRONG for Mali-400)
[libGLU.so.1]           <- DESKTOP OpenGL utility (WRONG)
[libstdc++.so.6]
[libm.so.6]
[libgcc_s.so.1]
[libc.so.6]
```

GL-related filter:

```
[libSDL2-2.0.so.0]   present  (good — SDL2 is required and present)
[libGL.so.1]         present  (BAD — desktop GL)
[libGLU.so.1]        present  (BAD — desktop GL utility)
```

Required GLES libs **ABSENT**: no `libGLESv2.so`, no `libEGL.so`.

**Verdict on linkage: this prebuilt is DESKTOP-GL, NOT GLES. WRONG ARTIFACT for Mali-400.**
It also depends on Broadcom VideoCore libs (`libbcm_host.so`, `libvchiq_arm.so`) that do not exist on
Allwinner hardware, so it could not load on the device even ignoring the GL profile.

This is a genuine empirical result: the build was downloaded, extracted, and its dynamic section read —
not inferred.

### Prebuilt-path conclusion: INCONCLUSIVE → cross-compile

No GLES-linked armhf ES-DE prebuilt exists anywhere after a genuine search (official + handheld
distros). The only official armhf ES-DE build (1.2.6) is desktop-GL and Broadcom-bound. Per the spike
plan this is the **INCONCLUSIVE → cross-compile** branch, not a failure.

**Recommended next step:** cross-compile ES-DE for armhf with OpenGL ES enabled
(`-DGL_PROFILE=GLES` / `cmake .. -DRPI=on` equivalent, or the ES-DE GLES build flag) against an armhf
sysroot, linking `libGLESv2` + `libEGL` instead of `libGL`. The ES-DE USERGUIDE explicitly endorses
this for SBCs lacking desktop-GL drivers: "it could be a good idea to instead build ES-DE yourself
with OpenGL ES support." The resulting binary's `readelf -d` must show `libGLESv2.so` and/or
`libEGL.so` and `libSDL2`, and MUST NOT show `libGL.so`, before it is worth pushing to the device.

---

## Verdict (pending device run)

PENDING — no device run yet. The prebuilt acquisition path is exhausted (no GLES armhf ES-DE prebuilt
exists); the spike proceeds on the cross-compile branch. The final GO / NO-GO / INCONCLUSIVE verdict
(render + free-RAM headroom on the membooted Classic Mini) will be appended by the device-bound tasks
once a GLES-linked armhf ES-DE binary is produced.
