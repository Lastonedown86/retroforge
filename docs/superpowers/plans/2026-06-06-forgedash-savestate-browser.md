# ForgeDash Save-State Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-game save-state browser to ForgeDash — D-pad Up opens a slot list + preview; A loads a slot (game resumes that state), Select deletes (with confirm), B returns — with screenshot thumbnails, no RGUI.

**Architecture:** Pure slot model + path math go in a new `forgedash-core::savestate` module (unit-tested). `platform::Nav` gains Up/Down/Back/Delete. `app/main.rs` gets a `Screen` state machine (Coverflow ↔ SaveStates) and the browser render/input + filesystem actions (copy slot→`.auto` to load; `remove_file` to delete). `ra_override_cfg` enables save-state thumbnails.

**Tech Stack:** Rust (`forgedash-core` pure lib, `forgedash` app), SDL2/`glow` (device only), RetroArch save-state files.

**Spec:** `docs/superpowers/specs/2026-06-06-forgedash-savestate-browser-design.md`

**Verified evdev codes (Clovercon, raw):** A=304, B=305, Select=314, D-pad Left=704, Right=705, Up=706, Down=707. RA savestate naming (verified): content `g1.nes` → `g1.state` (slot 0), `g1.state1..N`, `g1.state.auto`; thumbnail = `<state>.png`.

---

## File Structure

```
forgedash/core/src/savestate.rs   # NEW: SlotKind, SaveSlot, slots_for(), path helpers  (pure, unit-tested)
forgedash/core/src/lib.rs         # + pub mod savestate;
forgedash/core/src/launch.rs      # ra_override_cfg(): + savestate_thumbnail_enable
forgedash/app/src/platform.rs     # Nav += Up/Down/Back/Delete; poll() maps them        (device)
forgedash/app/src/main.rs         # Screen state machine + SaveStates browser           (device)
docs/HARDWARE-ACCEPTANCE-FORGEDASH.md  # + save-state browser acceptance
```

---

## Task 1: savestate module — slot model + path math (PURE, TDD)

**Files:**
- Create: `forgedash/core/src/savestate.rs`
- Modify: `forgedash/core/src/lib.rs`

- [ ] **Step 1: Register the module**

In `forgedash/core/src/lib.rs`, add a line (keep the existing `pub mod` lines):

```rust
pub mod savestate;
```

- [ ] **Step 2: Write the failing tests**

Create `forgedash/core/src/savestate.rs`:

