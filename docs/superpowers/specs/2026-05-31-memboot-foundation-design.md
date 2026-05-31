# Slice 2 — Memboot Foundation

**Status:** Approved (2026-05-31)
**Slice:** Second buildable slice. Loads a boot image into the device's RAM over
FEL and executes it, proving the memboot mechanism end to end.

**Depends on:** Slice 1 (FEL transport + version handshake), which is hardware-verified
against a real NES Classic Mini.

## Goal

Build the host-side mechanism to **memboot** a Classic Mini: over FEL, bring up DRAM,
load a boot image and U-Boot into RAM, and execute it so the console boots that image
without writing anything to NAND. The visible end-state: a "Memboot (test)" action in the
app that runs the sequence, detects that the device has left FEL mode (programmatic success
signal), while the console visibly boots on the TV (human confirmation).

This slice deliberately stops short of NAND access. It is the prerequisite mechanism that a
later slice (NAND backup) will build on.

## Decisions (locked during brainstorming)

- **Scope:** memboot foundation only. No NAND read/dump, no persistent write/restore, no
  authoring our own boot image.
- **Payload:** bundle Hakchi's proven reference binaries (fes1 DRAM-init blob, U-Boot, and a
  known-good boot.img) as the memboot payload. We build only the Rust FEL load+exec
  mechanism; authoring a system image is slice 6 (per ADR-0002, "inherit a working boot
  first").
- **Success signal:** after exec, the FEL USB device (`1F3A:EFE8`) disappearing within a
  timeout is the programmatic success signal; the user confirms the actual boot visually on
  the TV. Detecting the booted system over USB is out of scope (couples to image internals).
- **Safety:** memboot is RAM-only and non-persistent. There is no NAND write and therefore
  no brick path in this slice. A bad payload merely hangs the device; recovery is a
  power-cycle back into FEL.

## Background (from the Hakchi FelLib reference)

The memboot procedure and memory map are taken from `../Hakchi2-CE`
(`Libraries/FelLib/FelLib/Fel.cs`) and mirrored, not copied:

- `fes1_base_m = 0x2000` (SRAM) — where the DRAM-init blob is loaded and executed.
- `dram_base = 0x40000000`.
- `uboot_base_m = dram_base + 0x7000000`.
- `transfer_base_m = dram_base + 0x7400000` — where the boot image is staged.
- `WriteMemory(addr)`: if `addr >= dram_base`, run `InitDram()` first.
- `InitDram()`: write fes1 to `0x2000`, `Exec(0x2000)`.
- `Exec(addr)`: `FEL_RUN` request at `addr`.
- `RunUbootCmd(cmd)`: load U-Boot to `uboot_base_m`, patch the command string at a marker
  offset inside the U-Boot binary, `Exec(uboot_base_m)`.
- Memboot: init DRAM, stage the boot image, run the U-Boot `boota` command pointing at it.

FEL memory ops extend slice 1's transport: `FEL_DOWNLOAD`/`FEL_UPLOAD` (an `AWFELMessage`
carrying request + address + length, then the bulk payload + status), and `FEL_RUN`.

## Architecture

A refactor precedes the new work. Slice 1 left the FEL transport (open/claim, looped bulk
read/write, AWUSB envelope) inside `handshake()` as local closures. Memboot must reuse it,
so it is extracted into reusable units. The refactor preserves slice-1 behavior exactly and
is re-verified on the live device.

```
src-tauri/src/device/
  fel.rs        PURE protocol. AWUSB envelope encoder (aw_usb_request) and FEL request
                encoder (fel_request) moved here from usb.rs. parse_version, soc_name,
                constants. No USB; fully unit-tested.
  usb.rs        FelTransport: an open + interface-claimed rusb handle plus the IN/OUT bulk
                endpoints. read_exact (loops until the buffer is filled), fel_write and
                fel_read (AWUSB envelope + payload + trailing status drain), verify_device
                (the slice-1 handshake, now a method). UsbProbe is built on FelTransport.
  memboot.rs    NEW. Orchestration over a FelOps trait. write_memory / read_memory / exec,
                init_dram, run_uboot_cmd, and memboot(). The "init DRAM before any DRAM-range
                write" rule lives here.
  blobs.rs      NEW. Bundled reference binaries embedded with include_bytes!: fes1, U-Boot,
                boot.img. Accessors return &'static [u8] and surface BlobMissing if a blob is
                absent at build time.
  mod.rs        Re-exports; unchanged status model.
error.rs        New RfError variants: MemoryWriteFailed, ExecFailed, BlobMissing,
                MembootTimeout.
lib.rs          New memboot() command + memboot-progress event; worker thread.
```

## Components and boundaries

### `fel.rs` (pure protocol)
Owns the byte layouts: `aw_usb_request(req, len) -> [u8; 32]`, `fel_request(cmd, addr, len)
-> [u8; 16]`, response parsing, and constants (request types `FEL_VERIFY_DEVICE`,
`FEL_DOWNLOAD`, `FEL_UPLOAD`, `FEL_RUN`; the memory-map addresses). No I/O — testable in
isolation.

