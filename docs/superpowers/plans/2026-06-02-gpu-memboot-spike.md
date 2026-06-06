# Brick-Safe GPU Memboot Tracer Bullet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **This is a manual/scripted hardware spike, not a code feature.** No pytest/cargo TDD loop. "Tests" are on-device observations with exact commands + expected output. The only committed artifact is the findings doc. **Task 1 is host-side and autonomous** (find + verify kernel modules); **Tasks 2–8 require the physical device** + HDMI + NES controller on the bench. Everything is RAM-only — zero NAND writes.

**Goal:** Prove that hakchi's matching kernel modules (`mali.ko` + `clovercon.ko` for `3.4.113.29-madmonkey`), `insmod`'d under the live memboot, let the already-staged RetroArch render RGUI on HDMI and be navigated by the controller — all RAM-only, brick-safe.

**Architecture:** Keep hakchi's memboot kernel (its USB gadget provides RNDIS/SSH). Source the two `.ko` files built for that exact kernel from the same hakchi distribution that supplies our `boot.img` (`hakchi-latest`), push them to tmpfs, `insmod` by full path, re-run the staged RA (`/tmp/ra`) on the `gl` (Mali EGL) driver, observe render + controller navigation. No app code committed.

**Tech Stack:** RetroForge memboot (FEL) + network shell (SSH `none` auth, device `169.254.13.37`). On-device: hakchi memboot kernel `3.4.113.29-madmonkey`, busybox `insmod`. Host: WSL `readelf`/`modinfo` for module verification, `tar`+`ssh` for transfer.

---

## Spec reference

Design: `docs/superpowers/specs/2026-06-02-gpu-memboot-spike-design.md`. Read it first — locked decisions + verdict criteria. Predecessor findings: `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md` (why the GPU wall exists; RA already staged + verified working to video init). Memory note `memboot-no-gpu`.

## Preconditions

- [ ] Device on hand, FEL-capable (USB `1F3A:EFE8`, WinUSB via Zadig — verify `DEVPKEY_Device_Service = WinUSB`). See memory `fel-hardware-constraints`.
- [ ] RNDIS host NIC works after memboot (`04E8:6863`; generic RNDIS NIC + one-time reboot to clear ghost miniports). See memory `slice3-clovershell-pending-hw`.
- [ ] NES controller plugged into the console (Task 7).
- [ ] RetroForge runs (`npm run tauri dev`, vite :1420; kill stale `retroforge.exe` + free :1420 first).
- [ ] The staged RA tree from the RA spike: host `%TEMP%\retroarch-hmod\extracted\ra\` (ClusterM `retroarch-clover` 1.1d). If absent, re-fetch per `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md`.

## Runbook gotchas (apply throughout)

- dropbear has **no sftp-server** → `scp` fails; transfer via **tar-over-ssh stdin**.
- no `ldd` → use `LD_TRACE_LOADED_OBJECTS=1`.
- launch RA with `setsid ... </dev/null` (it grabs the tty and kills SSH otherwise).
- **never** `pkill -f /tmp/ra/...` — the pattern matches your own SSH command line and SIGKILLs the remote shell (exit 255). Use `pidof retroarch`.
- `modprobe` is broken (no `modules.dep`); `insmod` by **full path**.
- SSH invocation used below (`$SSH`): `ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL -o PreferredAuthentications=none -o PubkeyAuthentication=no -o ConnectTimeout=10 root@169.254.13.37`. For multi-line remote scripts, write a local `.sh` (LF endings) and pipe via `cmd /c "ssh <opts> root@169.254.13.37 sh -s < script.sh"` (avoids PowerShell mangling).

## File structure

- **Create:** `docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md` — the deliverable. Module source/provenance, `modinfo` vermagic proof, `insmod`/`lsmod`/`/dev` state, RA `gl` video log, HDMI photo ref, controller result, verdict.
- **No app source changes.** Throwaway snippets recorded inline in the findings doc.

---

### Task 1: Source + verify the kernel modules (HOST-SIDE, autonomous)

No device needed. Produce verified `mali.ko` + `clovercon.ko` whose vermagic is exactly `3.4.113.29-madmonkey`, provenance tied to `hakchi-latest`.

**Files:**
- Create: `docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md` (start it here)

- [ ] **Step 1: Confirm our memboot kernel's exact build identity**

The modules must match the kernel RetroForge memboots, which comes from `hakchi-latest.hmod`. Determine the hakchi version RetroForge fetches: inspect `src-tauri/src/device/hmod.rs` (`HMOD_URL = https://hakchi.net/hakchi/hmods/hakchi-latest.hmod`) and `blobs.rs` for how `boot.img` is extracted. Record the hakchi version/source that supplies our `boot.img`.

