# ForgeDash Headless-RetroArch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make RetroArch run headless under ForgeDash — no RGUI ever — with in-game exit/save/load/slot on Select-modified NES pad hotkeys, driven by a ForgeDash-generated RetroArch override config.

**Architecture:** Add a pure config-builder (`ra_override_cfg`) + a `PadMap` of Clovercon button indices to `forgedash-core::launch`; ForgeDash writes `ra-override.cfg` next to the handoff on launch; `forge-loop.sh` appends that config when starting RetroArch. All logic is pure + unit-tested; the app wiring and script change are verified on hardware.

**Tech Stack:** Rust (`forgedash-core` pure lib), POSIX shell (`forge-loop.sh`), RetroArch 1.7.0 config keys.

**Spec:** `docs/superpowers/specs/2026-06-06-forgedash-headless-ra-design.md`
**Roadmap:** `docs/superpowers/forgedash-ra-bubbleup-roadmap.md`

**Clovercon button indices (from RA udev autoconfig, verified on dp-nes):**
A=0, B=1, Select=8, Start=9, D-pad Left=11, Right=12, Up=13, Down=14 (plain `_btn`).

---

## File Structure

```
forgedash/core/src/launch.rs   # + PadMap, ra_override_cfg(), write_ra_override()  (pure + fs; unit-tested)
forgedash/app/src/main.rs      # Confirm arm also writes ra-override.cfg           (HW-verified)
forgedash/scripts/forge-loop.sh# --appendconfig points at the ForgeDash-owned cfg  (HW-verified)
docs/HARDWARE-ACCEPTANCE-FORGEDASH.md  # + headless acceptance section
```

---

## Task 1: PadMap + RA override config builder (PURE, TDD)

**Files:**
- Modify: `forgedash/core/src/launch.rs`

- [ ] **Step 1: Write the failing test**

Append this test inside the existing `#[cfg(test)] mod tests { ... }` block in `forgedash/core/src/launch.rs` (alongside `handoff_line_is_tab_separated`):

```rust
    #[test]
    fn override_has_headless_and_clovercon_hotkeys() {
        let c = ra_override_cfg(&PadMap::clovercon());
        assert!(c.contains("menu_driver = \"null\""));
        assert!(c.contains("input_joypad_driver = \"udev\""));
        assert!(c.contains("input_driver = \"udev\""));
        assert!(c.contains("fps_show = \"false\""));
        assert!(c.contains("input_enable_hotkey_btn = \"8\""));   // Select
        assert!(c.contains("input_exit_emulator_btn = \"9\""));   // Start
        assert!(c.contains("input_save_state_btn = \"0\""));      // A
        assert!(c.contains("input_load_state_btn = \"1\""));      // B
        assert!(c.contains("input_state_slot_increase_btn = \"12\"")); // D-pad Right
        assert!(c.contains("input_state_slot_decrease_btn = \"11\"")); // D-pad Left
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core launch::`
Expected: FAIL to compile — `PadMap` / `ra_override_cfg` not found.

- [ ] **Step 3: Implement PadMap + ra_override_cfg + write_ra_override**

In `forgedash/core/src/launch.rs`, add above the `#[cfg(test)]` block (keep the existing `handoff_line` / `write_handoff`):