### `usb.rs` — `FelTransport`
Wraps one open, interface-claimed device handle and the discovered bulk endpoints. Provides
the looped `read_exact`, and the two FEL primitives `fel_write(payload)` (envelope + payload
+ 13-byte status drain) and `fel_read(len) -> Vec<u8>` (envelope + looped read + status
drain). `verify_device()` performs the version handshake. This is the single place that
touches rusb for FEL exchanges.

### `memboot.rs` — `FelOps` + orchestration
`FelOps` trait: `write_memory(addr, &[u8])`, `read_memory(addr, len) -> Vec<u8>`,
`exec(addr)`. The real implementation delegates to `FelTransport` (using `FEL_DOWNLOAD` /
`FEL_UPLOAD` / `FEL_RUN`). A **mock** implementation records the calls for unit tests.

Orchestration functions are written against `FelOps` so they are testable without hardware:
- `init_dram(ops, fes1)` — write fes1 to `0x2000`, `exec(0x2000)`.
- `run_uboot_cmd(ops, uboot, cmd)` — patch the command string into the U-Boot image at its
  marker offset, write U-Boot to `uboot_base_m`, `exec(uboot_base_m)`.
- `memboot(ops, fes1, uboot, boot_img)` — call `init_dram` **once** explicitly, then write
  `boot_img` to `transfer_base_m`, then `run_uboot_cmd` with the `boota` command pointing at
  `transfer_base_m`. The orchestration owns DRAM-init ordering; `write_memory` does **not**
  auto-init DRAM (unlike the reference's `WriteMemory`), so init happens exactly once.

### `lib.rs` — command + event
`memboot()` Tauri command spawns a worker thread (the sequence is multi-step and slow),
emits `memboot-progress` events (one per step: dram-init, load-image, load-uboot, exec,
waiting-for-fel-exit), then polls for the FEL device to disappear within a timeout. Emits a
terminal success or failure.

### UI
A "Memboot (test)" button and a step/status readout placed near the existing status panel.
Disabled unless the panel shows Connected. (The proper Advanced surface is a later slice.)

## Data flow

User clicks Memboot → command spawns worker → open `FelTransport` → `init_dram` → write
`boot.img` to transfer base → write + patch U-Boot → `exec` boota → poll for `1F3A:EFE8` to
vanish within the timeout → emit `Success` (programmatic) → user confirms the console booted
on the TV. Each step emits a progress event so the UI shows where it is.

## Error handling and safety

Memboot performs **no NAND write** — it is RAM-only and non-persistent, so there is no brick
path in this slice. Every FEL step returns `Result`. New `RfError` variants:
`MemoryWriteFailed`, `ExecFailed`, `BlobMissing`, `MembootTimeout`. A bad or mismatched blob
hangs the device: the FEL device never disappears, the poll times out, and the UI reports
failure with recovery guidance ("unplug and replug while holding RESET to return to FEL").
No panics cross the Tauri boundary.

## Testing

- **Unit (no hardware):**
  - `fel.rs` encoders: `aw_usb_request` and `fel_request` produce the exact expected byte
    layouts (table-tested), including the `FEL_DOWNLOAD`/`FEL_RUN` request encodings.
  - `memboot.rs` orchestration via a mock `FelOps`: assert the emitted call sequence —
    DRAM init happens before any DRAM-range write, addresses match the memory map, the
    U-Boot command is patched at the correct offset, and `boota`/exec is issued last.
- **Hardware acceptance (manual, the real gate):** a documented checklist — run Memboot
  against the device, confirm the FEL device disappears within the timeout AND the console
  visibly boots the loaded image on the TV. Captures any address/blob corrections from the
  real run.

## Out of scope (later slices)

- NAND read/dump (slice 3), persistent NAND write/restore.
- Authoring our own boot image / system image (slice 6).
- Detecting the booted system over USB after memboot.
- The Advanced-surface UI proper; this slice uses a single test button.

## Open items (resolved during plan/implementation)

- **Blob sourcing.** `fes1.bin` is present at `Hakchi2-CE/hakchi_gui/data/fes1.bin`. The
  **U-Boot binary** and a **boot.img** are not in the reference repo by name (Hakchi fetches
  or builds them) — they must be located in the reference or sourced as known-good binaries
  before the hardware test. This gates the hardware run, not the code, which is testable
  against the mock `FelOps`.
- **U-Boot command marker.** `run_uboot_cmd` depends on a known marker offset inside the
  specific Hakchi U-Boot build where the command string is patched. The exact marker/format
  must be read from the reference and confirmed against the sourced U-Boot binary.
- **boota argument form.** The exact `boota`/boot command string and address argument are
  confirmed from the reference during implementation.