```rust
//! Pure RetroArch save-state slot model + path math. No I/O.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlotKind {
    Auto,
    Manual(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaveSlot {
    pub kind: SlotKind,
    pub exists: bool,
    pub state_path: String,
    pub thumb_path: String,
}

/// Split a ROM path into (dir-with-trailing-slash, stem-without-extension).
fn split_dir_stem(rom_path: &str) -> (String, String) {
    let (dir, file) = match rom_path.rfind('/') {
        Some(i) => (&rom_path[..=i], &rom_path[i + 1..]),
        None => ("", rom_path),
    };
    let stem = match file.rfind('.') {
        Some(j) => &file[..j],
        None => file,
    };
    (dir.to_string(), stem.to_string())
}

/// Path of the auto-save for a ROM (`<dir><stem>.state.auto`).
pub fn auto_path(rom_path: &str) -> String {
    let (d, s) = split_dir_stem(rom_path);
    format!("{d}{s}.state.auto")
}

/// Path of a given slot's state file.
pub fn state_path(rom_path: &str, kind: SlotKind) -> String {
    let (d, s) = split_dir_stem(rom_path);
    match kind {
        SlotKind::Auto => format!("{d}{s}.state.auto"),
        SlotKind::Manual(0) => format!("{d}{s}.state"),
        SlotKind::Manual(n) => format!("{d}{s}.state{n}"),
    }
}

/// Thumbnail path for a state file (`<state>.png`).
pub fn thumb_path(state_path: &str) -> String {
    format!("{state_path}.png")
}

/// Build the ordered slot list for a ROM, given the file names in its directory.
/// Always includes Auto and Slot 0 (even if absent), then Manual(1..=max existing).
pub fn slots_for(rom_path: &str, dir_listing: &[String]) -> Vec<SaveSlot> {
    let (_d, stem) = split_dir_stem(rom_path);
    let has = |path: &str| {
        let fname = path.rsplit('/').next().unwrap_or(path);
        dir_listing.iter().any(|f| f == fname)
    };
    let mk = |kind: SlotKind| {
        let sp = state_path(rom_path, kind);
        SaveSlot {
            kind,
            exists: has(&sp),
            thumb_path: thumb_path(&sp),
            state_path: sp,
        }
    };

    let mut out = vec![mk(SlotKind::Auto), mk(SlotKind::Manual(0))];

    // Highest existing manual slot N>0: file is "<stem>.stateN" where N parses as a number.
    let prefix = format!("{stem}.state");
    let mut max_n = 0u8;
    for f in dir_listing {
        if let Some(rest) = f.strip_prefix(&prefix) {
            if let Ok(n) = rest.parse::<u8>() {
                if n > max_n {
                    max_n = n;
                }
            }
        }
    }
    for n in 1..=max_n {
        out.push(mk(SlotKind::Manual(n)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers() {
        assert_eq!(auto_path("roms/g1.nes"), "roms/g1.state.auto");
        assert_eq!(state_path("roms/g1.nes", SlotKind::Manual(0)), "roms/g1.state");
        assert_eq!(state_path("roms/g1.nes", SlotKind::Manual(2)), "roms/g1.state2");
        assert_eq!(state_path("roms/g1.nes", SlotKind::Auto), "roms/g1.state.auto");
        assert_eq!(thumb_path("roms/g1.state"), "roms/g1.state.png");
        // no directory prefix
        assert_eq!(state_path("g1.nes", SlotKind::Manual(0)), "g1.state");
    }

    #[test]
    fn slots_for_parses_listing_and_ignores_noise() {
        let listing: Vec<String> = [
            "g1.nes", "g1.state", "g1.state.auto", "g1.state1",
            "g1.srm", "g1.state1.png", "g2.state",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let slots = slots_for("roms/g1.nes", &listing);

        // Auto, Slot0, Slot1 — all exist; g2.state / g1.srm / g1.nes ignored.
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].kind, SlotKind::Auto);
        assert!(slots[0].exists);
        assert_eq!(slots[0].state_path, "roms/g1.state.auto");
        assert_eq!(slots[1].kind, SlotKind::Manual(0));
        assert!(slots[1].exists);
        assert_eq!(slots[2].kind, SlotKind::Manual(1));
        assert!(slots[2].exists);
        assert_eq!(slots[2].thumb_path, "roms/g1.state1.png");
    }

    #[test]
    fn slots_for_empty_still_has_auto_and_slot0() {
        let listing = vec!["g1.nes".to_string()];
        let slots = slots_for("roms/g1.nes", &listing);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].kind, SlotKind::Auto);
        assert!(!slots[0].exists);
        assert_eq!(slots[1].kind, SlotKind::Manual(0));
        assert!(!slots[1].exists);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core savestate::`
Expected: compile error first run? No — the file is complete. Expected: the 3 tests run and PASS immediately (this module is self-contained). If you want a strict red phase, temporarily change `out` to `vec![]` in `slots_for`, watch the two slots_for tests fail, then restore. Either way, end with all 3 passing.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core savestate::`
Expected: 3 passed.

- [ ] **Step 5: Run full core suite (regression)**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core`
Expected: 15 passed (12 prior + 3 new).

- [ ] **Step 6: Commit**

```bash
git add forgedash/core/src/savestate.rs forgedash/core/src/lib.rs
git commit -m "feat(forgedash): save-state slot model + path math"
```

---

## Task 2: Enable save-state thumbnails in the RA override (PURE, TDD)

**Files:**
- Modify: `forgedash/core/src/launch.rs`

- [ ] **Step 1: Add the failing assertion**

In `forgedash/core/src/launch.rs`, inside the `override_has_headless_and_clovercon_hotkeys` test, add one assertion (after the existing `fps_show` assert):

