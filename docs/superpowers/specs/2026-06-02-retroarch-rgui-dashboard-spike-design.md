# RetroArch RGUI Dashboard Tracer Bullet — Design

**Date:** 2026-06-02
**Status:** design
**Parent initiative:** Custom boot screen + customizable dashboard (deployed via RetroForge).
**This spec covers the dashboard tracer bullet only.** It is the smallest
end-to-end proof that RetroArch can serve as the device's dashboard shell,
before any theming, playlists, or boot-time autostart work.

## Why this, and why now

The dashboard initiative first considered porting a desktop frontend
(EmulationStation-DE / Pegasus). A feasibility spike killed the cheap path:
**no GLES-linked armhf ES-DE prebuilt exists** — the only official armhf build is
desktop-GL + Broadcom-bound and cannot run on the Mini's Mali-400. Getting ES-DE
on-device would require a from-scratch cross-compile. See
`docs/superpowers/findings/2026-06-02-esde-spike.md` and memory note
`esde-prebuilt-dead-end`.

RetroArch is a far cheaper, lower-risk path and is **already on the sanctioned
architecture**: ADR-0002 commits RetroForge to reusing hakchi's on-device system
("ramdisk scripts, init, **RetroArch glue**, controller driver — forked from
Hakchi's `ce-data`") and names the device-side value-add as "swap in newer
RetroArch/cores ... save-states/cheats/overlays." The membooted hakchi image
**already contains RetroArch + the `launchRA` wrapper**; it just boots
`m2engage` by default. And the boot-screen feature already proved we can repack
that ramdisk. So the dashboard is RetroArch's own menu, configured and deployed
by RetroForge.

## Goal & boundary

Prove the membooted Mini can run RetroArch's menu as its dashboard shell — the
tracer bullet for the whole "customizable dashboard" initiative, mirroring how
the boot-screen tracer bullet proved the modify → deploy → observe loop.

**Done =** on a membooted device, with stock `m2engage` stopped, RetroArch
launched into its **RGUI** menu, rendering to HDMI, and **navigable with the
physical controller**. Non-persistent (RAM-memboot only, zero NAND writes); a
normal power-cycle returns to stock.

### In scope
- Memboot the stock hakchi image (existing `memboot` command).
- Over the network shell: stop `m2engage`, push an RGUI config, launch
  RetroArch in menu mode via hakchi's own launch path.
- Observe the RGUI menu render on HDMI.
- Confirm the physical controller navigates the menu.
- Capture a findings doc with the verdict.

### Out of scope
- ozone / XMB / any GLES menu driver (RGUI is framebuffer-only by design here).
- Themes, wallpaper, playlists, launching a game from the menu.
- Boot-time autostart (ramdisk repack so the device boots straight into RA).
- Persistence / NAND flash.
- Any RetroForge UI or committed app code.

## Decisions locked in brainstorm

| Question | Decision | Why |
|---|---|---|
| First cut | **Tracer bullet: boot into RA menu** | Smallest end-to-end proof; mirrors the boot-screen win |
| Menu driver | **RGUI** | Framebuffer-only, zero GPU/GLES dependency — guaranteed render; isolates the boot-redirect mechanic from Mali-400 GPU risk |
| Mechanism | **SSH-launch first** (not ramdisk repack) | Reuses memboot + network shell; fastest render proof; autostart productized in a later cut |
| Success bar | **Renders + navigates** (controller) | A dashboard you can't navigate isn't one; input is part of the proof |
| Integration | **Manual + scripted** | Throwaway runbook over existing features; nothing to maintain for a probe |

## Mechanism / data flow

Reuses memboot + SSH-over-RNDIS. No new app code; all steps are
manual/scripted, recorded inline in the findings doc.

1. **Memboot** the stock hakchi image (existing `memboot` command). Device comes
   up on the RNDIS link at `169.254.13.37`, dropbear listening, SSH auth =
   `none` method. See memory `slice3-clovershell-pending-hw`.
2. **Locate RetroArch + its launch env.** Over SSH, find the RA binary and read
   how hakchi launches it — the `launchRA` wrapper sets the env (LD paths,
   video/audio driver, config dir) RA needs on this hardware. **Reuse hakchi's
   launch path; do not hand-roll a bare invocation.** Record the binary path,
   config dir, and the video driver hakchi's RA uses.
3. **Stop the stock UI.** `pkill m2engage` / stop the `moon-game` init job →
   frees the framebuffer and RAM; HDMI goes idle.
4. **Stage an RGUI config.** Push a minimal `retroarch.cfg` (or use
   `--appendconfig`) over SSH forcing `menu_driver = "rgui"` and start-in-menu,
   **layered on hakchi's existing config** so device-specific video/input
   settings survive.
5. **Launch RetroArch in menu mode** via the hakchi-mimicked launch path
   (`retroarch --menu` + the appended RGUI config), rendering to the
   framebuffer / HDMI.
6. **Observe render** — RGUI menu visible on HDMI.
7. **Navigate with the controller** — confirm the membooted image's controller
   driver feeds RA (RA joypad autoconfig). Move through menu items. If input
   does not reach RA, **characterize it** (which input device, what RA saw) —
   do not report a silent fail.

## Verdict criteria

- **GO** — RGUI renders on HDMI **and** the controller navigates it. RetroArch
  is viable as the dashboard shell. Next cut: pretty GLES driver (ozone) +
  autostart-on-boot (ramdisk repack).
- **PARTIAL** — RGUI renders but controller input does not reach RA. Still a GO
  on render; the input gap becomes the first task of the next cut (joypad
  autoconfig). Capture the exact input device + what RA saw.
- **NO-GO** — RA won't launch on the membooted image or won't render at all.
  Unlikely (hakchi ships RA), but if so capture the launch error verbatim →
  routes to investigating hakchi's RA launch env.

## Deliverable

A findings doc: `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md`,
containing:
- RA binary path + launch env (how hakchi's `launchRA` invokes it).
- The pushed RGUI config (or `--appendconfig` contents).
- A photo of the RGUI menu on HDMI.
- The controller-navigation result (works / device seen but no input / etc.).
- A one-line **GO / PARTIAL / NO-GO** call with the next step.

No app code is committed; throwaway snippets live in the runbook inside the
findings doc. This spec lives in `docs/superpowers/specs/`.

## Risks & notes

- **Controller → RA input** (primary unknown): `m2engage` uses hakchi's
  controller path; RA needs the joypad device + autoconfig. Step 2 records the
  device; step 7 proves or characterizes it. A PARTIAL verdict is an acceptable,
  informative outcome — not a failure.
- **RA launch env**: RA on this image likely depends on `launchRA`'s env
  (Mali/video driver, paths). Mitigation = reuse that wrapper, not a bare
  invocation.
- **RGUI video path**: RGUI renders through RA's configured video driver and
  needs no GL; if the device's GL driver balks, force an fbdev/software-safe
  video path in the appended config.
- **Hardware-only**: like the prior features, the real verdict comes only from
  the device — no CI coverage applies to a manual spike.
