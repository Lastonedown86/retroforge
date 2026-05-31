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
- **Payload sourcing:** the memboot payload is three blobs — `fes1` (DRAM init), `uboot.bin`,
  and `boot.img`.
  - `fes1.bin` is **bundled** in our repo (an Allwinner DRAM-init blob, ~14 KB; sourced from
    `Hakchi2-CE/hakchi_gui/data/fes1.bin`).
  - `uboot.bin` and `boot.img` are **fetched at runtime**: download Hakchi's public
    `hakchi-latest.hmod` (a tar archive) from `https://hakchi.net/hakchi/hmods/hakchi-latest.hmod`,
    extract `boot/uboot.bin` and `boot/boot.img`, and cache them locally (gitignored).
  - Rationale: `boot.img` contains Nintendo's stock R16 kernel; committing it to our public
    GPLv3 repo is a copyright problem, and `uboot.bin` is a GPLv2 binary we'd rather not
    redistribute either. Runtime-fetch keeps both out of our source and mirrors exactly how
    Hakchi obtains them. We build only the Rust FEL load+exec mechanism; authoring a system
    image is slice 6 (per ADR-0002, "inherit a working boot first").
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
  blobs.rs      NEW. fes1 embedded with include_bytes! (returns &'static [u8]). Also the
                accessor layer that resolves uboot.bin + boot.img from the local hmod cache
                (see hmod.rs), surfacing BlobMissing if they are not yet available.
  hmod.rs       NEW. Fetches hakchi-latest.hmod (download + cache to a gitignored app-data
                path), extracts boot/uboot.bin and boot/boot.img from the tar, and exposes
                them as byte buffers. Re-download only if the cache is absent.
  mod.rs        Re-exports; unchanged status model.
error.rs        New RfError variants: MemoryWriteFailed, ExecFailed, BlobMissing,
                MembootTimeout, HmodFetchFailed, HmodExtractFailed.
lib.rs          New memboot() command + memboot-progress event; worker thread. An
                ensure_blobs step fetches/caches the hmod before the FEL sequence.
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

### `blobs.rs` + `hmod.rs` — payload provisioning
`hmod.rs` owns acquiring the runtime blobs: download `hakchi-latest.hmod` to a gitignored
app-data cache path (only if not already cached), open the archive, and extract
`boot/uboot.bin` and `boot/boot.img`. `blobs.rs` is the unified accessor: `fes1()` returns
the embedded blob; `uboot()` and `boot_img()` return the cached extractions, or
`BlobMissing` if `ensure_blobs` has not run. Archive handling uses a Rust tar/compression
crate; the extraction (locate an entry by path, read its bytes) is unit-testable against a
small synthetic archive fixture.

### `lib.rs` — command + event
`memboot()` Tauri command spawns a worker thread (the sequence is multi-step and slow). It
first runs `ensure_blobs` (fetch + cache the hmod, extract uboot/boot.img) — emitting a
`fetching-payload` progress step — then emits one event per FEL step (dram-init, load-image,
load-uboot, exec, waiting-for-fel-exit), then polls for the FEL device to disappear within a
timeout. Emits a terminal success or failure.

### UI
A "Memboot (test)" button and a step/status readout placed near the existing status panel.
Disabled unless the panel shows Connected. (The proper Advanced surface is a later slice.)

## Data flow

User clicks Memboot → command spawns worker → `ensure_blobs` (download + cache
hakchi-latest.hmod, extract uboot.bin + boot.img; skipped if already cached) → open
`FelTransport` → `init_dram` → write `boot.img` to transfer base → write + patch U-Boot →
`exec` boota → poll for `1F3A:EFE8` to vanish within the timeout → emit `Success`
(programmatic) → user confirms the console booted on the TV. Each step emits a progress
event so the UI shows where it is.

## Error handling and safety

Memboot performs **no NAND write** — it is RAM-only and non-persistent, so there is no brick
path in this slice. Every FEL step returns `Result`. New `RfError` variants:
`MemoryWriteFailed`, `ExecFailed`, `BlobMissing`, `MembootTimeout`, `HmodFetchFailed`,
`HmodExtractFailed`. The payload fetch can fail (offline, server down, corrupt archive) —
that surfaces as `HmodFetchFailed`/`HmodExtractFailed` with a clear message before any FEL
action, leaving the device untouched. A bad or mismatched blob hangs the device: the FEL
device never disappears, the poll times out, and the UI reports failure with recovery
guidance ("unplug and replug while holding RESET to return to FEL"). No panics cross the
Tauri boundary.

## Testing

- **Unit (no hardware):**
  - `fel.rs` encoders: `aw_usb_request` and `fel_request` produce the exact expected byte
    layouts (table-tested), including the `FEL_DOWNLOAD`/`FEL_RUN` request encodings.
  - `memboot.rs` orchestration via a mock `FelOps`: assert the emitted call sequence —
    DRAM init happens before any DRAM-range write, addresses match the memory map, the
    U-Boot command is patched at the correct offset, and `boota`/exec is issued last.
  - `hmod.rs` extraction: against a small synthetic tar fixture containing `boot/uboot.bin`
    and `boot/boot.img`, assert the correct entry bytes are returned and a missing entry
    yields `HmodExtractFailed`. (The network download itself is not unit-tested.)
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

- **Blob sourcing — RESOLVED in approach, details in plan.** `fes1.bin` is bundled (copied
  from `Hakchi2-CE/hakchi_gui/data/fes1.bin`). `uboot.bin` + `boot.img` are fetched at
  runtime from `https://hakchi.net/hakchi/hmods/hakchi-latest.hmod` and extracted from the
  tar (entries `boot/uboot.bin`, `boot/boot.img`). The plan must pin the exact archive
  compression (the hmod is opened generically by Hakchi via SharpCompress, then read as a
  tar — confirm whether it is gzip/xz/plain tar and pick the matching Rust crate).
- **U-Boot command marker.** `run_uboot_cmd` depends on a known marker offset inside the
  Hakchi U-Boot build where the command string is patched (Hakchi finds it by scanning for a
  prefix; the SD variant also swaps the last 8 bytes — see `hakchi.cs` `Uboot()`). The exact
  marker/prefix and the SD-vs-normal handling must be read from the reference and confirmed
  against the extracted `uboot.bin`. For this slice, target the "normal" (non-SD) U-Boot.
- **boota argument form.** The exact `boota`/boot command string and address argument are
  confirmed from the reference (`Fel.cs` memboot path) during implementation.
