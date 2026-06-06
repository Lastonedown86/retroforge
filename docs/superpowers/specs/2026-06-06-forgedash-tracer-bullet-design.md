# ForgeDash — Custom On-Device Dashboard, Tracer-Bullet Slice (Design)

> A custom game launcher that sits on top of RetroArch on the device: the
> frontend the player sees on the TV. Picks a game, hands off to RetroArch to
> run it, returns on quit. This document specifies the **first tracer-bullet
> slice** — the minimal end-to-end spine, proven on real hardware.

## Why a tracer bullet first

The repo builds in vertical slices, and one integration risk dwarfs the rest:
**can our own Rust + GLES2 binary obtain a Mali EGL surface and read the
controller on this device, the way RetroArch already does?** Everything else
(shelves, theming, settings, sync) is additive once that spine is proven. This
slice exists to kill that unknown end-to-end before committing to the full
launcher.

Prior proof we build on:
- `findings/2026-06-02-retroarch-rgui-spike.md` — bare memboot has no GPU; a
  GPU-capable kernel + matching modules is required.
- `findings/2026-06-03-controller-memboot-spike.md` — **SOLVED** brick-safe
  RAM-memboot bring-up that yields a working Mali GLES2 surface (RA RGUI
  @1280×720) and a udev-tagged controller, with **zero NAND writes**.

ForgeDash is a GLES2 app exactly like RetroArch, so the same bring-up that gives
RA a surface + pad serves ForgeDash.

## Resolved design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Architecture | **Separate native launcher that spawns RetroArch** | Full control of look/UX; RA stays the emulator backend. |
| Build tech | **Rust + SDL2 + GLES2 (`glow`)**, built from scratch | Reuses the core's Rust; dynamic-links the device's own `libSDL2`/`libMali` to mirror RA's proven dependency chain. |
| Layout | **Coverflow** | Cinematic, console-grade; big center cover, neighbors peeking, scroll L/R. |
| Organization | **Netflix shelves** (stacked coverflows) | Recently Played / Favorites / per-folder rows. *Slice = one shelf;* full set deferred. |
| Art direction | **Modern dark minimal** | Slate gradients, cover-glow, clean sans. Matches RetroForge's "modern tool" brand; box art is the hero; cheap on GLES2. |
| Process model | **Shell-loop (one GLES app at a time)** | Dashboard exits, script runs RA, relaunch on quit. Bulletproof on Mali EGL (avoids surface release/re-acquire BAD_ALLOC); matches hakchi. |
| Scope | **Tracer-bullet first slice** | De-risk Rust-GLES + pad + launch integration on hardware before the full product. |

## Slice scope

**In:** boot into ForgeDash under the proven brick-safe memboot → one coverflow
shelf of real games read from an on-device manifest → D-pad navigation by the
controller → press A to launch a ROM into RetroArch → quit RA → return to
ForgeDash at the same shelf. Modern-dark theme. Zero NAND writes.

**Out (later slices):** multiple shelves (favorites/recent/folders), sync data
contract, theming system + theme packs, settings UI, RESET-button reset-to-menu,
persistent NAND install, metadata scraping. Noted where they attach below.

## Runtime context

- **Bring-up:** the SOLVED memboot recipe — insmod `input-polldev.ko`,
  `mali.ko`, `clovercon.ko` (dp-nes params), `evdev.ko` from
  `hakchi-latest.hmod`; one-shot `clover-mcp` kick; `udevd` + input-only
  `udevadm trigger` to tag the pad `ID_INPUT_JOYSTICK=1`. (Reused as-is; not
  re-designed here. Productizing these manual steps into a RetroForge-driven
  bring-up is its own follow-up, per the controller findings.)
- **Brick safety:** RAM-memboot only, zero NAND writes; power-cycle returns the
  device to bone stock. Honors ADR-0003 (no persistent write without a verified
  backup) by writing nothing persistent.
- **Staging:** tar-over-ssh into tmpfs (reuse the RA-spike mechanism; dropbear
  has no sftp-server). Stage: ForgeDash binary, `library.json`, cover PNGs, one
  ROM, the ClusterM `retroarch-clover` 1.1d tree.

## Architecture & components

Single Rust binary, **ForgeDash**, plus a launcher script. Bounded units:

- **`platform`** — SDL2 fullscreen window @1280×720, EGL/GLES2 context init,
  controller input via SDL gamecontroller (udev pad), the frame loop. Owns all
  device/GL interaction. Depends on: device `libSDL2`, `libMali` (EGL/GLES2).
- **`gfx`** — GLES2 renderer: textured-quad batch, glyph-atlas text, the
  coverflow transform (per-cover scale / x-offset / opacity for the
  center-vs-neighbor effect), modern-dark theme (gradient background + center
  cover glow). Cheap fragment work only. Depends on: `platform` GL context.
- **`model`** — parses `library.json` (serde_json) into the game list; holds
  shelf state (selection index). **Pure, fully unit-testable off-device.**
- **`ui`** — the coverflow shelf widget: L/R moves selection, A launches, eased
  slide animation. Layout/animation math is **pure and unit-tested**; it calls
  `gfx` to draw.
