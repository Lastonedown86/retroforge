# RetroArch RGUI Dashboard Tracer Bullet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **This is a manual/scripted hardware spike, not a code feature.** There is no pytest/cargo TDD loop. "Tests" here are on-device observations with exact commands and expected output. The only committed artifact is the findings doc. **Every task requires the physical device on the bench** — there is no host-only autonomous task. An agentic worker without the device executes the host-side prep it can (launch the app), then hands the device-bound steps to the user with the exact commands below.

**Goal:** Prove the membooted Nintendo Classic Mini can run RetroArch's RGUI menu as its dashboard shell — rendered on HDMI and navigable with the physical controller — using only the existing memboot + SSH path.

**Architecture:** Memboot the stock hakchi image (existing `memboot` command), then over SSH-over-RNDIS: locate RetroArch and how hakchi's `launchRA` invokes it, stop `m2engage`, push an RGUI config layered on hakchi's, and launch RetroArch in menu mode. Observe render + controller navigation. No app code committed; throwaway snippets live in the findings doc.

**Tech Stack:** Existing RetroForge memboot (FEL) + network shell (russh SSH `none` auth, device at `169.254.13.37`). On-device: hakchi's RetroArch + `launchRA` wrapper. Host: `ssh`/`scp` over the RNDIS link.

---

## Spec reference

Design: `docs/superpowers/specs/2026-06-02-retroarch-rgui-dashboard-spike-design.md`. Read it first — it holds the locked decisions and verdict criteria this plan executes. ADR-0002 (`docs/adr/0002-reuse-on-device-system.md`) is the architectural basis: the hakchi image already ships RetroArch + `launchRA`.

## Preconditions (verify before Task 1)

- [ ] Device on hand, USB-FEL capable. FEL = USB `1F3A:EFE8`; rusb needs WinUSB bound via Zadig (reverts to libusb0 each session — re-bind, verify `DEVPKEY_Device_Service` = `WinUSB`). See memory `fel-hardware-constraints`.
- [ ] RNDIS host NIC works: after memboot, gadget = `04E8:6863`; host needs generic RNDIS NIC driver + (one-time) reboot to clear ghost miniports. See memory `slice3-clovershell-pending-hw`.
- [ ] A controller plugged into the console (the navigation test in Task 6 needs it).
- [ ] RetroForge builds + runs: `npm run tauri dev` (vite :1420). Kill stale instances first (`Stop-Process -Name retroforge -Force`, free :1420).

## File structure

- **Create:** `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md` — the deliverable. Holds RA binary path + launch env, the pushed RGUI config, the HDMI photo reference, the controller-navigation result, and the one-line verdict. Filled progressively across tasks.
- **No app source changes.** All shell snippets are throwaway and recorded inline in the findings doc as they are run.

---

### Task 1: Memboot the device and confirm SSH

- [ ] **Step 1: Memboot**

Launch RetroForge (`npm run tauri dev`), trigger `memboot` from the UI. Watch the `memboot-progress` events to completion.

Expected: memboot completes; the device reboots into the RAM image and brings up RNDIS. The stock `m2engage` menu appears on HDMI.

- [ ] **Step 2: Confirm the device is reachable**

```powershell
Test-NetConnection 169.254.13.37 -Port 22
```

Expected: `TcpTestSucceeded : True`. (The device ignores ICMP — do not rely on ping.)

- [ ] **Step 3: Confirm SSH exec works (none auth)**

```powershell
ssh -o StrictHostKeyChecking=no -o PreferredAuthentications=none -o PubkeyAuthentication=no root@169.254.13.37 "uname -a"
```

Expected: prints `Linux clover ...`. If `none` is rejected, fall back to empty-password (`ssh ...` then blank password). Record reachability + `uname -a` in the findings doc.

- [ ] **Step 4: Start the findings doc**

Create `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md` with headings `## Device + SSH`, `## RetroArch binary + launch env`, `## RGUI config`, `## Render observation`, `## Controller navigation`, `## Verdict`. Record the Step 2–3 results under `## Device + SSH`.

---

### Task 2: Locate RetroArch and its launch env (reuse hakchi's `launchRA`)

The whole risk-mitigation hinges on reusing hakchi's launch wrapper rather than guessing RA's env. Find it before touching anything.

- [ ] **Step 1: Find the RetroArch binary**

```powershell
ssh root@169.254.13.37 "which retroarch; find / -xdev -name 'retroarch' -type f 2>/dev/null"
```