```rust
        assert!(c.contains("savestate_thumbnail_enable = \"true\""));
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core launch::`
Expected: FAIL (string not present).

- [ ] **Step 3: Add the config line**

In `ra_override_cfg` (in `forgedash/core/src/launch.rs`), add a line to the format string after `menu_show_load_content_animation = \"false\"\n\`:

```rust
         savestate_thumbnail_enable = \"true\"\n\
```

(So the block now contains `...menu_show_load_content_animation = "false"` then `savestate_thumbnail_enable = "true"` then the `input_enable_hotkey_btn` line. Keep the trailing `\` line-continuations intact.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core launch::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add forgedash/core/src/launch.rs
git commit -m "feat(forgedash): enable RA save-state thumbnails in override cfg"
```

---

## Task 3: Extend Nav with Up/Down/Back/Delete (app, device-verified)

> The `forgedash` app crate links SDL2 and only cross-builds with the device toolchain (`forgedash/dockerbuild.sh`); it does not compile on this host. Verify by reading. Do NOT run a host `cargo build -p forgedash`.

**Files:**
- Modify: `forgedash/app/src/platform.rs`

- [ ] **Step 1: Extend the Nav enum**

In `forgedash/app/src/platform.rs`, replace the `Nav` enum with:

```rust
/// Abstract navigation events, mapped from keyboard (dev) and the raw evdev pad (device).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nav {
    None,
    Left,
    Right,
    Up,
    Down,
    Confirm,
    Back,
    Delete,
    Quit,
}
```

- [ ] **Step 2: Add the new button codes + map them**

In `forgedash/app/src/platform.rs`, update the evdev button-code constants block. It currently is:

```rust
const EV_KEY: u16 = 1;
const CODE_A: u16 = 304; // BTN_SOUTH
const CODE_LEFT: u16 = 704; // clovercon d-pad left  (BTN_TRIGGER_HAPPY1)
const CODE_RIGHT: u16 = 705; // clovercon d-pad right (BTN_TRIGGER_HAPPY2)
const O_NONBLOCK: i32 = 0o4000;
```

Replace it with:

```rust
const EV_KEY: u16 = 1;
const CODE_A: u16 = 304; // BTN_SOUTH  -> Confirm
const CODE_B: u16 = 305; // BTN_EAST   -> Back
const CODE_SELECT: u16 = 314; // BTN_SELECT -> Delete
const CODE_LEFT: u16 = 704; // d-pad left
const CODE_RIGHT: u16 = 705; // d-pad right
const CODE_UP: u16 = 706; // d-pad up
const CODE_DOWN: u16 = 707; // d-pad down
const O_NONBLOCK: i32 = 0o4000;
```

Then in `poll()`, update BOTH branches:

Keyboard branch — replace the `Event::KeyDown { keycode: Some(k), .. } => match k { ... }` arm with:

```rust
                Event::KeyDown { keycode: Some(k), .. } => match k {
                    Keycode::Left => result = Nav::Left,
                    Keycode::Right => result = Nav::Right,
                    Keycode::Up => result = Nav::Up,
                    Keycode::Down => result = Nav::Down,
                    Keycode::Return | Keycode::Space => result = Nav::Confirm,
                    Keycode::Backspace => result = Nav::Back,
                    Keycode::Delete => result = Nav::Delete,
                    Keycode::Escape => return Nav::Quit,
                    _ => {}
                },
```

Evdev branch — replace the `if etype == EV_KEY && value == 1 { match code { ... } }` block with:

```rust
                if etype == EV_KEY && value == 1 {
                    match code {
                        CODE_LEFT => result = Nav::Left,
                        CODE_RIGHT => result = Nav::Right,
                        CODE_UP => result = Nav::Up,
                        CODE_DOWN => result = Nav::Down,
                        CODE_A => result = Nav::Confirm,
                        CODE_B => result = Nav::Back,
                        CODE_SELECT => result = Nav::Delete,
                        _ => {}
                    }
                }
```

- [ ] **Step 3: Verify by reading**

Confirm: `Nav` has the 9 variants; both poll branches map them; `CODE_B`/`CODE_SELECT`/`CODE_UP`/`CODE_DOWN` are defined and used. (Host build not possible.)

- [ ] **Step 4: Confirm core still builds**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core`
Expected: 15 passed (you didn't touch core; sanity check).

- [ ] **Step 5: Commit**

```bash
git add forgedash/app/src/platform.rs
git commit -m "feat(forgedash): Nav adds Up/Down/Back/Delete (Clovercon codes)"
```

---

## Task 4: Screen state machine + save-state browser (app, device-verified)

> App-only (SDL2/device). Verify by reading + the Task 5 hardware run.

**Files:**
- Modify: `forgedash/app/src/main.rs`

- [ ] **Step 1: Replace the entire contents of `forgedash/app/src/main.rs`**

```rust
mod platform;

use forgedash_core::{gfx, launch, model, savestate, ui};
use glow::HasContext;
use std::path::Path;

const CARD_W: f32 = 240.0;
const CARD_H: f32 = 320.0;
const CENTER_X: f32 = 640.0;
const CENTER_Y: f32 = 360.0;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Coverflow,
    SaveStates,
}

