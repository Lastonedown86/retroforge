# ForgeDash Slice — Save-State Browser (Design)

> Third ForgeDash slice (roadmap item 2). Bubbles RetroArch save-state
> management into ForgeDash: a per-game screen to browse slots (with screenshot
> thumbnails), load any slot, and delete slots — no RGUI. Builds on the headless
> slice. Roadmap: `docs/superpowers/forgedash-ra-bubbleup-roadmap.md`.

## Goal & success criteria

From the coverflow, **D-pad Up** opens a save-state screen for the selected game:
a slot list on the left, a large screenshot preview of the highlighted slot on
the right (layout A from brainstorming). The player can, with the NES pad:
- pick a slot (Up/Down),
- **load** it (A) — the game launches resuming that exact state,
- **delete** it (Select → confirm),
- go **back** (B).

A on the coverflow still launches instantly (RA auto-loads the latest auto-save).
Proven on hardware (`dp-nes`), zero NAND writes.

## Background (device reality, verified)

RA config already has `savestate_auto_save = "true"`, `savestate_auto_load =
"true"`, `savestate_directory = "default"` (states sit next to the ROM),
`savestate_thumbnail_enable = "false"`. Save files observed:
`/tmp/forge/roms/g1.state` (manual slot 0), `g1.state.auto` (auto-save on exit),
`g2.state.auto`, etc. Manual slots are `<stem>.state` (slot 0), `<stem>.state1`,
`<stem>.state2`, …; the auto-save is `<stem>.state.auto`.

**Load constraint:** RA only auto-loads the `.auto` file at content start; it will
not auto-load an arbitrary slot. So "load slot N" is implemented by ForgeDash
**copying `<stem>.stateN` → `<stem>.state.auto`** before launch; RA's auto-load
then restores it. (The auto slot loads itself; A on the coverflow already does
this for the latest state.)

## Decisions

| Decision | Choice |
|---|---|
| Layout | List of slots + large preview of the highlighted slot |
| Open gesture | Coverflow D-pad Up → SaveStates(game); B → back |
| Browser controls | Up/Down pick, A load+launch, Select delete (confirm), B back |
| Load slot N | Copy `<stem>.stateN` → `<stem>.state.auto`, then launch (RA auto-load) |
| Thumbnails | Enable `savestate_thumbnail_enable = "true"` in `ra_override_cfg`; RA writes `<state>.png` going forward; missing → placeholder |
| Coverflow A | Unchanged — instant launch (auto-resume) |

## Architecture & components

- **`forgedash-core::savestate` (new module, pure, unit-tested).**
  - `SaveSlot { kind: SlotKind, exists: bool, state_path: String, thumb_path: String }`
    where `SlotKind = Auto | Manual(u8)`.
  - `slots_for(rom_path: &str, dir_listing: &[String]) -> Vec<SaveSlot>` — given a
    ROM path and the file names in its directory, return the ordered slot list
    (Auto first, then Manual 0,1,2,…). Recognizes `<stem>.state`, `<stem>.stateN`,
    `<stem>.state.auto`; ignores unrelated files. Always includes Auto and Slot 0
    entries even if absent (`exists=false`) so the player sees empty slots.
  - Path helpers (pure): `state_path(rom, kind)`, `auto_path(rom)`,
    `thumb_path(state_path)` (= `state_path + ".png"`).
  - All of the above unit-tested off-device.
- **`forgedash-core::launch::ra_override_cfg`** — add
  `savestate_thumbnail_enable = "true"`. (Test updated.)
- **`app/src/main.rs`** — introduce a screen state machine:
  `enum Screen { Coverflow, SaveStates { game: usize, sel: usize } }`. Coverflow
  Up → SaveStates. SaveStates: render the slot list (text rows via `gfx`) + a
  large thumbnail quad (via `gfx::Texture::from_png`, placeholder if missing) +
  control hints; input Up/Down/A/Select/B. Filesystem actions (copy slot→auto on
  load; remove slot file + its `.png` on delete) use `std::fs`. App-only →
  hardware-verified (not host-compilable; SDL2/device toolchain).

Data flow (load): SaveStates A → `std::fs::copy(slot_state, auto_path)` →
`launch::write_ra_override` + `launch::write_handoff` → exit → forge-loop runs RA
→ auto-load restores the state.

## Error handling

- ROM directory unreadable / no slots → render an "empty / no saves" message; B
  returns to coverflow.
- Missing thumbnail → placeholder quad (no error).
- `std::fs::copy` / `remove_file` failure → on-screen error toast; stay in the
  browser; log to `forge.log`.
- Delete confirm: Select highlights "Delete? A=yes B=no"; only A removes.
- Deleting the auto slot is allowed; afterward A on the coverflow starts fresh.

## Testing

- **`cargo test -p forgedash-core`:**
  - `slots_for` parses a realistic listing (`g1.state`, `g1.state1`,
    `g1.state.auto`, plus noise like `g1.nes`, `g1.srm`) into Auto + Slot0 +
    Slot1 (+ absent Slot markers as specified), ignoring noise.
  - path helpers: `auto_path`, `state_path(Manual(2))`, `thumb_path` produce the
    expected strings.
  - `ra_override_cfg` contains `savestate_thumbnail_enable = "true"`.
- **Hardware acceptance** (append to `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md`),
  on `dp-nes`:
  1. In a game, Select+A save a couple of times → exit.
  2. Coverflow → **Up** on that game → save-state screen opens; slots listed;
     newly-made saves show **thumbnails**.
  3. Up/Down highlights slots; preview updates.
  4. **A** on a slot → game launches resuming that exact state.
  5. **Select** → confirm → slot deleted (gone on re-open).
  6. **B** → back to coverflow. Power-cycle → bone stock (zero NAND writes).

## Out of scope

Slot rename/labels, cross-game import, host/cloud sync, SRAM (`.srm`) management,
auto-thumbnail backfill for pre-existing saves (only new saves get thumbnails).
Per-game core options / remaps / cheats — later roadmap slices.