- [ ] **Step 2: Locate matching modules in the hakchi distribution**

Research (web + GitHub) where hakchi ships `mali.ko` + `clovercon.ko` for the `3.4.113.29-madmonkey` kernel. Candidates, in order:
1. The **same `hakchi-latest.hmod`** (or the hakchi2-CE kernel package it derives from) — the modules hakchi installs to NAND alongside its kernel. Best: provenance matches our boot.img exactly.
2. hakchi2-CE distribution data (ClusterM/hakchi2 repo `mod/` / kernel artifacts) for the madmonkey kernel.
3. A hakchi "mali" / kernel-modules hmod if one is published.

Download. Record exact source URL + how it ties to `hakchi-latest`.

- [ ] **Step 3: Verify vermagic + arch (WSL)**

`readelf`/`modinfo` are not on the Windows PATH; use WSL.

```bash
wsl modinfo /mnt/c/.../mali.ko       # or: wsl readelf -p .modinfo mali.ko
wsl modinfo /mnt/c/.../clovercon.ko
wsl readelf -h /mnt/c/.../mali.ko | grep -E 'Class|Machine'
```

Expected:
- `mali.ko` + `clovercon.ko` → `vermagic: 3.4.113.29-madmonkey SMP preempt mod_unload ARMv7`.
- `readelf -h` → `Class: ELF32`, `Machine: ARM`.

If vermagic differs from `3.4.113.29-madmonkey` → wrong build; return to Step 2. If no module with that exact vermagic is downloadable → this is the **INCONCLUSIVE → build-from-source** branch; report it (do not call NO-GO).

- [ ] **Step 4: Record findings**

In `docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md`, write sections `## Module source + provenance` (URLs, tie to hakchi-latest), `## Vermagic proof` (the `modinfo`/`readelf` output), and placeholders `## insmod result`, `## RA gl render`, `## Controller`, `## Verdict (pending device)`.

- [ ] **Step 5: Commit**

```powershell
git add docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md
git commit -m "spike(gpu): record hakchi kernel modules + vermagic proof"
```

---

### Task 2: Memboot the device + confirm SSH + ensure RA staged

Device required from here.

- [ ] **Step 1: Memboot + reach SSH**

Trigger `memboot` in RetroForge; wait for Success. Then:

```powershell
Test-NetConnection 169.254.13.37 -Port 22 -WarningAction SilentlyContinue
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL -o PreferredAuthentications=none -o PubkeyAuthentication=no root@169.254.13.37 "uname -r"
```

Expected: `TcpTestSucceeded : True` (after a short boot settle); `uname -r` = `3.4.113.29-madmonkey` (must match the module vermagic from Task 1).

- [ ] **Step 2: Ensure RA is staged at /tmp/ra (re-push if power-cycled)**

```powershell
ssh ... root@169.254.13.37 "ls /tmp/ra/bin/retroarch 2>/dev/null && echo PRESENT || echo MISSING"
```

If `MISSING`, re-stage (tmpfs was cleared by power-cycle):