fn slot_label(s: &savestate::SaveSlot) -> String {
    let name = match s.kind {
        savestate::SlotKind::Auto => "Auto".to_string(),
        savestate::SlotKind::Manual(n) => format!("Slot {n}"),
    };
    if s.exists {
        name
    } else {
        format!("{name}  (empty)")
    }
}

/// Read the ROM's directory and build its slot list.
fn load_slots(rom_path: &str) -> Vec<savestate::SaveSlot> {
    let dir = Path::new(rom_path).parent().unwrap_or_else(|| Path::new("."));
    let listing: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    savestate::slots_for(rom_path, &listing)
}

fn main() {
    let lib_path = std::env::args().nth(1).unwrap_or_else(|| "library.json".to_string());
    let lib = match std::fs::read_to_string(&lib_path)
        .map_err(|e| e.to_string())
        .and_then(|s| model::Library::from_json(&s).map_err(|e| format!("{:?}", e)))
    {
        Ok(l) => l,
        Err(e) => {
            run_error_screen(&format!("library error: {e}"));
            return;
        }
    };

    let mut plat = platform::Platform::new(1280, 720).unwrap_or_else(|e| {
        eprintln!("forgedash: EGL/platform init failed: {e}");
        std::process::exit(1);
    });
    let font = include_bytes!("../assets/font.ttf");
    let r = gfx::Renderer::new(&plat.gl, plat.width, plat.height, font);

    let mut titles = Vec::new();
    let mut metas = Vec::new();
    let mut covers: Vec<Option<gfx::Texture>> = Vec::new();
    for g in &lib.games {
        titles.push(r.text_texture(&plat.gl, &g.title, 40.0));
        metas.push(r.text_texture(&plat.gl, &format!("{} · {} · {}", g.year, g.publisher, g.players), 22.0));
        covers.push(gfx::Texture::from_png(&plat.gl, &g.cover_path));
    }

    let no_pad_hint = if !plat.controller_present {
        Some(r.text_texture(&plat.gl, "No controller detected — keyboard only", 22.0))
    } else {
        None
    };

    let ra_exit_path = Path::new("/tmp/forge/ra_exit");
    let ra_toast = std::fs::read_to_string(ra_exit_path).ok().map(|s| {
        let _ = std::fs::remove_file(ra_exit_path);
        r.text_texture(&plat.gl, &format!("RetroArch exited with code {}", s.trim()), 22.0)
    });
    let mut frame: u32 = 0;

    let mut shelf = ui::Shelf::new(lib.games.len());
    let mut anim_pos = 0.0f32;
    let handoff_dir = Path::new("/tmp/forge");
    let mut screen = Screen::Coverflow;
    let mut ss_sel: usize = 0;
    let mut ss_confirm_delete = false;

    // SaveStates view state (rebuilt on entering / after delete).
    let mut slots: Vec<savestate::SaveSlot> = Vec::new();
    let mut slot_labels: Vec<gfx::Texture> = Vec::new();
    let mut preview: Option<gfx::Texture> = None;
    let mut preview_for: i64 = -1;
    let mut hdr: Option<gfx::Texture> = None;

    loop {
        let nav = plat.poll();
        if nav == platform::Nav::Quit {
            break;
        }
        frame += 1;

        match screen {
            Screen::Coverflow => {
                match nav {
                    platform::Nav::Left => shelf.move_left(),
                    platform::Nav::Right => shelf.move_right(),
                    platform::Nav::Up => {
                        let g = &lib.games[shelf.selected];
                        slots = load_slots(&g.rom_path);
                        slot_labels = slots
                            .iter()
                            .map(|s| r.text_texture(&plat.gl, &slot_label(s), 28.0))
                            .collect();
                        hdr = Some(r.text_texture(&plat.gl, &format!("{} — Save States", g.title), 34.0));
                        preview = None;
                        preview_for = -1;
                        ss_sel = 0;
                        ss_confirm_delete = false;
                        screen = Screen::SaveStates;
                    }
                    platform::Nav::Confirm => {
                        let g = &lib.games[shelf.selected];
                        let _ = launch::write_ra_override(handoff_dir, &launch::PadMap::clovercon());
                        let _ = launch::write_handoff(handoff_dir, &g.rom_path, &g.core);
                        println!("launching {} ({})", g.title, g.core);
                        break;
                    }
                    _ => {}
                }

                let target = shelf.selected as f32;
                anim_pos += (target - anim_pos) * 0.25;
                if (target - anim_pos).abs() < 0.001 {
                    anim_pos = target;
                }

                unsafe {
                    plat.gl.clear_color(0.04, 0.06, 0.11, 1.0);
                    plat.gl.clear(glow::COLOR_BUFFER_BIT);
                }
                r.begin(&plat.gl);

                if let Some(h) = &no_pad_hint {
                    r.draw_texture(&plat.gl, h, 40.0, 40.0, h.w as f32, h.h as f32, 0.85);
                }
                if frame < 180 {
                    if let Some(t) = &ra_toast {
                        r.draw_texture(&plat.gl, t, 40.0, 80.0, t.w as f32, t.h as f32, 0.9);
                    }
                }

                r.fill_rect(&plat.gl, CENTER_X - 200.0, CENTER_Y - 240.0, 400.0, 480.0, [0.23, 0.51, 0.96, 0.18]);

                let mut order: Vec<usize> = (0..lib.games.len()).collect();
                order.sort_by(|&a, &b| {
                    let da = (a as f32 - anim_pos).abs();
                    let db = (b as f32 - anim_pos).abs();
                    db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
                });
                for &i in &order {
                    let offset = i as f32 - anim_pos;
                    let slot = ui::slot_for(offset.round() as i32);
                    let x = CENTER_X + offset * ui::CARD_SPACING;
                    let w = CARD_W * slot.scale;
                    let h = CARD_H * slot.scale;
                    if slot.opacity <= 0.0 {
                        continue;
                    }
                    let cx = x - w / 2.0;
                    let cy = CENTER_Y - h / 2.0;
                    match &covers[i] {
                        Some(tex) => r.draw_texture(&plat.gl, tex, cx, cy, w, h, slot.opacity),
                        None => r.fill_rect(&plat.gl, cx, cy, w, h, [0.27, 0.33, 0.42, slot.opacity]),
                    }
                }

                let sel = shelf.selected;
                let t = &titles[sel];
                r.draw_texture(&plat.gl, t, CENTER_X - t.w as f32 / 2.0, CENTER_Y + 200.0, t.w as f32, t.h as f32, 1.0);
                let m = &metas[sel];
                r.draw_texture(&plat.gl, m, CENTER_X - m.w as f32 / 2.0, CENTER_Y + 250.0, m.w as f32, m.h as f32, 0.8);

                plat.present();
                std::thread::sleep(std::time::Duration::from_millis(16));
            }

            Screen::SaveStates => {
                if ss_confirm_delete {
                    match nav {
                        platform::Nav::Confirm => {
                            if let Some(slot) = slots.get(ss_sel) {
                                if slot.exists {
                                    let _ = std::fs::remove_file(&slot.state_path);
                                    let _ = std::fs::remove_file(&slot.thumb_path);
                                }
                            }
                            let g = &lib.games[shelf.selected];
                            slots = load_slots(&g.rom_path);
                            slot_labels = slots
                                .iter()
                                .map(|s| r.text_texture(&plat.gl, &slot_label(s), 28.0))
                                .collect();
                            if ss_sel >= slots.len() && !slots.is_empty() {
                                ss_sel = slots.len() - 1;
                            }
                            preview_for = -1;
                            ss_confirm_delete = false;
                        }
                        platform::Nav::Back => {
                            ss_confirm_delete = false;
                        }
                        _ => {}
                    }
                } else {
                    match nav {
                        platform::Nav::Back => {
                            screen = Screen::Coverflow;
                            continue;
                        }
                        platform::Nav::Up => {
                            if ss_sel > 0 {
                                ss_sel -= 1;
                            }
                        }
                        platform::Nav::Down => {
                            if ss_sel + 1 < slots.len() {
                                ss_sel += 1;
                            }
                        }
                        platform::Nav::Confirm => {
                            if let Some(slot) = slots.get(ss_sel) {
                                if slot.exists {
                                    let g = &lib.games[shelf.selected];
                                    if slot.kind != savestate::SlotKind::Auto {
                                        let _ = std::fs::copy(&slot.state_path, savestate::auto_path(&g.rom_path));
                                    }
                                    let _ = launch::write_ra_override(handoff_dir, &launch::PadMap::clovercon());
                                    let _ = launch::write_handoff(handoff_dir, &g.rom_path, &g.core);
                                    println!("loading {} -> {}", g.title, slot.state_path);
                                    break;
                                }
                            }
                        }
                        platform::Nav::Delete => {
                            if slots.get(ss_sel).map(|s| s.exists).unwrap_or(false) {
                                ss_confirm_delete = true;
                            }
                        }
                        _ => {}
                    }
                }

                // Lazily (re)load the preview thumbnail for the highlighted slot.
                if preview_for != ss_sel as i64 {
                    preview = slots.get(ss_sel).and_then(|s| gfx::Texture::from_png(&plat.gl, &s.thumb_path));
                    preview_for = ss_sel as i64;
                }

                unsafe {
                    plat.gl.clear_color(0.04, 0.06, 0.11, 1.0);
                    plat.gl.clear(glow::COLOR_BUFFER_BIT);
                }
                r.begin(&plat.gl);

                if let Some(h) = &hdr {
                    r.draw_texture(&plat.gl, h, 60.0, 50.0, h.w as f32, h.h as f32, 1.0);
                }

                // Slot list (left column).
                let list_x = 60.0;
                let row_h = 46.0;
                let list_y0 = 140.0;
                for (i, label) in slot_labels.iter().enumerate() {
                    let y = list_y0 + i as f32 * row_h;
                    if i == ss_sel {
                        r.fill_rect(&plat.gl, list_x - 12.0, y - 6.0, 360.0, row_h - 8.0, [0.23, 0.51, 0.96, 0.85]);
                    } else {
                        r.fill_rect(&plat.gl, list_x - 12.0, y - 6.0, 360.0, row_h - 8.0, [0.12, 0.16, 0.22, 0.7]);
                    }
                    r.draw_texture(&plat.gl, label, list_x, y, label.w as f32, label.h as f32, 1.0);
                }

                // Preview (right).
                let pv_x = 460.0;
                let pv_y = 140.0;
                let pv_w = 720.0;
                let pv_h = 440.0;
                match &preview {
                    Some(tex) => r.draw_texture(&plat.gl, tex, pv_x, pv_y, pv_w, pv_h, 1.0),
                    None => r.fill_rect(&plat.gl, pv_x, pv_y, pv_w, pv_h, [0.16, 0.20, 0.27, 1.0]),
                }

                // Hint / confirm bar.
                if ss_confirm_delete {
                    r.fill_rect(&plat.gl, 60.0, 620.0, 1120.0, 60.0, [0.4, 0.06, 0.06, 0.95]);
                }

                plat.present();
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    }
}

fn run_error_screen(msg: &str) {
    eprintln!("forgedash: {msg}");
    let mut plat = match platform::Platform::new(1280, 720) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("forgedash: cannot open window: {e}");
            return;
        }
    };
    let font = include_bytes!("../assets/font.ttf");
    let r = gfx::Renderer::new(&plat.gl, plat.width, plat.height, font);
    let tex = r.text_texture(&plat.gl, msg, 32.0);
    loop {
        if plat.poll() == platform::Nav::Quit {
            break;
        }
        unsafe {
            plat.gl.clear_color(0.15, 0.02, 0.02, 1.0);
            plat.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        r.begin(&plat.gl);
        r.draw_texture(&plat.gl, &tex, 80.0, 320.0, tex.w as f32, tex.h as f32, 1.0);
        plat.present();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
```

- [ ] **Step 2: Verify by reading**

Confirm: imports include `savestate`; `Screen` enum + `slot_label`/`load_slots` helpers present; Coverflow `Up` builds the slot view and switches screen; SaveStates handles confirm-delete modal (A=confirm, B=cancel) vs normal (Up/Down/A-load/Select-delete/B-back); preview reloads when `sel` changes; both screens render + present. Every `forgedash_core` call matches the APIs from Tasks 1–2 and the existing `gfx`/`ui`/`launch`/`model` modules. (Host build not possible — sdl2.)

- [ ] **Step 3: Confirm core still builds/tests**

Run: `cargo test --manifest-path forgedash/Cargo.toml -p forgedash-core`
Expected: 15 passed.

- [ ] **Step 4: Commit**

```bash
git add forgedash/app/src/main.rs
git commit -m "feat(forgedash): save-state browser screen (list + preview, load/delete)"
```

---

## Task 5: Hardware-acceptance — save-state browser

**Files:**
- Modify: `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`

- [ ] **Step 1: Append the acceptance section**

Append to `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`:

```markdown

## Save-state browser slice acceptance (slice 3)

Build `forgedash/dockerbuild.sh`; push the new `forgedash` binary to `/tmp/forge`
(forge-loop.sh unchanged). ForgeDash now also writes `savestate_thumbnail_enable`
into `ra-override.cfg`, so save states made from this build get `.png` thumbnails.

- [ ] In a game: Select+A save once or twice; Select+Start exit.
- [ ] Coverflow: highlight that game, press **D-pad Up** → save-state screen opens
      (title "<game> — Save States", slot list left, preview right).
- [ ] Saves made from this build show a **thumbnail** in the preview; older saves
      show the solid placeholder.
- [ ] **Up/Down** moves the highlight; preview updates to the highlighted slot.
- [ ] **A** on an existing slot → game launches and resumes that state.
- [ ] **Select** on an existing slot → red confirm bar; **A** confirms (slot gone
      on re-open), **B** cancels.
- [ ] **B** → back to the coverflow.
- [ ] Power-cycle → bone stock (zero NAND writes).
```

- [ ] **Step 2: Commit**

```bash
git add docs/HARDWARE-ACCEPTANCE-FORGEDASH.md
git commit -m "docs(forgedash): save-state browser hardware acceptance"
```

---

## Notes for the implementer

- **TDD:** Tasks 1–2 are pure logic — red/green with `cargo test -p forgedash-core`. Tasks 3–4 touch the SDL2 app (device-only); verify by reading + the Task 5 hardware run.
- **Delete confirm is modal:** while `confirm_delete` is true, A = confirm delete, B = cancel (NOT back/load). This resolves the A=load / B=back overload.
- **Load mechanism:** copying the chosen slot to `<rom>.state.auto` is what makes RA resume it (RA auto-loads `.auto` at content start). Loading the Auto slot needs no copy.
- **Thumbnails are forward-only:** RA writes `.png` only for saves made after `savestate_thumbnail_enable` is set; pre-existing saves render the placeholder.
- **Brick safety:** nothing writes NAND; all RAM-memboot.
```