Expected: one or more paths (hakchi commonly puts it under `/bin/retroarch` or `/usr/bin/retroarch`). Record the path.

- [ ] **Step 2: Find and read the `launchRA` wrapper**

```powershell
ssh root@169.254.13.37 "find / -xdev -name 'launchRA' 2>/dev/null; echo '--- contents ---'; cat $(find / -xdev -name 'launchRA' 2>/dev/null | head -1)"
```

Expected: the `launchRA` script text. Read it for: the exact `retroarch` invocation, any exported env (`LD_LIBRARY_PATH`, `HOME`, video/audio driver vars), the `--config` / config dir it points at, and any `--appendconfig`. This is the env RA needs on this hardware. Record the full invocation + env in the findings doc under `## RetroArch binary + launch env`.

- [ ] **Step 3: Find the active RetroArch config + current menu/video drivers**

```powershell
ssh root@169.254.13.37 "find / -xdev -name 'retroarch.cfg' 2>/dev/null; echo '--- drivers ---'; for f in $(find / -xdev -name 'retroarch.cfg' 2>/dev/null); do echo \"== $f ==\"; grep -E 'menu_driver|video_driver|menu_show' $f; done"
```

Expected: the config path(s) and the current `video_driver` (likely `sunxi`/`gl`/`sdl2` on this hardware) and `menu_driver`. Record them — the RGUI config in Task 4 layers on top of this so the device's working `video_driver` is preserved.

---

### Task 3: Stop the stock UI and free the framebuffer

- [ ] **Step 1: Stop m2engage**

```powershell
ssh root@169.254.13.37 "pkill -f m2engage; pkill -f moon-game; sleep 1; pgrep -f m2engage || echo stopped"
```

Expected: prints `stopped` (no m2engage process). HDMI goes blank/idle — the framebuffer is now free for RetroArch.

- [ ] **Step 2: Confirm nothing else holds the framebuffer**

```powershell
ssh root@169.254.13.37 "fuser /dev/fb0 2>/dev/null; echo done"
```

Expected: `done` with no PID listed (or only PIDs you will replace). If a process still holds `/dev/fb0`, identify and stop it. Record in the findings doc.

---

### Task 4: Stage an RGUI config layered on hakchi's

Force `menu_driver = "rgui"` and start-in-menu without disturbing the device's working video/input settings. Use `--appendconfig` so the base config from Task 2 Step 3 stays intact.

- [ ] **Step 1: Write the append-config on the host**

Create a small file `rgui-append.cfg` with exactly:

```
menu_driver = "rgui"
menu_show_load_core = "true"
menu_show_load_content = "true"
content_show_settings = "true"
```

