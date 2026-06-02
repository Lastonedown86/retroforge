# ES-DE Feasibility Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **This is a manual/scripted hardware spike, not a code feature.** There is no pytest/cargo TDD loop. "Tests" here are on-device observations with exact commands and expected output. The only committed artifact is the findings doc. Most steps run against a physical device over SSH — a worker without the hardware in hand executes as far as it can and hands the device-bound steps back to the user.

**Goal:** Produce an evidence-backed GO / NO-GO / INCONCLUSIVE verdict on whether ES-DE can run on the Nintendo Classic Mini (Allwinner R16, Mali-400, 256MB RAM), by running a prebuilt armhf ES-DE on the membooted device and measuring render + free RAM.

**Architecture:** Reuse the proven memboot + SSH-over-RNDIS path. Acquire a GLES-linked prebuilt armhf ES-DE on the host, push it into device tmpfs over SSH, stop the stock `m2engage` UI, launch ES-DE against the device's own Mali EGL/GLES blobs, and read `/proc/meminfo` before/while running. No app code committed; throwaway snippets live in the findings doc.

**Tech Stack:** Existing RetroForge memboot (FEL) + netshell (russh SSH `none` auth, device at `169.254.13.37`). Host-side: `gh`/`curl` to fetch the build, `readelf`/`ldd` to verify GLES linkage, `scp`/`ssh` for transfer.

---

## Spec reference

Design: `docs/superpowers/specs/2026-06-02-esde-feasibility-spike-design.md`. Read it first — it holds the locked decisions and verdict criteria this plan executes.

## Preconditions (verify before Task 1)

- [ ] Device on hand, USB-FEL capable. FEL = USB `1F3A:EFE8`, rusb needs WinUSB bound via Zadig (reverts to libusb0 each session — re-bind, verify `DEVPKEY_Device_Service` = `WinUSB`). See memory `fel-hardware-constraints`.
- [ ] RNDIS host NIC works: after memboot, gadget = `04E8:6863`; host needs generic RNDIS NIC driver + (one-time) reboot to clear ghost miniports. See memory `slice3-clovershell-pending-hw`.
- [ ] RetroForge builds + runs: `npm run tauri dev` (vite :1420). Kill stale instances first (`Stop-Process -Name retroforge -Force`, free :1420).

## File structure

- **Create:** `docs/superpowers/findings/2026-06-02-esde-spike.md` — the deliverable. Holds the build identity, GLES-linkage proof, baseline + running `/proc/meminfo`, HDMI photo reference, and the one-line verdict. Filled progressively across tasks.
- **No app source changes.** All shell snippets are throwaway and recorded inline in the findings doc as they are run.

---

### Task 1: Acquire a GLES-linked prebuilt armhf ES-DE build (host-side)

No device needed. Goal: a downloaded ES-DE armhf binary whose dynamic deps show `libGLESv2`/`libEGL`, not desktop `libGL`. A desktop-GL build is the wrong artifact and cannot bind to Mali-400.

**Files:**
- Create: `docs/superpowers/findings/2026-06-02-esde-spike.md` (start it here)

- [ ] **Step 1: Identify candidate builds**

Prefer builds that target Mali/embedded GLES. Best candidates, in order:
1. **Batocera / ArkOS / muOS** ES-DE armhf packages (these target Allwinner/Rockchip handhelds with Mali GPUs — GLES by construction).
2. ES-DE official ARM AppImage/tarball **only if** it has an `armv7l`/`armhf` GLES variant (the desktop x86 AppImage is GL — wrong).

Record the exact source URL + version chosen in the findings doc.

- [ ] **Step 2: Download the build**

```powershell
# Example shape — substitute the real URL chosen in Step 1
curl -L -o $env:TEMP\esde-armhf.tar.gz "<BUILD_URL>"
```