```rust
/// RetroArch joypad button indices for a controller (from RA's udev autoconfig).
/// Defaults are the Nintendo Clovercon (NES Classic), verified on dp-nes.
pub struct PadMap {
    pub enable_hotkey: u8, // Select
    pub exit: u8,          // Start
    pub save: u8,          // A
    pub load: u8,          // B
    pub slot_inc: u8,      // D-pad Right
    pub slot_dec: u8,      // D-pad Left
}

impl PadMap {
    pub fn clovercon() -> PadMap {
        PadMap { enable_hotkey: 8, exit: 9, save: 0, load: 1, slot_inc: 12, slot_dec: 11 }
    }
}

/// Build the RetroArch override config that makes RA headless (no RGUI) and binds
/// in-game hotkeys (Select held = enable_hotkey). Appended via `--appendconfig`.
pub fn ra_override_cfg(pad: &PadMap) -> String {
    format!(
        "input_joypad_driver = \"udev\"\n\
         input_driver = \"udev\"\n\
         menu_driver = \"null\"\n\
         fps_show = \"false\"\n\
         menu_enable_widgets = \"false\"\n\
         menu_show_load_content_animation = \"false\"\n\
         input_enable_hotkey_btn = \"{hk}\"\n\
         input_exit_emulator_btn = \"{exit}\"\n\
         input_save_state_btn = \"{save}\"\n\
         input_load_state_btn = \"{load}\"\n\
         input_state_slot_increase_btn = \"{inc}\"\n\
         input_state_slot_decrease_btn = \"{dec}\"\n",
        hk = pad.enable_hotkey,
        exit = pad.exit,
        save = pad.save,
        load = pad.load,
        inc = pad.slot_inc,
        dec = pad.slot_dec,
    )
}

/// Write `<dir>/ra-override.cfg` for forge-loop.sh to `--appendconfig`.
pub fn write_ra_override(dir: &Path, pad: &PadMap) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("ra-override.cfg"), ra_override_cfg(pad))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core launch::`
Expected: PASS (2 tests: `handoff_line_is_tab_separated`, `override_has_headless_and_clovercon_hotkeys`).

- [ ] **Step 5: Run full core suite (regression)**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core`
Expected: 12 passed (11 prior + 1 new).

- [ ] **Step 6: Commit**

```bash
git add forgedash/core/src/launch.rs
git commit -m "feat(forgedash): headless RA override cfg builder + PadMap"
```

---

## Task 2: Write ra-override.cfg on launch (app wiring)

> The `forgedash` app crate links SDL2 and only cross-builds with the device toolchain (`forgedash/dockerbuild.sh`); it does not compile in a plain host environment. Verify this change by reading + the Task 4 hardware run. Do NOT attempt a host `cargo build -p forgedash`.

**Files:**
- Modify: `forgedash/app/src/main.rs`

- [ ] **Step 1: Add the override write to the Confirm arm**

In `forgedash/app/src/main.rs`, find the `platform::Nav::Confirm` arm inside the main loop. It currently reads:

```rust
            platform::Nav::Confirm => {
                let g = &lib.games[shelf.selected];
                let _ = launch::write_handoff(handoff_dir, &g.rom_path, &g.core);
                println!("launching {} ({})", g.title, g.core);
                break; // forge-loop.sh takes over from here
            }
```

Replace it with (adds the override write before the handoff):

```rust
            platform::Nav::Confirm => {
                let g = &lib.games[shelf.selected];
                // ForgeDash owns RA's runtime config: headless + in-game hotkeys.
                let _ = launch::write_ra_override(handoff_dir, &launch::PadMap::clovercon());
                let _ = launch::write_handoff(handoff_dir, &g.rom_path, &g.core);
                println!("launching {} ({})", g.title, g.core);
                break; // forge-loop.sh takes over from here
            }
```

(`launch` is already imported via `use forgedash_core::{gfx, launch, model, ui};`; `PadMap`/`write_ra_override` are reached as `launch::PadMap` / `launch::write_ra_override`. `handoff_dir` is the existing `Path::new("/tmp/forge")`.)

- [ ] **Step 2: Verify by reading**

Re-read the Confirm arm and confirm: `write_ra_override(handoff_dir, &launch::PadMap::clovercon())` is called before `write_handoff`, both use `handoff_dir`, and no other line changed. (Host build is not possible — sdl2/device toolchain only.)

- [ ] **Step 3: Commit**

```bash
git add forgedash/app/src/main.rs
git commit -m "feat(forgedash): write ra-override.cfg on game launch"
```

---

## Task 3: Point forge-loop.sh at the ForgeDash override (with guard)

**Files:**
- Modify: `forgedash/scripts/forge-loop.sh`

- [ ] **Step 1: Replace the RetroArch launch block**

In `forgedash/scripts/forge-loop.sh`, the current launch (inside `if [ -f "$HANDOFF" ]; then`) is:

```sh
        CORE=$(cut -f1 "$HANDOFF")
        ROM=$(cut -f2 "$HANDOFF")
        setsid env HOME="$RA_ROOT/etc/libretro" LD_LIBRARY_PATH=/usr/lib \
            "$RA_ROOT/bin/retroarch" \
            -c "$RA_ROOT/etc/libretro/retroarch.cfg" \
            --appendconfig /tmp/ra-input.cfg \
            -L "$RA_ROOT/etc/libretro/core/${CORE}_libretro.so" \
            "$FORGE_ROOT/$ROM" \
            </dev/null >"$FORGE_ROOT/ra.log" 2>&1
        RA_STATUS=$?