```powershell
tar -cf $env:TEMP\ra.tar --format ustar -C $env:TEMP\retroarch-hmod\extracted\ra .
cmd /c "ssh <opts> root@169.254.13.37 ""cat > /tmp/ra.tar"" < ""$env:TEMP\ra.tar"""
ssh ... root@169.254.13.37 "mkdir -p /tmp/ra && tar -xf /tmp/ra.tar -C /tmp/ra && chmod +x /tmp/ra/bin/*"
```

Expected: `/tmp/ra/bin/retroarch` present + executable.

---

### Task 3: Push the kernel modules to tmpfs

- [ ] **Step 1: tar the modules on host + push via ssh stdin**

```powershell
# place verified mali.ko + clovercon.ko in a dir, e.g. $env:TEMP\mods\
tar -cf $env:TEMP\mods.tar --format ustar -C $env:TEMP\mods .
$opts = '-o StrictHostKeyChecking=no -o UserKnownHostsFile=NUL -o PreferredAuthentications=none -o PubkeyAuthentication=no -o ConnectTimeout=15'
cmd /c "ssh $opts root@169.254.13.37 ""mkdir -p /tmp/mods && cat > /tmp/mods/mods.tar"" < ""$env:TEMP\mods.tar"""
ssh ... root@169.254.13.37 "tar -xf /tmp/mods/mods.tar -C /tmp/mods && ls -la /tmp/mods"
```

Expected: `mali.ko` + `clovercon.ko` present under `/tmp/mods`.

---

### Task 4: insmod mali.ko + verify GPU node

- [ ] **Step 1: insmod by full path**

```powershell
ssh ... root@169.254.13.37 "insmod /tmp/mods/mali.ko 2>&1 && echo MALI_OK || echo MALI_FAIL; echo '--- lsmod ---'; lsmod | grep -i mali; echo '--- /dev/mali ---'; ls -la /dev/mali* /dev/ump* 2>&1"
```

Expected (GO direction): `MALI_OK`, `lsmod` shows `mali`, `/dev/mali` exists. **No `invalid module format`.**

Failure signatures:
- `invalid module format` + `dmesg` vermagic mismatch → wrong module build → back to Task 1 (the `hakchi-latest` tie was not exact) → INCONCLUSIVE/build-from-source.
- `unknown symbol` → needs a companion module (e.g. `ump.ko`); source it from the same set and `insmod` it first.

Record the result in the findings doc under `## insmod result`.

---

### Task 5: insmod clovercon.ko + verify input node

- [ ] **Step 1: insmod with dp-nes params**

The board is `dp-nes` (confirmed `/newroot/etc/clover/boardtype`); stock `S79clovercon` uses `module_params=1,195,2,194` for it.

```powershell
ssh ... root@169.254.13.37 "insmod /tmp/mods/clovercon.ko module_params=1,195,2,194 2>&1 && echo CC_OK || echo CC_FAIL; echo '--- lsmod ---'; lsmod | grep -i clovercon; echo '--- input ---'; ls -la /dev/input/ 2>&1"
```

Expected: `CC_OK`, `lsmod` shows `clovercon`, `/dev/input/event*` appears. Record under `## insmod result`. (If clovercon fails but mali succeeded, continue — render can still be proven; input becomes the PARTIAL path.)

---

### Task 6: Re-run staged RA on the gl driver + observe render

- [ ] **Step 1: kill any prior RA, relaunch on gl (detached)**

Write a local `relaunch.sh` (LF) and pipe via stdin:

```sh
for p in $(pidof retroarch); do kill -9 $p; done
sleep 1
rm -f /tmp/ra.log
setsid env HOME=/tmp/ra/etc/libretro LD_LIBRARY_PATH=/usr/lib /tmp/ra/bin/retroarch -c /tmp/ra/etc/libretro/retroarch.cfg -v < /dev/null > /tmp/ra.log 2>&1 &
sleep 6
echo "=== alive? ==="; pidof retroarch && echo RUNNING || echo EXITED
echo "=== video log ==="; sed -n '/Video @ fullscreen/,$p' /tmp/ra.log | head -20
```

