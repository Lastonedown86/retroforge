# ForgeDash ← RetroArch Bubble-Up Roadmap

> **Direction:** limit visible RetroArch UI to ~zero. RA is the invisible
> emulation engine; **ForgeDash owns the entire player-facing UI.** Every feature
> RGUI exposes is "bubbled up" into ForgeDash by driving RA through its
> non-interactive surfaces (config files, on-disk artifacts, hotkeys, the command
> interface). This document inventories that surface and proposes a slice order.
>
> Living roadmap — each row becomes its own brainstorm → spec → plan → slice.

## How ForgeDash drives RetroArch (the "API")

RA has no public API for a frontend; instead it reads/writes files and accepts a
few runtime signals. ForgeDash uses five mechanisms:

| # | Mechanism | What it covers | When |
|---|---|---|---|
| A | **Config keys** in `retroarch.cfg` + the **override hierarchy** (global → core → content-dir → per-game `.cfg`) | video, audio, input driver, menu, most toggles | pre-launch (ForgeDash writes the cfg) |
| B | **On-disk artifacts** RA reads/writes: `.state`(+`.png` thumb), `.srm`, `.opt`, `.rmp`, `.cht`, `.glslp`, `.m3u`, screenshots, `system/` BIOS | save states, SRAM, core options, remaps, cheats, shaders, disks, BIOS | ForgeDash lists/edits/manages on disk |
| C | **Hotkeys** (pad combos, Select = enable_hotkey) | runtime actions: exit, save/load, slot, reset, pause, ff, disk | mid-game (no UI) — limited by the 5-button NES pad |
| D | **Command interface** (`network_cmd_enable` UDP / stdin) — SAVE_STATE, LOAD_STATE, RESET, PAUSE_TOGGLE, DISK_NEXT/EJECT, SCREENSHOT, QUIT, … | headless runtime control by a helper | mid-game, programmatic |
| E | **ForgeDash UI screens** | pre-launch options, browsers, managers | before/after a game (ForgeDash is resident only then) |

**The override hierarchy (A) is the keystone.** Once ForgeDash writes per-game
config overrides, most settings categories (options, video, audio, controls)
become "write a file, launch RA" — shared infrastructure built once and reused.

## Hard constraints (shape every decision)

1. **Single Mali EGL surface** — only one GLES app at a time (the reason for the
   shell-loop). No ForgeDash visuals can be drawn *over* a running game. Mid-game
   control = hotkeys (C) or commands (D) only; anything visual happens *around*
   the game in ForgeDash (E).
2. **NES Classic pad = D-pad, Select, Start, A, B.** Mid-game hotkeys are scarce
   (Select-modified combos). Rich control must live in ForgeDash pre-launch
   screens, not in-game combos.
3. **RA 1.7.0 (2018) + Mali GLES2.** Older feature set; shader support is GLSL
   ES-limited (heavy CRT presets may not run). Verify features on device.
4. **Brick-safe:** everything stays RAM-memboot for now; persistent paths come
   with the NAND-install slice (ADR-0003 backup-gated).

## Feature inventory → bubble-up plan

Legend for "Mechanism": A=config, B=file, C=hotkey, D=command, E=ForgeDash UI.

| Feature (RGUI item) | Mechanism | ForgeDash surface | Notes |
|---|---|---|---|
| **Launch content** | E | Library coverflow | ✅ DONE (shipped slice) |
| **Exit / return to menu** | C | Select+Start hotkey | Slice 1 (headless) |
| **Save / Load state** | C + B | hotkeys now; state browser later | Slice 1 (hotkeys+toasts); Slice 2 (browser) |
| **Save-state browser** | B + E | list/load/delete `.state` + `.png` thumbnails, slots, save-on-exit | Slice 2 |
| **Core options** (palette, overscan, region, …) | B + A | per-game options screen → writes `<game>.opt` | Slice 3 (needs override infra) |
| **Core selection / emulator routing** | A + E | per-game core field (fceumm/nestopia/kachikachi) | partial (library `core`); finish in Slice 3 |
| **Controls / input remap** (per-game button map, turbo, port) | B | remap screen → `.rmp` files | Slice 4 |
| **Video** (aspect ratio, integer scale, rotation, overscan crop) | A | video settings screen | Slice 5 |
| **Shaders / CRT filters** | B + A | shader picker → `.glslp` preset | Slice 5 — Mali GLES2-limited, curate presets |
| **Cheats** | B + A | cheat manager → `.cht`, enable/disable | Slice 6 |
| **Disk control** (FDS disk sides, multi-disc) | C/D + B | `.m3u` + disk-swap hotkey/UI | Slice 7 — FDS needs `disksys.rom` BIOS |
| **BIOS / system files** | B | ensure `system/` has required BIOS (e.g. FDS) | with Slice 7 |
| **SRAM battery saves** (`.srm`) | B | auto by RA; ForgeDash backup/restore | Slice 2-adjacent (with save mgmt) |
| **Reset / Restart content** | C/D | hotkey or ForgeDash relaunch | cheap, fold into Slice 1/3 |
| **Rewind** | A + C | per-game enable toggle (cfg) + hotkey | Slice 5-adjacent; cost on R16 — verify |
| **Fast-forward / slow-mo / pause** | C | hotkeys (pad combos scarce) | optional; pick few |
| **Screenshots** | C/D + B | capture hotkey; optional ForgeDash gallery | low priority |
| **Audio** (volume, mute, latency) | A | minor settings | low priority |
| **Overlays / bezels** | A + B | decorative frame; tie to ForgeDash theme | optional / cosmetic |
| **Achievements (RetroAchievements)** | A (+network) | login + indicators | OUT for v1 (needs network/account) |
| **Netplay** | — | — | OUT (no use on Classic) |

## Proposed slice order

1. **Headless RA** (this slice) — `menu_driver=null`, ForgeDash owns
   `ra-override.cfg`, exit + save/load hotkeys, save/load toasts kept. Foundation:
   ForgeDash now controls RA's runtime config.
2. **Save-state browser** — ForgeDash screen over `.state`(+thumb) + `.srm`:
   list/load/delete, slots, save-on-exit/auto-save. Highest player value.
3. **Per-game config override infra + Core options & routing** — build the
   override-writing infrastructure (keystone), expose core options (`.opt`) and
   per-game core pick. Unblocks slices 4–6.
4. **Controls / remap** — `.rmp` per-game button maps, turbo, port.
5. **Video + shaders** — aspect/integer/rotation + curated Mali-safe `.glslp`.
6. **Cheats** — `.cht` manager.
7. **Disk control + BIOS** — FDS/multi-disc via `.m3u`, ensure `system/` BIOS.
- **Later / optional:** rewind toggle, screenshots gallery, SRAM backup/sync,
  overlays/bezels. **Out for v1:** achievements, netplay.

## Shared infrastructure to build early (Slice 3)

A single ForgeDash module that writes RA's **per-game config override** (correct
path in the override hierarchy) is reused by options, controls, video, shaders,
cheats. Building it once in Slice 3 keeps later slices small (each just adds a UI
screen + the specific keys/files). Pair it with a typed model of the RA cfg keys
ForgeDash sets, so the cfg generation stays testable (pure string/serialization,
unit-tested off-device — the same pattern as `ra_override_cfg()` in Slice 1).