```

Replace it with (use the ForgeDash-owned override; guard if absent):

```sh
        CORE=$(cut -f1 "$HANDOFF")
        ROM=$(cut -f2 "$HANDOFF")
        APPEND=""
        [ -f "$FORGE_ROOT/ra-override.cfg" ] && APPEND="--appendconfig $FORGE_ROOT/ra-override.cfg"
        setsid env HOME="$RA_ROOT/etc/libretro" LD_LIBRARY_PATH=/usr/lib \
            "$RA_ROOT/bin/retroarch" \
            -c "$RA_ROOT/etc/libretro/retroarch.cfg" \
            $APPEND \
            -L "$RA_ROOT/etc/libretro/core/${CORE}_libretro.so" \
            "$FORGE_ROOT/$ROM" \
            </dev/null >"$FORGE_ROOT/ra.log" 2>&1
        RA_STATUS=$?
```

- [ ] **Step 2: Verify LF + content**

Run: `file forgedash/scripts/forge-loop.sh; grep -n "appendconfig" forgedash/scripts/forge-loop.sh`
Expected: no CRLF; the only `appendconfig` line references `$FORGE_ROOT/ra-override.cfg`. If the file shows CRLF, normalize: `tr -d '\r' < forgedash/scripts/forge-loop.sh > /tmp/f && mv /tmp/f forgedash/scripts/forge-loop.sh`.

- [ ] **Step 3: Commit**

```bash
git add forgedash/scripts/forge-loop.sh
git commit -m "feat(forgedash): forge-loop appends ForgeDash-owned ra-override.cfg"
```

---

## Task 4: Hardware-acceptance — headless section

**Files:**
- Modify: `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`

- [ ] **Step 1: Append the headless acceptance section**

Append to `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`:

```markdown

## Headless-RA slice acceptance (2026-06 slice 2)

Build: `docker run --rm -v <repo>:/work -w /work forgedash-build bash /work/forgedash/dockerbuild.sh`.
Stage: push the new `forgedash` binary + `forge-loop.sh` to `/tmp/forge` (see
`scripts/stage.sh` / the prior runbook). No need to stage `/tmp/ra-input.cfg` any
more — ForgeDash writes `/tmp/forge/ra-override.cfg` itself on launch.

Run `sh /tmp/forge/forge-loop.sh`, then on the controller:

- [ ] Launch a game (A) → **RetroArch shows NO menu** at any point (no RGUI).
- [ ] **Select+Start → returns to ForgeDash** (verify this FIRST — it is the only
      way out; if the exit bind is wrong, power-cycle to recover).
- [ ] Select+A → save state; brief toast appears.
- [ ] Select+B → load state; the save is restored; toast appears.
- [ ] Select+Right / Select+Left → save slot changes; toast shows the slot.
- [ ] Power-cycle → device boots bone stock (zero NAND writes).

If RA errors on `menu_driver = "null"` (check `/tmp/forge/ra.log`): fallback is to
set `menu_driver = "rgui"` in `ra_override_cfg` (RGUI present but never opened; the
exit hotkey still works). Re-build + re-test.
```

- [ ] **Step 2: Commit**

```bash
git add docs/HARDWARE-ACCEPTANCE-FORGEDASH.md
git commit -m "docs(forgedash): headless-RA hardware acceptance steps"
```

---

## Notes for the implementer

- **TDD:** Task 1 is pure logic — red/green with `cargo test -p forgedash-core`. Tasks 2–3 are device-only (app links SDL2; script runs on BusyBox) — verified by reading + the Task 4 hardware run, not a host build.
- **Order to verify on hardware:** the **exit combo first** — it is the only way out of a headless game; everything else is safe to try after.
- **Keep it pure where possible:** `ra_override_cfg` returns a `String` so config generation stays unit-tested off-device; this is the same pattern future bubble-up slices (core options, remaps, cheats) will extend.
- **Brick safety:** nothing writes NAND; all RAM-memboot.
```
