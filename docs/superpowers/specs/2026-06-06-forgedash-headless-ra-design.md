# ForgeDash Slice — Headless RetroArch (Design)

> Second ForgeDash slice. Removes RetroArch's visible UI (RGUI) so the player
> sees only ForgeDash and the running game. RetroArch becomes an invisible
> engine; ForgeDash owns RetroArch's runtime config. In-game actions move to
> Select-modified pad hotkeys. Foundation for the whole bubble-up roadmap
> (`docs/superpowers/forgedash-ra-bubbleup-roadmap.md`).

## Goal & success criteria

When a game is launched from ForgeDash, **no RetroArch menu ever appears**. The
player can, with the NES pad alone:
- exit the game back to ForgeDash (Select+Start),
- save / load state (Select+A / Select+B) with a brief on-screen toast,
- change save slot (Select+Right / Select+Left) with a toast.

Proven on hardware (`dp-nes`), zero NAND writes.

## Background (current state)

Shipped slice launches RA via `forge-loop.sh`:
`retroarch -c retroarch.cfg --appendconfig /tmp/ra-input.cfg -L <core> <rom>`.
`/tmp/ra-input.cfg` is hand-staged and sets `menu_driver = "rgui"` plus the
proven udev input driver. Quit currently uses RGUI (Select+Start → "Quit
RetroArch"). This slice replaces that hand-staged file with a ForgeDash-owned
override and drops the RGUI dependency.

## Decisions

| Decision | Choice |
|---|---|
| Menu | `menu_driver = "null"` — RGUI never instantiated |
| Config ownership | ForgeDash generates `ra-override.cfg` (seed of per-game override infra) |
| In-game control | Select=enable_hotkey; Start=exit, A=save, B=load, D-pad Right/Left=slot +/- |
| Feedback | Keep RA notification toasts for save/load/slot; `fps_show=false`, menu widgets/animations off |
| Process model | Unchanged shell-loop; all hotkeys handled RA-side (ForgeDash not resident in-game) |

## Architecture & components

Small change set across the existing workspace:

- **`forgedash-core::launch` — new pure fn `ra_override_cfg(map: &PadMap) -> String`.**
  Builds the full RetroArch override config text. Pure → unit-tested off-device.
  Emits, in one block:
  - input driver carry-over: `input_joypad_driver = "udev"`, `input_driver = "udev"`
  - `menu_driver = "null"`
  - OSD: `fps_show = "false"`, `menu_enable_widgets = "false"`,
    `menu_show_load_content_animation = "false"` (notifications left on for toasts)
  - hotkeys, using the Clovercon joypad button indices from `PadMap`:
    `input_enable_hotkey_btn`, `input_exit_emulator_btn`, `input_save_state_btn`,
    `input_load_state_btn`, and slot +/- bound to the d-pad (button or hat per
    the device's autoconfig).
- **`PadMap`** — a small struct of the Clovercon button indices / d-pad encoding
  (select, start, a, b, dpad-right, dpad-left). Sourced once from RA's
  `clovercon1.cfg` autoconfig on device and hardcoded as a documented default
  (`PadMap::clovercon()`); kept as a struct so it stays testable and overridable.
- **`app` (main.rs)** — on `Nav::Confirm`, before exit, write
  `ra_override_cfg(&PadMap::clovercon())` to `<FORGE_ROOT>/ra-override.cfg`
  (alongside the handoff). One added write call; reuses the existing handoff path.
- **`scripts/forge-loop.sh`** — change `--appendconfig /tmp/ra-input.cfg` to
  `--appendconfig "$FORGE_ROOT/ra-override.cfg"`. No other change.

Data flow: Confirm → write `handoff` + `ra-override.cfg` → exit → `forge-loop.sh`
runs RA `--appendconfig ra-override.cfg -L <core> <rom>` → headless game →
Select+Start → RA exits → ForgeDash relaunches.

## Determining the PadMap (one-time, on device)

RA's udev joypad driver auto-selected `clovercon1.cfg` (shipped slice). Read that
autoconfig on device for `input_select_btn`, `input_start_btn`, `input_a_btn`,
`input_b_btn`, and the d-pad encoding (`input_*_btn` vs `input_*_axis`/hat).
Those values populate `PadMap::clovercon()`. RetroArch hotkey `_btn` binds use the
same joypad button numbering, so the autoconfig values map directly.

## Error handling

- **`menu_driver=null` unsupported by RA 1.7.0** → RA may error at startup
  (caught by `forge-loop.sh`: non-zero exit → `ra_exit` toast on return).
  Fallback: keep `menu_driver=rgui` but never trigger it (exit via hotkey, which
  also works with RGUI present). Decide on HW.
- **Wrong exit-btn index → player stuck in game** (can't leave). Mitigation:
  **HW-verify the exit combo first**, before relying on it; RAM-memboot means a
  power-cycle always recovers. Acceptance step 1 is the exit combo.
- **`ra-override.cfg` missing** (ForgeDash failed to write) → `forge-loop.sh`
  should guard: if the file is absent, fall back to launching without
  `--appendconfig` (RA still runs; input may default) and log it.

## Testing

- **`cargo test -p forgedash-core`**: `ra_override_cfg()` emits the expected keys
  and values for a known `PadMap` (assert each hotkey line + `menu_driver=null` +
  driver carry-over). Pure, runs in CI.
- **Hardware acceptance** (`docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`, append a
  headless section), on `dp-nes`:
  1. Launch a game from ForgeDash → **no RGUI appears** at any point.
  2. **Select+Start → returns to ForgeDash** (verify before trusting other binds).
  3. Select+A saves (toast); Select+B loads the saved state (toast).
  4. Select+Right / Left changes slot (toast).
  5. Power-cycle → device boots bone stock (zero NAND writes).

## Out of scope

ForgeDash visual screens, per-game/per-core overrides, save-state browser, cheats,
shaders, controls remap, disk control — all later slices per the roadmap. This
slice ships only the headless config + global hotkeys.