(Only the menu driver is forced; video_driver is intentionally omitted so hakchi's value from Task 2 Step 3 is used. If Task 5 shows the GL video driver fails to bring up RGUI, Task 5 Step 3 adds an fbdev fallback.)

- [ ] **Step 2: Push it to device tmpfs**

```powershell
scp rgui-append.cfg root@169.254.13.37:/tmp/rgui-append.cfg
ssh root@169.254.13.37 "cat /tmp/rgui-append.cfg"
```

Expected: the file contents echo back. Record the exact pushed config in the findings doc under `## RGUI config`.

---

### Task 5: Launch RetroArch in RGUI menu mode and observe render

- [ ] **Step 1: Launch using hakchi's env + the RGUI append-config**

Build the command from Task 2's `launchRA` findings: reuse its env exports and binary path, add `--menu` and `--appendconfig /tmp/rgui-append.cfg`. Shape (substitute `<RA_BIN>` and any `<ENV>` from Task 2):

```powershell
ssh root@169.254.13.37 "<ENV> <RA_BIN> --menu --appendconfig /tmp/rgui-append.cfg 2>&1 | head -120"
```

Keep the SSH session open so RA keeps running (or background with `&` and tail the log).

Expected (GO direction): RetroArch initializes, selects the `rgui` menu driver, opens the framebuffer, and **the HDMI output shows the RGUI menu** (RetroArch's grey/blue text menu). Log shows no fatal driver errors.

Failure signatures to record verbatim if seen:
- `Failed to open ...fb0` / `video driver ... failed to init` → framebuffer/video NO-GO → try Step 3.
- `Killed` / OOM (`ssh ... dmesg | tail`) → memory NO-GO (unlikely for RGUI).
- RA exits immediately with a config/path error → launch-env problem; re-check Task 2 env.

- [ ] **Step 2: Photograph the RGUI menu on HDMI**

Take a photo of the screen showing the RGUI menu (or the failure state). Save the reference/filename in the findings doc under `## Render observation`.

- [ ] **Step 3 (only if Step 1 video init failed): retry with an fbdev-safe video driver**

Append a forced framebuffer video driver and relaunch:

```powershell
ssh root@169.254.13.37 "printf 'video_driver = \"sdl2\"\n' >> /tmp/rgui-append.cfg; <ENV> <RA_BIN> --menu --appendconfig /tmp/rgui-append.cfg 2>&1 | head -120"
```

Expected: if the GL path was the blocker, an SDL2/fbdev video driver brings RGUI up. Record which video driver finally rendered RGUI. (If neither GL nor SDL2/fbdev renders, that is the NO-GO render finding — capture both error logs.)

---

### Task 6: Controller navigation test

- [ ] **Step 1: Identify the input device RA sees**

While RA runs (from a second SSH session), check the joypad:

```powershell
ssh root@169.254.13.37 "ls -la /dev/input/; echo '--- RA autoconfig ---'; find / -xdev -path '*autoconfig*' -name '*.cfg' 2>/dev/null | head"
```

Expected: an event/js device under `/dev/input/` (the console controller) and any RA joypad autoconfig present. Record the device + whether an autoconfig matches.

- [ ] **Step 2: Navigate with the physical controller**

On the console, use the plugged-in controller: press D-pad up/down and A/B.

Expected (GO): the RGUI selection highlight moves and menu items activate — the menu is navigable.

If input does NOT move the menu (PARTIAL): capture which `/dev/input/` device exists, whether RA's log mentions detecting a joypad, and whether an autoconfig was loaded. This characterizes the input gap for the next cut (joypad autoconfig); it is NOT a silent fail.

- [ ] **Step 3: Record the navigation result**

Write the outcome (works / device seen but no input / no device) into the findings doc under `## Controller navigation`.

---

### Task 7: Write the verdict, finalize findings, restore device

- [ ] **Step 1: Apply the verdict criteria**

From the spec:
- **GO** — RGUI rendered on HDMI **and** the controller navigated it.
- **PARTIAL** — RGUI rendered but controller input did not reach RA. Still a GO on render; the input gap is the next cut's first task. Capture the exact input device + what RA saw.
- **NO-GO** — RA won't launch or won't render at all. Capture the launch/render error verbatim.

- [ ] **Step 2: Complete the findings doc**

Ensure `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md` contains: RA binary path + the hakchi launch env used, the pushed RGUI append-config, which video driver rendered RGUI, the HDMI photo reference, the controller-navigation result, and a **one-line GO / PARTIAL / NO-GO call with the recommended next step** (next cut = ozone GLES driver + boot-autostart via ramdisk repack).

- [ ] **Step 3: Commit**

```powershell
git add docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md
git commit -m "spike(retroarch): RGUI dashboard findings + GO/PARTIAL/NO-GO verdict"
```

(End the commit message with a trailing `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>` line. The LF→CRLF warning is harmless.)

- [ ] **Step 4: Restore the device (cleanup)**

The spike is non-persistent — a normal power-cycle returns the device to stock (memboot wrote nothing to NAND). Power-cycle and confirm a clean stock boot (m2engage). Note confirmation in the findings doc.

---

## Self-review

- **Spec coverage:** Goal/boundary → Tasks 1–7. Mechanism steps 1–7 → Task 1 (memboot+ssh), Task 2 (locate RA + launchRA env), Task 3 (stop m2engage + free fb), Task 4 (stage RGUI config), Task 5 (launch + observe render), Task 6 (controller navigate). Verdict criteria (GO/PARTIAL/NO-GO) → Task 7 Step 1. Deliverable (RA path+env, pushed config, photo, nav result, one-line call) → Tasks 2,4,5,6,7 all write into the one findings doc. Primary risk "reuse launchRA env" → Task 2 + Task 5 Step 1. Controller-input risk → Task 6 (+ PARTIAL verdict). RGUI video-path risk → Task 5 Step 3. No spec requirement left unmapped.
- **Placeholders:** `<RA_BIN>`, `<ENV>` are deliberate runtime substitutions discovered in Task 2 and flagged at point of use — not vague TODOs. No "add error handling"-class gaps.
- **Consistency:** device IP `169.254.13.37`, append-config `/tmp/rgui-append.cfg`, binary referenced as `retroarch`/`<RA_BIN>`, findings path `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md`, and the six findings-doc headings used identically throughout.
