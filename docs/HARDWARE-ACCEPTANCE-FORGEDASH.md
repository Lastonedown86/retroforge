# Hardware Acceptance — ForgeDash Tracer Bullet

> **STATUS: HW-PASSED 2026-06-06** on NES Classic `dp-nes` — every step below
> green, zero NAND writes. Build recipe + toolchain saga + controller codes in
> `docs/superpowers/findings/2026-06-06-forgedash-hw.md`. Build via
> `forgedash/dockerbuild.sh` (Linaro GCC 4.9 / glibc 2.21; zig + modern cross-gcc
> both failed against the device's Buildroot glibc 2.22).

Device: NES Classic (`dp-nes`). All steps RAM-only (brick-safe). Power-cycle =
bone stock. This proves the custom dashboard spine: our own Rust + GLES2 binary
gets a Mali surface, navigates by pad, and launches ROMs into RetroArch.

## Preconditions

1. **GPU + pad bring-up** — memboot the brick-safe recipe from
   `docs/superpowers/findings/2026-06-03-controller-memboot-spike.md`
   (insmod mali/clovercon/evdev, one-shot `clover-mcp` kick, `udevd` +
   input-only `udevadm trigger`). Confirm RetroArch RGUI renders + is
   pad-navigable BEFORE proceeding — that proves the Mali surface + pad path.
2. **RetroArch staged** into `/tmp/ra` (ClusterM `retroarch-clover` 1.1d) per
   `docs/superpowers/findings/2026-06-02-retroarch-rgui-spike.md`, with the
   `/tmp/ra-input.cfg` udev joypad config in place.
3. **Build the device binary** (on a box with the armv7 cross toolchain + a
   `./sysroot` pulled from the device's `/usr/lib`), from the `forgedash/` dir:
   `cargo build -p forgedash --release --target armv7-unknown-linux-gnueabihf`.
4. **Add real content:** put real `.nes` ROMs in `forgedash/app/roms/` and cover
   PNGs in `forgedash/app/art/`, referenced by `forgedash/app/library.json`.
5. **Stage to device:** from `forgedash/`, `sh scripts/stage.sh`
   (binary + library.json + art + roms + forge-loop.sh → `/tmp/forge`).

## Acceptance steps + expected results

- [ ] `sh /tmp/forge/forge-loop.sh` → ForgeDash window appears on HDMI.
      Expect: Mali GLES2 surface @1280×720, **no `EGL_BAD_ALLOC`** in
      `/tmp/forge/forge.log`.
- [ ] Coverflow shows ≥3 real covers; center cover large with a blue glow; title
      + `year · publisher · players` render in the modern-dark theme.
- [ ] D-pad Left/Right slides the selection with the easing animation (pad seen
      via the udev path — same as the RA spike).
- [ ] Press **A** on a game → ForgeDash exits → RetroArch launches that ROM with
      its core and the game runs.
- [ ] In RA: Hotkey+Start → "Quit RetroArch" → `forge-loop.sh` relaunches
      ForgeDash at the same shelf.
- [ ] Power-cycle → device boots bone stock (no NAND writes occurred).

## Result

Record PASS/FAIL per step with log excerpts (`/tmp/forge/forge.log`,
`/tmp/forge/ra.log`). On PASS, the Rust-GLES + pad + launch spine is proven;
later slices add the remaining shelves (Recently Played / Favorites / per-folder),
the RetroForge Sync data contract, the theming system, the RESET-button
reset-to-menu, and the persistent NAND-install boot path (backup-gated, ADR-0003).

## Known deferred risks (verify here first time on hardware)

- **App cross-compile/link** against the device's `libSDL2`/`libMali` — never
  built off-device in CI (no C toolchain there). First real compile happens on
  the cross box; resolve link errors against the device sysroot, not on-device.
- **GLES2 context creation** from SDL with `GLProfile::GLES` v2.0 — RA proves the
  Mali EGL path, but our binary requesting it directly is first proven here.
- **OES_vertex_array_object** — the renderer was written VAO-free (pure GLES2) to
  avoid this dependency; confirm quads render regardless.

## Headless-RA slice acceptance (slice 2) — HW-PASSED 2026-06-06

> PASS on dp-nes: game launches with no visible RGUI, Select+Start returns to
> ForgeDash, Select+A/B save/load + Select+←/→ slot all work with toasts, zero
> NAND writes. **Note:** `menu_driver="null"` segfaults RA 1.7.0 at content
> start — use `menu_driver="rgui"` (it stays unreachable on the NES pad: the
> menu-toggle maps to "Home", which the pad lacks). Fixed in `ra_override_cfg`.

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

## Save-state browser slice acceptance (slice 3)

Build `forgedash/dockerbuild.sh`; push the new `forgedash` binary to `/tmp/forge`
(forge-loop.sh unchanged). ForgeDash now also writes `savestate_thumbnail_enable`
into `ra-override.cfg`, so save states made from this build get `.png` thumbnails.

- [ ] In a game: Select+A save once or twice; Select+Start exit.
- [ ] Coverflow: highlight that game, press D-pad Up -> save-state screen opens
      (title "<game> - Save States", slot list left, preview right).
- [ ] Saves made from this build show a thumbnail; older saves show the placeholder.
- [ ] Up/Down moves the highlight; preview updates to the highlighted slot.
- [ ] A on an existing slot -> game launches and resumes that state.
- [ ] Select on an existing slot -> red confirm bar; A confirms (slot gone on
      re-open), B cancels.
- [ ] B -> back to the coverflow. Power-cycle -> bone stock (zero NAND writes).
