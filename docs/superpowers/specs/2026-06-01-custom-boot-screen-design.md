# Custom Boot Screen — Design (Sub-project 1)

**Date:** 2026-06-01
**Status:** design
**Parent initiative:** Custom boot screen + customizable dashboard (deployed via RetroForge).
**This spec covers Sub-project 1 only** (boot screen). The dashboard (port
EmulationStation-DE/Pegasus in place of `m2engage`) is a separate later cycle.

## Goal & boundary

Let a user pick a PNG and see it as the device's boot logo, by repacking the
cached hakchi `boot.img` with that PNG swapped into its ramdisk overlay and
**RAM-membooting** the repacked image via the proven FEL path. Non-persistent
(a normal power-on still shows the stock logo); zero NAND writes; zero brick
risk.

This is the **tracer bullet** for the whole initiative: it proves the
modify → deploy → observe loop end-to-end with the smallest possible change
(one image asset) before the much larger dashboard work.

### In scope (v1)
- Swap the boot **PNG** only.
- Repack entirely in RetroForge's Rust; memboot the result through the existing
  pipeline.
- CI-safe unit tests for the (un)pack logic; manual on-device acceptance for the
  actual displayed logo.

### Out of scope (v1)
- Boot sound (`boot.wav`), persistence/NAND flash, the dashboard, animated
  splashes, multiple images, theming config.

## How the boot logo works (verified from the hakchi ramdisk)

- The boot logo is shown by `etc/preinit.d/p7010_bootlogo`:
  `showImage "$cfg_boot_logo" || showImage "$rootfs/etc/boot.png" || showImage "$rootfs/etc/$modname.png"`
  then `playSound "$rootfs/etc/boot.wav"`. `showImage` pipes a PNG through
  `bin/decodepng` to the framebuffer.
- These files live **inside the `boot.img` ramdisk overlay**
  (`hakchi/rootfs/etc/...`), not on NAND. So a repacked `boot.img` carrying a
  different `boot.png` shows a different logo when membooted — no persistence
  needed.
- The cached `boot.img` ramdisk is **XZ-compressed newc cpio** (magic
  `fd 37 7a 58 5a 00`), decompressing to the cpio that contains
  `hakchi/rootfs/etc/boot.png` and the hakchi overlay.

## Architecture

Pipeline: `user PNG → validate → load cached boot.img → split → xz-decompress
ramdisk → cpio replace boot.png → xz-recompress → reassemble boot.img → memboot`.

### New modules (focused units)

1. **`device/bootimage.rs`** — Android boot image split/reassemble.
   - `split(img: &[u8]) -> BootImage { header, kernel, ramdisk, second, … }`
     parsing the v0 header (magic `ANDROID!`, kernel/ramdisk sizes + addrs,
     `page_size` at offset 36, cmdline at 64).
   - `reassemble(parts) -> Vec<u8>` recomputing `ramdisk_size` and page-padding
     each region. **The `id[8]` SHA is left stale** — hakchi's `boota` does not
     verify it (proven: the prior `inject_cmdline` path shipped a stale id and
     booted). Documented in code.
   - Replaces the role of the deleted `bootimg.rs`, but full pack rather than
     just a cmdline edit.

2. **`device/ramdisk.rs`** — XZ + newc-cpio (un)pack with replace-entry.
   - `decompress_xz(&[u8]) -> Vec<u8>` / `compress_xz(&[u8]) -> Vec<u8>` using
     the same XZ container the kernel expects (CRC32 check, no BCJ filter, no
     stream concatenation). Crate: `xz2` (liblzma) — confirm the kernel's XZ
     ramdisk decompressor accepts our stream during HW acceptance.
   - `cpio_replace(cpio: &[u8], path: &str, new_bytes: &[u8]) -> Vec<u8>` —
     parse newc entries, replace the named file's data (updating its `filesize`
     + 4-byte padding), re-emit byte-for-byte otherwise. Newc parsing mirrors
     the reference Python already used to read this ramdisk.

3. **`device/bootscreen.rs`** — orchestration.
   - `validate_png(png: &[u8], expect: Dims) -> Result<()>` — parse the PNG IHDR
     (width/height/bit-depth/colour-type/interlace); require dimensions matching
     the stock `boot.png` and a `decodepng`-friendly format (8-bit, non-
     interlaced, RGB/RGBA). Reject otherwise with a clear error.
   - `build_custom_bootimg(stock_img: &[u8], png: &[u8]) -> Result<Vec<u8>>` —
     ties bootimage + ramdisk together; reads the stock `boot.png` dims to feed
     `validate_png`, swaps it, returns the repacked image.

### RetroForge integration

- Refactor `run_memboot` to take an **image source** (stock cached blob *or* an
  in-memory custom image) so both the existing memboot and the boot-screen flow
  share one staging/boota path. No change to FEL primitives.
- New Tauri command `memboot_bootscreen(png_path: String)`: load the user PNG,
  `build_custom_bootimg`, memboot it; reuse the `MembootProgress` events.
- Frontend: a small control to pick a PNG file and trigger it, reusing the
  memboot progress UI.

## Data flow & errors

| Step | Failure | Error |
|---|---|---|
| Read user PNG | unreadable | `BlobMissing` / IO |
| Validate PNG | wrong dims/format | new `BootScreenInvalidPng(String)` |
| Split boot.img | malformed | `ExecFailed` |
| xz decompress | bad stream | new `RamdiskError(String)` |
| cpio replace | `boot.png` entry absent | `RamdiskError` |
| memboot | FEL/transfer | existing `MembootTimeout` etc. |

## Testing

### CI-safe (no hardware)
- `bootimage`: split→reassemble round-trips a synthetic image byte-identically
  when nothing changes; `ramdisk_size`/padding update correctly when the ramdisk
  grows/shrinks.
- `ramdisk`: `cpio_replace` on a hand-built newc archive swaps the target
  entry, leaves siblings byte-identical, fixes filesize+padding; xz
  decompress∘compress round-trips.
- `bootscreen`: `validate_png` accepts a correct-dims 8-bit PNG, rejects wrong
  dims / interlaced / wrong colour type (IHDR parsing).
- A real-asset ignored test (like the old `inject_real_boot_img`): on the cached
  hmod, swap `boot.png` with a generated same-dims PNG and assert the rebuilt
  image re-splits + the ramdisk re-extracts with the new bytes.

### Hardware acceptance (manual)
- Memboot the custom image; confirm the chosen PNG displays on the device HDMI,
  then boot continues to the stock menu. (We already have a working
  memboot + the device on the bench.)

## Risks

- **Kernel XZ pickiness:** the in-kernel/preinit XZ decompressor may reject a
  stream built with different options. Mitigate by matching the original
  container (CRC32, single stream, no filter); verify on HW. Fallback: if the
  kernel ramdisk is actually consumed as-is by U-Boot/preinit rather than the
  kernel, the constraint relaxes — confirm during acceptance.
- **Stale id-SHA:** assumed ignored by `boota` (proven historically). If a
  future image verifies it, add SHA-1 recompute to `reassemble`.
- **PNG format vs `decodepng`:** unknown exact constraints; validate
  conservatively (8-bit, non-interlaced) and refine after the first on-device
  test.

## YAGNI notes
- No persistence, no wav, no dashboard, no multi-asset theming in v1. Each is a
  later, separate increment once the loop is proven.