Expected: a downloaded archive, non-zero size. Extract to a known host dir (e.g. `$env:TEMP\esde-armhf\`).

- [ ] **Step 3: Verify the binary is armhf + GLES-linked**

Use `readelf` (from WSL/git-bash, or `arm-none-eabi`/llvm tools). On the extracted ES-DE binary:

```bash
readelf -h <path>/emulationstation   # or es-de
readelf -d <path>/emulationstation | grep -iE 'NEEDED'
```

Expected:
- `readelf -h` → `Machine: ARM`, `Class: ELF32`.
- `NEEDED` list contains `libGLESv2.so` and/or `libEGL.so` and `libSDL2`. It MUST NOT depend on `libGL.so` (desktop GL).

If it depends on `libGL.so` → wrong build, return to Step 1. If no GLES-linked prebuilt exists at all → this is the **INCONCLUSIVE → cross-compile** branch; stop the spike and report that the prebuilt path is exhausted (do not call NO-GO).

- [ ] **Step 4: Record findings**

In `docs/superpowers/findings/2026-06-02-esde-spike.md`, write the build source URL, version, `readelf -h` machine/class, and the `NEEDED` list. This is the GLES-linkage proof required by the deliverable.

- [ ] **Step 5: Commit**

```powershell
git add docs/superpowers/findings/2026-06-02-esde-spike.md
git commit -m "spike(esde): record prebuilt armhf build + GLES linkage proof"
```

---

### Task 2: Memboot the device and confirm SSH

Device required from here on.

- [ ] **Step 1: Memboot**

Launch RetroForge (`npm run tauri dev`), trigger `memboot` from the UI. Watch the `memboot-progress` events to completion.

Expected: memboot completes; device reboots into the RAM image and brings up RNDIS.

- [ ] **Step 2: Confirm the device is reachable on the link**

```powershell
Test-NetConnection 169.254.13.37 -Port 22
```

Expected: `TcpTestSucceeded : True`. (The device ignores ICMP — do not rely on ping.)

- [ ] **Step 3: Confirm SSH exec works (none auth)**

```powershell
ssh -o StrictHostKeyChecking=no -o PreferredAuthentications=none -o PubkeyAuthentication=no root@169.254.13.37 "uname -a"
```

Expected: prints `Linux clover ...` (kernel banner). If `none` is rejected, the device daemon differs from the verified config — note it and fall back to empty-password (`ssh ... ` then blank password).

Record reachability + `uname -a` in the findings doc.

---

### Task 3: Locate the device's Mali GLES/EGL blobs and SDL2

The stock `m2engage` uses Mali GLES, so the blobs exist in the device rootfs. Find them and the runtime loader path.

- [ ] **Step 1: Find the GL/EGL libraries on the device**

```powershell
ssh root@169.254.13.37 "find / -xdev -name 'libMali*' -o -name 'libEGL*' -o -name 'libGLESv2*' 2>/dev/null"
```

Expected: one or more paths (commonly under `/usr/lib`). Record them.

- [ ] **Step 2: Check for SDL2 on the device**

```powershell
ssh root@169.254.13.37 "find / -xdev -name 'libSDL2*' 2>/dev/null; ls -la /usr/lib | grep -iE 'sdl|egl|gles|mali'"
```

Expected: note whether `libSDL2` is present. If absent, SDL2 must be bundled alongside the ES-DE binary in Task 4 (pull an armhf `libSDL2` matching the build's `NEEDED` version).

- [ ] **Step 3: Record the LD_LIBRARY_PATH plan**

In the findings doc, write the directory(ies) holding the Mali blobs — this becomes `LD_LIBRARY_PATH` at launch in Task 6.

---

### Task 4: Stage ES-DE into device tmpfs

`/tmp` is RAM-backed. Keep the payload lean (~30–40MB target).

- [ ] **Step 1: Create the staging dir on device**

```powershell
ssh root@169.254.13.37 "mkdir -p /tmp/esde && echo staged"
```

Expected: prints `staged`.

- [ ] **Step 2: Push the binary + minimal assets**

```powershell
scp -r $env:TEMP\esde-armhf\* root@169.254.13.37:/tmp/esde/
```

Push: the ES-DE binary, any bundled `libSDL2` (if device lacked it, per Task 3 Step 2), and a **minimal** theme/config (no large media). Skip bundled desktop GL libs.

Expected: transfer completes. Verify size headroom:

```powershell
ssh root@169.254.13.37 "du -sh /tmp/esde; df -h /tmp"
```

Expected: `/tmp` usage well under available — leave RAM for the run. If `/tmp` fills, trim assets.

- [ ] **Step 3: Make the binary executable + smoke-check linkage on-device**

```powershell
ssh root@169.254.13.37 "chmod +x /tmp/esde/emulationstation; ldd /tmp/esde/emulationstation 2>&1 | grep -iE 'not found|GLES|EGL|SDL'"
```

Expected: no `not found` lines. Every `NEEDED` lib resolves (Mali blobs via the Task 3 path, SDL2 from device or bundle). Any `not found` → resolve before launching (bundle the missing lib or point `LD_LIBRARY_PATH` at it).

Record the `ldd` result in the findings doc.

---

### Task 5: Stop the stock UI and capture baseline RAM

Frees the framebuffer and gives a clean RAM baseline.

- [ ] **Step 1: Stop m2engage**

```powershell
ssh root@169.254.13.37 "pkill -f m2engage; pkill -f moon-game; sleep 1; pgrep -f m2engage || echo stopped"
```

Expected: prints `stopped` (no m2engage process). The HDMI screen goes blank/idle — the framebuffer is now free.

- [ ] **Step 2: Baseline /proc/meminfo**

```powershell
ssh root@169.254.13.37 "grep -E 'MemTotal|MemFree|MemAvailable' /proc/meminfo"
```

Expected: `MemTotal` ≈ 256MB; record `MemFree` and `MemAvailable` as the **baseline** (m2engage stopped, ES-DE not yet running). Write both into the findings doc.

---

### Task 6: Launch ES-DE and observe render

The core feasibility moment.

- [ ] **Step 1: Launch with LD_LIBRARY_PATH pointed at Mali blobs**

```powershell
ssh root@169.254.13.37 "cd /tmp/esde && LD_LIBRARY_PATH=<MALI_DIR>:/tmp/esde ./emulationstation --no-splash 2>&1 | head -80"
```

Substitute `<MALI_DIR>` from Task 3. Keep the SSH session open so the process keeps running (or background it with `&` and tail the log).

Expected (GO direction): ES-DE initializes EGL/GLES, opens a window/framebuffer, and the **HDMI output shows the ES-DE UI**. Log shows GL context creation without fatal errors.

Failure signatures to record verbatim if seen:
- `Killed` / OOM in dmesg (`ssh ... dmesg | tail`) → memory NO-GO.
- EGL/GLES init error, `eglInitialize failed`, `Could not create GL context` → Mali bind NO-GO.
- Black screen, no log progress → render NO-GO.
- `libX...: not found` / `undefined symbol` → likely libc/SDL mismatch → **INCONCLUSIVE → cross-compile**.

- [ ] **Step 2: Photograph the HDMI output**

Take a photo of the screen showing the ES-DE UI (or the failure state). Save the reference/filename in the findings doc.

---

### Task 7: Measure running RAM

- [ ] **Step 1: Read /proc/meminfo while ES-DE runs**

From a second SSH session (leave ES-DE running):

```powershell
ssh root@169.254.13.37 "grep -E 'MemFree|MemAvailable' /proc/meminfo; cat /proc/$(pgrep -f emulationstation)/status | grep -E 'VmRSS'"
```

Expected: record `MemFree`/`MemAvailable` with ES-DE running, plus the ES-DE process `VmRSS`. The delta vs the Task 5 baseline is ES-DE's real footprint; the running `MemAvailable` is the headroom number the verdict hinges on.

- [ ] **Step 2: Record the running numbers**

Write baseline vs running `MemAvailable` and ES-DE `VmRSS` into the findings doc side by side.

---

### Task 8: Write the verdict and finalize the findings doc

- [ ] **Step 1: Apply the verdict criteria**

From the spec:
- **GO** — ES-DE rendered to HDMI **and** running `MemAvailable` ≥ ~40–50MB headroom on top of the frontend.
- **NO-GO** — no Mali bind / OOM / black screen / near-zero free RAM.
- **INCONCLUSIVE** — prebuilt failed on libc/dep/SDL mismatch fixable by cross-compiling against the device sysroot. Next step = cross-compile, not abandon.

- [ ] **Step 2: Complete the findings doc**

Ensure `docs/superpowers/findings/2026-06-02-esde-spike.md` contains: build source + version, `readelf`/`ldd` GLES-linkage proof, baseline `/proc/meminfo`, running `/proc/meminfo` + `VmRSS`, HDMI photo reference, and a **one-line GO / NO-GO / INCONCLUSIVE call with the recommended next step**.

- [ ] **Step 3: Commit**

```powershell
git add docs/superpowers/findings/2026-06-02-esde-spike.md
git commit -m "spike(esde): feasibility findings + GO/NO-GO verdict"
```

- [ ] **Step 4: Restore the device (cleanup)**

The spike is non-persistent — a normal power-cycle returns the device to stock (memboot wrote nothing to NAND). Power-cycle to confirm clean stock boot. Note confirmation in the findings doc.

---

## Self-review

- **Spec coverage:** Goal/boundary → Tasks 1–8. Mechanism steps 1–7 → Tasks 2(memboot+ssh), 3(pull blobs), 4(stage), 5(stop UI+baseline), 6(launch), 7(measure). Verdict criteria → Task 8. Deliverable (build id, GLES proof, both meminfo reads, photo, one-line call) → Tasks 1,5,6,7,8 all write into the one findings doc. GLES-linkage-first risk → Task 1 Step 3 + Task 4 Step 3. No spec requirement left unmapped.
- **Placeholders:** `<BUILD_URL>`, `<MALI_DIR>` are deliberate runtime substitutions (the URL is chosen in Task 1 Step 1, the dir discovered in Task 3) — flagged at point of use, not vague TODOs. No "add error handling"-class gaps.
- **Consistency:** binary referred to as `emulationstation` (with `es-de` noted as the alt name in Task 1 Step 3); device IP `169.254.13.37`, staging `/tmp/esde`, findings path `docs/superpowers/findings/2026-06-02-esde-spike.md` used identically throughout.