```powershell
cmd /c "ssh <opts> root@169.254.13.37 sh -s < $env:TEMP\relaunch.sh"
```

Expected (GO): `RUNNING`; the video log shows the Mali EGL context created (no `EGL_BAD_ALLOC`, no `Cannot open video driver`), RGUI initialized.

Failure: still `EGL_BAD_ALLOC` despite mali loaded → NO-GO (GPU can't init under memboot even with the module); capture the full EGL error + `dmesg` and escalate.

- [ ] **Step 2: Photograph the RGUI menu on HDMI**

Take a photo of the RGUI menu (or failure state). Save the reference in the findings doc under `## RA gl render`.

---

### Task 7: Controller navigation

- [ ] **Step 1: Confirm RA sees the joypad**

```powershell
ssh ... root@169.254.13.37 "ls -la /dev/input/; echo '--- RA joypad log ---'; grep -iE 'joypad|udev|input|pad' /tmp/ra.log | tail -15"
```

Expected: `/dev/input/event*` present; RA log shows a joypad detected (udev driver).

- [ ] **Step 2: Navigate with the controller**

On the console, press D-pad up/down + A/B.

Expected (GO): the RGUI highlight moves and items activate. If not (PARTIAL): record `/dev/input` state + whether RA logged a joypad; the input gap becomes the next task. Write the result under `## Controller`.

---

### Task 8: Verdict, finalize findings, restore device

- [ ] **Step 1: Apply verdict criteria**

From the spec: **GO** (mali loads + RGUI renders + controller navigates); **PARTIAL** (renders, no input); **INCONCLUSIVE → build-from-source** (no exact-match module downloadable); **NO-GO** (matched mali loads but EGL still fails).

- [ ] **Step 2: Complete the findings doc**

Ensure `docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md` contains: module source + provenance, vermagic proof, `insmod`/`lsmod`/`/dev` state, RA `gl` video log, HDMI photo ref, controller result, and a one-line **GO / PARTIAL / INCONCLUSIVE / NO-GO** call with next step.

- [ ] **Step 3: Commit**

```powershell
git add docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md
git commit -m "spike(gpu): GPU memboot findings + verdict"
```

- [ ] **Step 4: Restore device (cleanup)**

RAM-only — power-cycle returns the unit to bone-stock (nothing written to NAND; modules were `insmod`'d into RAM only). Power-cycle, confirm clean stock boot, note it in the findings doc.

---

## Self-review

- **Spec coverage:** Goal/boundary → Tasks 1–8. Mechanism step 1 (source) → Task 1; step 2 (verify) → Task 1 Step 3; step 3 (push) → Task 3; step 4 (insmod mali+clovercon) → Tasks 4–5; step 5 (re-run RA gl) → Task 6; step 6 (observe + navigate) → Tasks 6–7. Verdict criteria (GO/PARTIAL/INCONCLUSIVE/NO-GO) → Task 8 Step 1. Deliverable (source, vermagic, insmod/lsmod/dev, RA log, photo, controller, one-line call) → Tasks 1,4,5,6,7,8 all write the one findings doc. Primary risk (exact-build match) → Task 1 Steps 1–3 + Task 4 failure branch. Companion-module risk → Task 4 failure branch. Keep-hakchi-kernel → Task 2 Step 1 (uname check). Brick-safety → Task 8 Step 4 (RAM-only, power-cycle). No spec requirement unmapped.
- **Placeholders:** `<opts>` and `...` in `ssh ...` are the documented `$SSH` invocation from the gotchas block, abbreviated at point of use; `$env:TEMP\mods\` is where Task 1's verified modules are placed. No vague TODO-class gaps.
- **Consistency:** kernel `3.4.113.29-madmonkey`, clovercon params `1,195,2,194` (dp-nes), paths `/tmp/ra`, `/tmp/mods`, findings path `docs/superpowers/findings/2026-06-02-gpu-memboot-spike.md`, and the findings-doc section headings used identically throughout.