- **`launch`** — on A: write the chosen `{rom_path, core}` to a handoff file
  (e.g. `/tmp/forge/handoff`) and exit with a known status. No direct RA spawn
  from inside the GL app (keeps "one GLES app at a time").
- **`forge-loop.sh`** — the supervisor: `while true: run ForgeDash; if a handoff
  was written, run retroarch <rom> with the proven launch env, clear handoff;
  loop`. This realizes the shell-loop process model.

**Two build targets, one codebase:**
- *Desktop dev build* — same Rust, builds/runs on the Windows dev box (desktop
  SDL2 + GL). For fast iteration on layout, navigation, animation, and theme
  without flashing the device.
- *Device build* — `armv7-unknown-linux-gnueabihf`, dynamically linked against a
  sysroot pulled from the device's own `/usr/lib`, matching RA's NEEDED set. The
  hardware proof.

## Data contract (slice = hand-staged)

`library.json` — an array of game records:

```json
[
  {
    "title": "Super Mario Bros. 3",
    "year": 1988,
    "publisher": "Nintendo",
    "players": "1-2P",
    "cover_path": "art/smb3.png",
    "rom_path": "roms/smb3.nes",
    "core": "fceumm"
  }
]
```

Paths are relative to the staged ForgeDash root. For the slice this file is
hand-placed during staging. **Later:** RetroForge Sync writes `library.json` (or
a SQLite catalog) plus art, ROMs, and recent/favorites state to a known
device path; ForgeDash reads it at boot. The schema here is the seed of that
contract.

## Launch / return flow

1. ForgeDash renders the shelf; user navigates by D-pad.
2. Press **A** → `launch` writes `{rom_path, core}` to the handoff file →
   ForgeDash exits cleanly (releases the GLES surface fully).
3. `forge-loop.sh` sees the handoff → runs `retroarch <rom>` with the proven
   ClusterM launch env (`HOME`, `LD_LIBRARY_PATH=/usr/lib`, udev joypad cfg) →
   game runs.
4. **Quit RA** via Hotkey+Start → "Quit RetroArch" (proven navigable with the
   pad). RA exits.
5. Loop clears the handoff and relaunches ForgeDash at the same shelf state.

*v1 target (deferred):* the console **RESET button** triggers reset-to-menu
(kills RA, returns to ForgeDash) — the stock-style hook, requiring NAND/clovercon
reset handling not present under bare memboot.

## Error handling

- **Manifest missing / unparseable:** render an on-screen error panel (never a
  black screen); log details to `/tmp/forge/forge.log`.
- **No controller detected:** still render the shelf; show a "no controller"
  hint; log. (No keyboard exists on the device — pad is the only input.)
- **RA exits non-zero:** loop returns to ForgeDash and shows a toast; log RA's
  exit code + tail of its log.
- **EGL/GLES init fails:** log the exact EGL error (e.g. `BAD_ALLOC`) and exit
  cleanly — never hang holding the framebuffer (so the failure is diagnosable,
  matching the RA-spike discipline).

## Testing strategy

- **`cargo test` (off-device, CI-able):** `model` manifest parsing & validation;
  `ui` shelf navigation index logic; `gfx`/`ui` coverflow layout math; easing
  functions. All pure.
- **Desktop dev build:** manual iteration of visuals/nav/theme on the dev box.
- **Hardware acceptance (the slice's real proof):**
  1. ForgeDash launches on the device and obtains a Mali GLES2 surface
     @1280×720 with **no `EGL_BAD_ALLOC`**.
  2. Renders ≥3 real covers + title/year/publisher/players in the modern-dark
     theme.
  3. D-pad Left/Right moves the selection with the slide animation; pad seen via
     the udev path (same as the RA spike).
  4. Press A → ForgeDash exits → RA launches that ROM and runs it → quit RA →
     ForgeDash relaunches at the same shelf.
  5. **Zero NAND writes**; power-cycle = bone stock.

  Captured in a `docs/HARDWARE-ACCEPTANCE-FORGEDASH.md` runbook, matching the
  repo's hardware-acceptance pattern.

## Biggest risk

Cross-compiling Rust for `armv7-unknown-linux-gnueabihf` and dynamically linking
the device's `libSDL2` / `libMali` with a matching ABI/version. **Mitigation:**
build against a sysroot pulled from the device's own `/usr/lib`; match RA's
NEEDED library set exactly (RA proves these libs load and reach a Mali surface).
If our binary's loader trace resolves the same libs RA's does, the surface path
is the same one already proven. Confirming this is the central purpose of the
slice.

## Out-of-scope follow-ups (attach points for later slices)

- Multiple shelves (Recently Played / Favorites / per-folder) + up/down nav.
- Sync-written data contract (`library.json`/SQLite) replacing hand-staging.
- Theming system + importable theme packs (superset of the Hakchi theme format).
- Settings surface (per-game core routing, etc.).
- RESET-button reset-to-menu.
- Persistent NAND install (boot straight into ForgeDash), gated by ADR-0003
  mandatory backup + NAND-write tooling.
- Productizing the memboot GPU+pad bring-up into a RetroForge-driven sequence.
