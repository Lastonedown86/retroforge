# Memboot Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load a boot image into the Classic Mini's RAM over FEL and execute it (memboot), proving the load+exec mechanism end to end without touching NAND.

**Architecture:** Refactor slice-1's buried FEL transport into a reusable `FelTransport` plus a pure `fel.rs` encoder layer, then build memboot on a small `FelIo` trait so the memory ops and orchestration are unit-testable with a mock; the real boot blobs (`uboot.bin`, `boot.img`) are fetched at runtime from Hakchi's public `hakchi-latest.hmod` (only `fes1.bin` is bundled).

**Tech Stack:** Rust (rusb, thiserror, ureq, flate2, tar), Tauri v2, React/TS.

**Reference:** All FEL constants/sequences mirror `../Hakchi2-CE/Libraries/FelLib/FelLib/Fel.cs` and `hakchi_gui/hakchi.cs` / `Tasks/MembootTasks.cs`. This is a clean-room reimplementation in Rust; no Hakchi code is copied.

---

## Safety note

Memboot is **RAM-only and non-persistent**. No NAND write occurs in this slice, so there is no brick path. A bad/mismatched blob merely hangs the device; recovery is unplug → hold RESET → replug back into FEL.

## File Structure

| Path | Responsibility |
|------|----------------|
| `src-tauri/src/device/fel.rs` | PURE protocol: AWUSB/FEL encoders (moved here), constants, memory map, parse_version. |
| `src-tauri/src/device/usb.rs` | `FelTransport` (open/claim, looped read/write, fel_write/fel_read, verify_device); `UsbProbe`. |
| `src-tauri/src/device/memboot.rs` | NEW. `FelIo` trait, write_memory/read_memory/exec, init_dram/run_uboot_cmd/memboot, mock + tests. |
| `src-tauri/src/device/hmod.rs` | NEW. Download+cache hakchi-latest.hmod; extract entries from tar(.gz). |
| `src-tauri/src/device/blobs.rs` | NEW. Bundled fes1 (include_bytes) + cached uboot/boot.img accessors; ensure_blobs. |
| `src-tauri/resources/fes1.bin` | NEW. Bundled DRAM-init blob (copied from Hakchi). |
| `src-tauri/src/error.rs` | New RfError variants. |
| `src-tauri/src/lib.rs` | `memboot()` command + `memboot-progress` event + worker thread. |
| `src/lib/memboot.ts`, `src/components/MembootButton.tsx` | Frontend trigger + progress UI. |

---

## Task 1: Move FEL encoders into `fel.rs` + add memboot constants

Pure-protocol consolidation. The AWUSB/FEL request encoders currently live in `usb.rs`; move them to `fel.rs` (where the other protocol code is) and add the memory-op constants. No behavior change.

**Files:**
- Modify: `src-tauri/src/device/fel.rs`, `src-tauri/src/device/usb.rs`

- [ ] **Step 1: Add constants + encoders to `fel.rs`**

Append to `src-tauri/src/device/fel.rs` (after the existing constants, before `soc_name`):

```rust
// FEL request types (AWFELStandardRequest).
pub const FEL_DOWNLOAD: u32 = 0x101; // write data to device memory
pub const FEL_RUN: u32 = 0x102; // execute code at address
pub const FEL_UPLOAD: u32 = 0x103; // read data from device memory

// Memboot memory map (Allwinner R16 / Hakchi reference).
pub const FES1_BASE: u32 = 0x2000; // SRAM: DRAM-init blob load+exec address
pub const DRAM_BASE: u32 = 0x4000_0000;
pub const UBOOT_BASE: u32 = DRAM_BASE + 0x0700_0000; // 0x4700_0000
pub const TRANSFER_BASE: u32 = DRAM_BASE + 0x0740_0000; // 0x4740_0000
pub const SECTOR_SIZE: usize = 0x2_0000;
pub const TRANSFER_MAX_SIZE: u32 = (SECTOR_SIZE as u32) * 0x100; // 0x200_0000
pub const MAX_BULK: usize = 0x1_0000; // per-transfer chunk size

/// Build the 32-byte AWUC USB request envelope.
pub fn aw_usb_request(req: u16, len: u32) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[0..4].copy_from_slice(b"AWUC");
    b[8..12].copy_from_slice(&len.to_le_bytes());
    b[12..16].copy_from_slice(&0x0c00_0000u32.to_le_bytes());
    b[16..18].copy_from_slice(&req.to_le_bytes());
    b[18..22].copy_from_slice(&len.to_le_bytes());
    b
}

/// Build the 16-byte FEL request (AWFELMessage): cmd (u16) + tag(0) + address + length + flags(0).
pub fn fel_request(request: u32, address: u32, length: u32) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&request.to_le_bytes());
    b[4..8].copy_from_slice(&address.to_le_bytes());
    b[8..12].copy_from_slice(&length.to_le_bytes());
    b
}

#[cfg(test)]
mod request_tests {
    use super::*;

    #[test]
    fn fel_request_encodes_download_addr_len() {
        let m = fel_request(FEL_DOWNLOAD, 0x4740_0000, 0x100);
        assert_eq!(&m[0..4], &[0x01, 0x01, 0x00, 0x00]); // cmd 0x101, tag 0
        assert_eq!(&m[4..8], &0x4740_0000u32.to_le_bytes()); // address
        assert_eq!(&m[8..12], &0x100u32.to_le_bytes()); // length
        assert_eq!(&m[12..16], &[0, 0, 0, 0]); // flags 0
    }

    #[test]
    fn fel_request_run_has_zero_len() {
        let m = fel_request(FEL_RUN, 0x4700_0000, 0);
        assert_eq!(&m[0..4], &[0x02, 0x01, 0x00, 0x00]); // cmd 0x102
        assert_eq!(&m[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn aw_usb_request_read_envelope() {
        let e = aw_usb_request(AW_USB_READ, 32);
        assert_eq!(&e[0..4], b"AWUC");
        assert_eq!(&e[8..12], &32u32.to_le_bytes());
        assert_eq!(&e[16..18], &AW_USB_READ.to_le_bytes());
    }
}
```

- [ ] **Step 2: Remove the duplicate encoders from `usb.rs` and import from `fel`**

In `src-tauri/src/device/usb.rs`, delete the local `aw_usb_request` and `fel_request` functions. Update the `fel` import so the handshake uses `fel::aw_usb_request` / `fel::fel_request`. Concretely, change the existing calls `aw_usb_request(...)` → `fel::aw_usb_request(...)` and `fel_request(...)` → `fel::fel_request(...)` inside `handshake`, and ensure the `use crate::device::fel::{...}` line still brings in `AW_FEL_VERSION, AW_USB_READ, AW_USB_WRITE` (it can keep `self as fel` too). Remove now-unused imports if clippy flags them.

- [ ] **Step 3: Verify**

Run: `cd src-tauri && cargo test device::fel`
Expected: existing fel tests + the 3 new `request_tests` pass.

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/device/fel.rs src-tauri/src/device/usb.rs
git commit -m "refactor(core): move FEL encoders to fel.rs, add memboot constants"
```

---

## Task 2: Extract `FelTransport` from the handshake

Turn the open/claim + looped bulk I/O + `fel_write`/`fel_read` (currently closures inside `handshake`) into a reusable `FelTransport`. `UsbProbe` then uses `FelTransport::open` + `verify_device`. Behavior is identical to slice 1 (re-verified on hardware later).

**Files:**
- Modify: `src-tauri/src/device/usb.rs`

- [ ] **Step 1: Add `FelTransport`**

In `src-tauri/src/device/usb.rs`, add (keeping `FEL_VID`/`FEL_PID`/`is_fel_device`/`bulk_endpoints`):

```rust
use crate::device::fel::{self, AW_FEL_VERSION, AW_USB_READ, AW_USB_WRITE};
use crate::device::SocInfo;

/// An open, interface-claimed FEL device plus its bulk endpoints. All FEL
/// exchanges go through here. rusb DeviceHandle methods take &self, so this is
/// shared-borrow friendly.
pub struct FelTransport {
    handle: rusb::DeviceHandle<rusb::Context>,
    ep_in: u8,
    ep_out: u8,
}

impl FelTransport {
    /// Open and claim interface 0 of a FEL device. Claim failure (e.g. WinUSB
    /// not bound) is an error the caller maps to DetectedNoDriver.
    pub fn open(device: &rusb::Device<rusb::Context>) -> Result<Self, RfError> {
        let handle = device
            .open()
            .map_err(|e| RfError::UsbClaimFailed(e.to_string()))?;
        let _ = handle.set_auto_detach_kernel_driver(true); // expected to fail on Windows
        handle
            .claim_interface(0)
            .map_err(|e| RfError::UsbClaimFailed(e.to_string()))?;
        let (ep_in, ep_out) = bulk_endpoints(device)?;
        Ok(Self {
            handle,
            ep_in,
            ep_out,
        })
    }

    fn write_all(&self, data: &[u8]) -> Result<(), RfError> {
        let mut pos = 0;
        while pos < data.len() {
            let n = self
                .handle
                .write_bulk(self.ep_out, &data[pos..], TIMEOUT)
                .map_err(|e| RfError::FelProtocolError(e.to_string()))?;
            if n == 0 {
                return Err(RfError::FelProtocolError("zero-length bulk write".into()));
            }
            pos += n;
        }
        Ok(())
    }

    fn read_exact(&self, buf: &mut [u8]) -> Result<(), RfError> {
        let mut pos = 0;
        while pos < buf.len() {
            let n = self
                .handle
                .read_bulk(self.ep_in, &mut buf[pos..], TIMEOUT)
                .map_err(|e| RfError::FelProtocolError(e.to_string()))?;
            if n == 0 {
                return Err(RfError::FelProtocolError(format!(
                    "zero-length bulk read at {pos}/{}",
                    buf.len()
                )));
            }
            pos += n;
        }
        Ok(())
    }

    /// Send a FEL payload: AWUC WRITE envelope, the payload, then drain the
    /// 13-byte AWUS status.
    pub fn fel_write(&self, payload: &[u8]) -> Result<(), RfError> {
        self.write_all(&fel::aw_usb_request(AW_USB_WRITE, payload.len() as u32))?;
        self.write_all(payload)?;
        let mut status = [0u8; 13];
        self.read_exact(&mut status)?;
        Ok(())
    }

    /// Read `len` bytes from FEL: AWUC READ envelope, the payload, then drain
    /// the 13-byte AWUS status.
    pub fn fel_read(&self, len: usize) -> Result<Vec<u8>, RfError> {
        self.write_all(&fel::aw_usb_request(AW_USB_READ, len as u32))?;
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        let mut status = [0u8; 13];
        self.read_exact(&mut status)?;
        Ok(buf)
    }

    /// FEL version handshake → SoC identity (slice-1 behavior).
    pub fn verify_device(&self) -> Result<SocInfo, RfError> {
        self.fel_write(&fel::fel_request(AW_FEL_VERSION, 0, 0))?;
        let version = self.fel_read(32)?;
        let _fel_status = self.fel_read(8)?;
        fel::parse_version(&version)
    }
}
```

- [ ] **Step 2: Replace the old `handshake` free function**

Delete the old `handshake(device)` free function in `usb.rs`. In `probe_once`, replace `match handshake(&device) { ... }` with:

```rust
        return match FelTransport::open(&device).and_then(|t| t.verify_device()) {
            Ok(soc) => {
                *cache.borrow_mut() = Some(soc.clone());
                Ok(ProbeOutcome::Connected(soc))
            }
            Err(e) => {
                eprintln!("FEL handshake failed: {e}");
                Ok(ProbeOutcome::DetectedNoDriver)
            }
        };
```

Keep `bulk_endpoints` (now used by `FelTransport::open`). Remove imports left unused.

- [ ] **Step 3: Verify (no behavior change)**

Run: `cd src-tauri && cargo test` → all existing tests pass (slice-1 + Task 1 = 9 tests).
Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings` → clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/device/usb.rs
git commit -m "refactor(core): extract FelTransport for reuse by memboot"
```

> **Hardware re-verify (manual, after Task 2):** run `npm run tauri dev`, confirm the panel still reaches steady green "Connected — Allwinner R16" against the device (the refactor must not regress slice 1). This is a checkpoint, not a code step.

---

## Task 3: Add memboot error variants

**Files:**
- Modify: `src-tauri/src/error.rs`

- [ ] **Step 1: Add variants**

In `src-tauri/src/error.rs`, add to the `RfError` enum (after the existing variants):

```rust
    #[error("memory write failed: {0}")]
    MemoryWriteFailed(String),
    #[error("memory read failed: {0}")]
    MemoryReadFailed(String),
    #[error("FEL exec failed: {0}")]
    ExecFailed(String),
    #[error("boot blob unavailable: {0}")]
    BlobMissing(String),
    #[error("memboot timed out waiting for device to leave FEL")]
    MembootTimeout,
    #[error("failed to fetch hakchi hmod: {0}")]
    HmodFetchFailed(String),
    #[error("failed to extract from hmod: {0}")]
    HmodExtractFailed(String),
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo test error` → existing error test passes; new variants compile (dead-code allowed until used in later tasks — add `#[allow(dead_code)]` per-variant only if a later task in this plan does not consume it; all of these ARE consumed by Tasks 4–9, so no allow needed once those land. For THIS commit, a dead-code warning is acceptable under `cargo test`; do not run clippy -D yet).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/error.rs
git commit -m "feat(core): add memboot/hmod error variants"
```

---

## Task 4: `FelIo` trait + memory primitives (write/read/exec)

**Files:**
- Create: `src-tauri/src/device/memboot.rs`
- Modify: `src-tauri/src/device/mod.rs` (add `pub mod memboot;`)

- [ ] **Step 1: Write `memboot.rs` with the trait, primitives, mock, and failing tests**

Create `src-tauri/src/device/memboot.rs`:

```rust
use crate::device::fel;
use crate::error::RfError;

/// Low-level FEL byte channel. `FelTransport` implements this against real USB;
/// tests use a recording mock. Everything above this trait is hardware-free.
pub trait FelIo {
    fn fel_write(&mut self, payload: &[u8]) -> Result<(), RfError>;
    fn fel_read(&mut self, len: usize) -> Result<Vec<u8>, RfError>;
}

fn status_ok(status: &[u8]) -> bool {
    status.get(4).copied() == Some(0)
}

/// Write `data` to device memory starting at `addr`, chunked at MAX_BULK,
/// padded up to a 4-byte boundary. Mirrors the reference WriteMemory.
pub fn write_memory<T: FelIo>(io: &mut T, addr: u32, data: &[u8]) -> Result<(), RfError> {
    let mut buf = data.to_vec();
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
    let mut pos = 0usize;
    while pos < buf.len() {
        let chunk_len = (buf.len() - pos).min(fel::MAX_BULK);
        io.fel_write(&fel::fel_request(
            fel::FEL_DOWNLOAD,
            addr + pos as u32,
            chunk_len as u32,
        ))?;
        io.fel_write(&buf[pos..pos + chunk_len])?;
        let status = io.fel_read(8)?;
        if !status_ok(&status) {
            return Err(RfError::MemoryWriteFailed(format!(
                "bad status at {:#x}",
                addr + pos as u32
            )));
        }
        pos += chunk_len;
    }
    Ok(())
}

/// Read `len` bytes from device memory at `addr` (4-byte padded), chunked.
pub fn read_memory<T: FelIo>(io: &mut T, addr: u32, len: u32) -> Result<Vec<u8>, RfError> {
    let len = (len + 3) & !3;
    let mut out = Vec::with_capacity(len as usize);
    let mut addr = addr;
    let mut remaining = len;
    while remaining > 0 {
        let l = remaining.min(fel::MAX_BULK as u32);
        io.fel_write(&fel::fel_request(fel::FEL_UPLOAD, addr, l))?;
        out.extend_from_slice(&io.fel_read(l as usize)?);
        let status = io.fel_read(8)?;
        if !status_ok(&status) {
            return Err(RfError::MemoryReadFailed(format!("bad status at {addr:#x}")));
        }
        remaining -= l;
        addr += l;
    }
    Ok(out)
}

/// Execute code at `addr` (FEL_RUN). No status follows; the device runs.
pub fn exec<T: FelIo>(io: &mut T, addr: u32) -> Result<(), RfError> {
    io.fel_write(&fel::fel_request(fel::FEL_RUN, addr, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Records every fel_write payload; returns scripted (or zero) reads.
    pub(super) struct MockFelIo {
        pub writes: Vec<Vec<u8>>,
        pub reads: VecDeque<Vec<u8>>,
    }
    impl MockFelIo {
        pub fn new() -> Self {
            Self {
                writes: Vec::new(),
                reads: VecDeque::new(),
            }
        }
    }
    impl FelIo for MockFelIo {
        fn fel_write(&mut self, payload: &[u8]) -> Result<(), RfError> {
            self.writes.push(payload.to_vec());
            Ok(())
        }
        fn fel_read(&mut self, len: usize) -> Result<Vec<u8>, RfError> {
            // Default read is zeros (status byte[4]==0 => ok).
            Ok(self.reads.pop_front().unwrap_or_else(|| vec![0u8; len]))
        }
    }

    #[test]
    fn write_memory_single_chunk_issues_msg_then_data_then_status() {
        let mut io = MockFelIo::new();
        write_memory(&mut io, 0x4740_0000, &[0xaa, 0xbb, 0xcc, 0xdd]).unwrap();
        // Two writes: the FEL_DOWNLOAD message, then the 4-byte payload.
        assert_eq!(io.writes.len(), 2);
        assert_eq!(&io.writes[0][0..4], &[0x01, 0x01, 0x00, 0x00]); // FEL_DOWNLOAD
        assert_eq!(&io.writes[0][4..8], &0x4740_0000u32.to_le_bytes());
        assert_eq!(&io.writes[0][8..12], &4u32.to_le_bytes());
        assert_eq!(io.writes[1], vec![0xaa, 0xbb, 0xcc, 0xdd]);
    }

    #[test]
    fn write_memory_chunks_above_max_bulk() {
        let mut io = MockFelIo::new();
        let data = vec![0u8; fel::MAX_BULK + 0x8000]; // 1.5 chunks
        write_memory(&mut io, fel::TRANSFER_BASE, &data).unwrap();
        // chunk1 msg+data, chunk2 msg+data = 4 writes.
        assert_eq!(io.writes.len(), 4);
        assert_eq!(&io.writes[0][4..8], &fel::TRANSFER_BASE.to_le_bytes());
        assert_eq!(
            &io.writes[2][4..8],
            &(fel::TRANSFER_BASE + fel::MAX_BULK as u32).to_le_bytes()
        );
    }

    #[test]
    fn write_memory_errors_on_bad_status() {
        let mut io = MockFelIo::new();
        io.reads.push_back(vec![0xff, 0xff, 0, 0, 1, 0, 0, 0]); // State=1
        let err = write_memory(&mut io, 0x4740_0000, &[1, 2, 3, 4]).unwrap_err();
        assert!(matches!(err, RfError::MemoryWriteFailed(_)));
    }

    #[test]
    fn exec_issues_fel_run() {
        let mut io = MockFelIo::new();
        exec(&mut io, 0x4700_0000).unwrap();
        assert_eq!(io.writes.len(), 1);
        assert_eq!(&io.writes[0][0..4], &[0x02, 0x01, 0x00, 0x00]); // FEL_RUN
        assert_eq!(&io.writes[0][4..8], &0x4700_0000u32.to_le_bytes());
    }
}
```

- [ ] **Step 2: Wire the module**

In `src-tauri/src/device/mod.rs` add: `pub mod memboot;`

- [ ] **Step 3: Run the tests**

Run: `cd src-tauri && cargo test device::memboot::tests`
Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/device/memboot.rs src-tauri/src/device/mod.rs
git commit -m "feat(core): add FelIo trait and FEL memory primitives"
```

---

## Task 5: Memboot orchestration (init_dram, run_uboot_cmd, memboot) + FelTransport impl

**Files:**
- Modify: `src-tauri/src/device/memboot.rs`, `src-tauri/src/device/usb.rs`

- [ ] **Step 1: Add orchestration + failing tests to `memboot.rs`**

Append to `src-tauri/src/device/memboot.rs` (above the `#[cfg(test)]` module):

```rust
/// Bring up DRAM: load the fes1 blob into SRAM and execute it.
pub fn init_dram<T: FelIo>(io: &mut T, fes1: &[u8]) -> Result<(), RfError> {
    write_memory(io, fel::FES1_BASE, fes1)?;
    exec(io, fel::FES1_BASE)
}

/// Load U-Boot, patch a command string at its `bootcmd=` marker, and execute it.
pub fn run_uboot_cmd<T: FelIo>(io: &mut T, uboot: &[u8], cmd: &str) -> Result<(), RfError> {
    let marker = b"bootcmd=";
    let offset = uboot
        .windows(marker.len())
        .position(|w| w == marker)
        .map(|i| i + marker.len())
        .ok_or_else(|| RfError::ExecFailed("uboot 'bootcmd=' marker not found".into()))?;
    write_memory(io, fel::UBOOT_BASE, uboot)?;
    let mut cmd_buf = cmd.as_bytes().to_vec();
    cmd_buf.push(0); // NUL terminator
    write_memory(io, fel::UBOOT_BASE + offset as u32, &cmd_buf)?;
    exec(io, fel::UBOOT_BASE)
}

/// Full memboot: init DRAM, stage the boot image, then boota it via U-Boot.
pub fn memboot<T: FelIo>(
    io: &mut T,
    fes1: &[u8],
    uboot: &[u8],
    boot_img: &[u8],
) -> Result<(), RfError> {
    init_dram(io, fes1)?;

    // Pad the boot image up to a sector boundary; bound by transfer window.
    let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
    if padded as u32 > fel::TRANSFER_MAX_SIZE {
        return Err(RfError::ExecFailed(format!(
            "boot image too large: {padded} > {}",
            fel::TRANSFER_MAX_SIZE
        )));
    }
    let mut kernel = boot_img.to_vec();
    kernel.resize(padded, 0);
    write_memory(io, fel::TRANSFER_BASE, &kernel)?;

    let cmd = format!("boota {:x}", fel::TRANSFER_BASE);
    run_uboot_cmd(io, uboot, &cmd)
}
```

Add these tests inside the existing `#[cfg(test)] mod tests` (after the Task-4 tests):

```rust
    #[test]
    fn init_dram_writes_fes1_then_execs_it() {
        let mut io = MockFelIo::new();
        init_dram(&mut io, &[0x11, 0x22, 0x33, 0x44]).unwrap();
        // write msg + data, then exec msg.
        assert_eq!(io.writes.len(), 3);
        assert_eq!(&io.writes[0][4..8], &fel::FES1_BASE.to_le_bytes()); // download to 0x2000
        assert_eq!(&io.writes[2][0..4], &[0x02, 0x01, 0x00, 0x00]); // FEL_RUN
        assert_eq!(&io.writes[2][4..8], &fel::FES1_BASE.to_le_bytes());
    }

    #[test]
    fn run_uboot_cmd_patches_command_at_marker() {
        let mut io = MockFelIo::new();
        // uboot blob: 4 bytes, then "bootcmd=", then 16 spare bytes.
        let mut uboot = vec![0u8; 4];
        uboot.extend_from_slice(b"bootcmd=");
        uboot.extend_from_slice(&[0u8; 16]);
        let marker_off = 4 + b"bootcmd=".len(); // 12
        run_uboot_cmd(&mut io, &uboot, "boota 47400000").unwrap();
        // writes: uboot msg, uboot data, cmd msg, cmd data, exec msg.
        assert_eq!(io.writes.len(), 5);
        assert_eq!(&io.writes[0][4..8], &fel::UBOOT_BASE.to_le_bytes());
        // command is written at UBOOT_BASE + marker offset.
        assert_eq!(
            &io.writes[2][4..8],
            &(fel::UBOOT_BASE + marker_off as u32).to_le_bytes()
        );
        assert_eq!(&io.writes[3][0..14], b"boota 47400000");
        assert_eq!(io.writes[3][14], 0); // NUL
        assert_eq!(&io.writes[4][0..4], &[0x02, 0x01, 0x00, 0x00]); // FEL_RUN
        assert_eq!(&io.writes[4][4..8], &fel::UBOOT_BASE.to_le_bytes());
    }

    #[test]
    fn run_uboot_cmd_errors_without_marker() {
        let mut io = MockFelIo::new();
        let err = run_uboot_cmd(&mut io, &[0u8; 64], "boota 0").unwrap_err();
        assert!(matches!(err, RfError::ExecFailed(_)));
    }

    #[test]
    fn memboot_runs_dram_then_image_then_boota() {
        let mut io = MockFelIo::new();
        let mut uboot = vec![0u8; 4];
        uboot.extend_from_slice(b"bootcmd=");
        uboot.extend_from_slice(&[0u8; 16]);
        memboot(&mut io, &[0xaa; 8], &uboot, &[0xbb; 16]).unwrap();
        // First op is DRAM init: download to FES1_BASE.
        assert_eq!(&io.writes[0][4..8], &fel::FES1_BASE.to_le_bytes());
        // The boot image is staged at TRANSFER_BASE somewhere in the sequence.
        let staged = io
            .writes
            .iter()
            .any(|w| w.len() >= 8 && &w[0..4] == [0x01, 0x01, 0x00, 0x00] && &w[4..8] == fel::TRANSFER_BASE.to_le_bytes());
        assert!(staged, "boot image must be written to TRANSFER_BASE");
        // The final write is the FEL_RUN of U-Boot.
        let last = io.writes.last().unwrap();
        assert_eq!(&last[0..4], &[0x02, 0x01, 0x00, 0x00]);
        assert_eq!(&last[4..8], &fel::UBOOT_BASE.to_le_bytes());
    }
```

- [ ] **Step 2: Implement `FelIo` for `FelTransport`**

In `src-tauri/src/device/usb.rs`, add:

```rust
impl crate::device::memboot::FelIo for FelTransport {
    fn fel_write(&mut self, payload: &[u8]) -> Result<(), RfError> {
        FelTransport::fel_write(self, payload)
    }
    fn fel_read(&mut self, len: usize) -> Result<Vec<u8>, RfError> {
        FelTransport::fel_read(self, len)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test device::memboot`
Expected: 8 passed (4 from Task 4 + 4 here).

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: clean. (`div_ceil` is stable; if the toolchain is older and clippy flags it, replace with `(boot_img.len() + fel::SECTOR_SIZE - 1) / fel::SECTOR_SIZE`.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/device/memboot.rs src-tauri/src/device/usb.rs
git commit -m "feat(core): add memboot orchestration over FelIo"
```

---

## Task 6: Bundle `fes1.bin` + `blobs.rs` (fes1 accessor)

**Files:**
- Create: `src-tauri/resources/fes1.bin` (copied), `src-tauri/src/device/blobs.rs`
- Modify: `src-tauri/src/device/mod.rs`

- [ ] **Step 1: Copy the fes1 blob into the repo**

```bash
mkdir -p src-tauri/resources
cp ../Hakchi2-CE/hakchi_gui/data/fes1.bin src-tauri/resources/fes1.bin
```

Verify it is ~14 KB: `ls -l src-tauri/resources/fes1.bin` (expected ~14048 bytes).

- [ ] **Step 2: Create `blobs.rs` with the fes1 accessor + test**

Create `src-tauri/src/device/blobs.rs`:

```rust
/// The Allwinner DRAM-init blob, bundled (small, redistributable boot tooling).
const FES1: &[u8] = include_bytes!("../../resources/fes1.bin");

/// The bundled fes1 DRAM-init blob.
pub fn fes1() -> &'static [u8] {
    FES1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fes1_is_present_and_nontrivial() {
        assert!(fes1().len() > 1024, "fes1 blob looks too small");
    }
}
```

- [ ] **Step 3: Wire the module**

In `src-tauri/src/device/mod.rs` add: `pub mod blobs;`

- [ ] **Step 4: Run the test**

Run: `cd src-tauri && cargo test device::blobs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/resources/fes1.bin src-tauri/src/device/blobs.rs src-tauri/src/device/mod.rs
git commit -m "feat(core): bundle fes1 DRAM-init blob"
```

---

## Task 7: `hmod.rs` — fetch + cache + extract uboot/boot.img

**Files:**
- Create: `src-tauri/src/device/hmod.rs`
- Modify: `src-tauri/src/device/mod.rs`, `src-tauri/Cargo.toml`

- [ ] **Step 1: Add dependencies**

In `src-tauri/Cargo.toml` under `[dependencies]`:

```toml
ureq = "2"
flate2 = "1"
tar = "0.4"
```

Under `[dev-dependencies]` (for building the test fixture archive):

```toml
tar = "0.4"
```

(If `tar` is already in `[dependencies]`, it is available to tests too; the dev-dependency line is optional — skip it if it causes a duplicate-key error.)

- [ ] **Step 2: Write `hmod.rs` with extraction logic + failing test**

Create `src-tauri/src/device/hmod.rs`:

```rust
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::RfError;

pub const HMOD_URL: &str = "https://hakchi.net/hakchi/hmods/hakchi-latest.hmod";
pub const HMOD_FILENAME: &str = "hakchi-latest.hmod";

/// Path to the cached hmod inside `cache_dir`.
pub fn cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(HMOD_FILENAME)
}

/// Ensure the hmod is cached locally, downloading it once if absent. Returns the
/// cached file path.
pub fn ensure_hmod(cache_dir: &Path) -> Result<PathBuf, RfError> {
    let path = cache_path(cache_dir);
    if path.exists() {
        return Ok(path);
    }
    let bytes = download(HMOD_URL)?;
    fs::create_dir_all(cache_dir).map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    fs::write(&path, &bytes).map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    Ok(path)
}

fn download(url: &str) -> Result<Vec<u8>, RfError> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| RfError::HmodFetchFailed(e.to_string()))?;
    Ok(buf)
}

/// Extract a single entry (by path, e.g. "boot/uboot.bin") from an hmod archive.
/// Handles a gzip-compressed tar or a plain tar (sniffed by magic).
pub fn extract_entry(archive: &[u8], name: &str) -> Result<Vec<u8>, RfError> {
    let tar_bytes = if archive.starts_with(&[0x1f, 0x8b]) {
        let mut d = flate2::read::GzDecoder::new(archive);
        let mut v = Vec::new();
        d.read_to_end(&mut v)
            .map_err(|e| RfError::HmodExtractFailed(format!("gunzip: {e}")))?;
        v
    } else {
        archive.to_vec()
    };

    let mut tar = tar::Archive::new(&tar_bytes[..]);
    let entries = tar
        .entries()
        .map_err(|e| RfError::HmodExtractFailed(e.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| RfError::HmodExtractFailed(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| RfError::HmodExtractFailed(e.to_string()))?
            .to_string_lossy()
            .trim_start_matches("./")
            .to_string();
        if path == name {
            let mut out = Vec::new();
            entry
                .read_to_end(&mut out)
                .map_err(|e| RfError::HmodExtractFailed(e.to_string()))?;
            return Ok(out);
        }
    }
    Err(RfError::HmodExtractFailed(format!("entry not found: {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tar() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in [("boot/uboot.bin", b"UBOOT" as &[u8]), ("boot/boot.img", b"IMG")] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_cksum();
            builder.append_data(&mut header, name, data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn extracts_named_entries_from_plain_tar() {
        let tar = make_tar();
        assert_eq!(extract_entry(&tar, "boot/uboot.bin").unwrap(), b"UBOOT");
        assert_eq!(extract_entry(&tar, "boot/boot.img").unwrap(), b"IMG");
    }

    #[test]
    fn extracts_from_gzipped_tar() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let tar = make_tar();
        let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&tar).unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(extract_entry(&gz, "boot/uboot.bin").unwrap(), b"UBOOT");
    }

    #[test]
    fn missing_entry_errors() {
        let tar = make_tar();
        let err = extract_entry(&tar, "boot/nope").unwrap_err();
        assert!(matches!(err, RfError::HmodExtractFailed(_)));
    }
}
```

- [ ] **Step 3: Wire the module**

In `src-tauri/src/device/mod.rs` add: `pub mod hmod;`

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test device::hmod`
Expected: 3 passed. (First build compiles ureq/flate2/tar — allow a few minutes.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/device/hmod.rs src-tauri/src/device/mod.rs
git commit -m "feat(core): add hmod fetch + tar extraction"
```

---

## Task 8: `blobs.rs` uboot/boot.img accessors + `ensure_blobs`

**Files:**
- Modify: `src-tauri/src/device/blobs.rs`

- [ ] **Step 1: Add cached-blob accessors**

Append to `src-tauri/src/device/blobs.rs`:

```rust
use std::fs;
use std::path::Path;

use crate::device::hmod;
use crate::error::RfError;

/// Ensure the runtime blobs (inside hakchi-latest.hmod) are downloaded/cached.
pub fn ensure_blobs(cache_dir: &Path) -> Result<(), RfError> {
    hmod::ensure_hmod(cache_dir)?;
    Ok(())
}

fn read_hmod(cache_dir: &Path) -> Result<Vec<u8>, RfError> {
    let path = hmod::cache_path(cache_dir);
    fs::read(&path).map_err(|_| {
        RfError::BlobMissing("hmod not cached; run ensure_blobs first".into())
    })
}

/// The U-Boot binary extracted from the cached hmod.
pub fn uboot(cache_dir: &Path) -> Result<Vec<u8>, RfError> {
    hmod::extract_entry(&read_hmod(cache_dir)?, "boot/uboot.bin")
}

/// The boot image extracted from the cached hmod.
pub fn boot_img(cache_dir: &Path) -> Result<Vec<u8>, RfError> {
    hmod::extract_entry(&read_hmod(cache_dir)?, "boot/boot.img")
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn uboot_errors_when_hmod_absent() {
        let dir = std::env::temp_dir().join("retroforge-blobs-test-absent");
        let _ = std::fs::remove_dir_all(&dir);
        let err = uboot(&dir).unwrap_err();
        assert!(matches!(err, RfError::BlobMissing(_)));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test device::blobs`
Expected: 2 passed (fes1 + cache).

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/device/blobs.rs
git commit -m "feat(core): add hmod-backed uboot/boot.img accessors"
```

---

## Task 9: `memboot()` command + worker thread + FEL-exit polling

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the command, progress event, and worker**

In `src-tauri/src/lib.rs`, add the imports and code below, and register `memboot` in the existing `generate_handler!` list (alongside `get_device_status`).

```rust
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::device::memboot as memboot_ops; // module alias avoids clashing with the `memboot` command fn
use crate::device::usb::{is_fel_device, FelTransport};
use crate::device::{blobs, fel};
use crate::error::RfError;

const MEMBOOT_PROGRESS_EVENT: &str = "memboot-progress";

#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
enum MembootProgress {
    FetchingPayload,
    Connecting,
    InitDram,
    LoadingImage,
    LoadingUboot,
    Executing,
    WaitingForExit,
    Success,
    Failed { message: String },
}

fn emit_progress(app: &AppHandle, p: MembootProgress) {
    let _ = app.emit(MEMBOOT_PROGRESS_EVENT, p);
}

/// True if a FEL device is currently enumerated.
fn fel_present() -> bool {
    let Ok(ctx) = rusb::Context::new() else {
        return false;
    };
    let Ok(devices) = ctx.devices() else {
        return false;
    };
    devices.iter().any(|d| {
        d.device_descriptor()
            .map(|desc| is_fel_device(desc.vendor_id(), desc.product_id()))
            .unwrap_or(false)
    })
}

/// Open the currently-present FEL device as a transport.
fn open_fel() -> Result<FelTransport, RfError> {
    let ctx = rusb::Context::new().map_err(|e| RfError::FelProtocolError(e.to_string()))?;
    let devices = ctx
        .devices()
        .map_err(|e| RfError::FelProtocolError(e.to_string()))?;
    for device in devices.iter() {
        if let Ok(desc) = device.device_descriptor() {
            if is_fel_device(desc.vendor_id(), desc.product_id()) {
                return FelTransport::open(&device);
            }
        }
    }
    Err(RfError::DeviceGone)
}

fn run_memboot(app: &AppHandle, cache_dir: std::path::PathBuf) -> Result<(), RfError> {
    emit_progress(app, MembootProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let fes1 = blobs::fes1();
    let uboot = blobs::uboot(&cache_dir)?;
    let boot_img = blobs::boot_img(&cache_dir)?;

    emit_progress(app, MembootProgress::Connecting);
    let mut transport = open_fel()?;

    // The orchestration emits coarse phases; the underlying steps run inside memboot().
    emit_progress(app, MembootProgress::InitDram);
    memboot_ops::init_dram(&mut transport, fes1)?;
    emit_progress(app, MembootProgress::LoadingImage);
    {
        let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
        if padded as u32 > fel::TRANSFER_MAX_SIZE {
            return Err(RfError::ExecFailed("boot image too large".into()));
        }
        let mut kernel = boot_img.clone();
        kernel.resize(padded, 0);
        memboot_ops::write_memory(&mut transport, fel::TRANSFER_BASE, &kernel)?;
    }
    emit_progress(app, MembootProgress::LoadingUboot);
    emit_progress(app, MembootProgress::Executing);
    let cmd = format!("boota {:x}", fel::TRANSFER_BASE);
    memboot_ops::run_uboot_cmd(&mut transport, &uboot, &cmd)?;

    // Success = the FEL device leaves the bus (it is now running the image).
    drop(transport);
    emit_progress(app, MembootProgress::WaitingForExit);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if !fel_present() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(RfError::MembootTimeout)
}

#[tauri::command]
fn memboot(app: AppHandle) {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    std::thread::spawn(move || {
        let result = run_memboot(&app, cache_dir);
        match result {
            Ok(()) => emit_progress(&app, MembootProgress::Success),
            Err(e) => emit_progress(
                &app,
                MembootProgress::Failed {
                    message: e.to_string(),
                },
            ),
        }
    });
}
```

Note: this reuses the `memboot_ops` building blocks (`init_dram`, `write_memory`, `run_uboot_cmd`) directly so each phase can emit progress. The standalone `memboot_ops::memboot` function remains the unit-tested reference path; the small padding/size-check duplication here is deliberate so progress can be reported between steps. `is_fel_device` and `FelTransport` are already `pub` in `usb.rs`; `fel` is a `pub mod`. If clippy flags any import as unused, drop it.

- [ ] **Step 2: Verify it compiles + clippy**

Run: `cd src-tauri && cargo build`
Expected: builds. Remove any unused import clippy flags.

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `cd src-tauri && cargo test`
Expected: all prior tests still pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(core): add memboot command with progress events and exit polling"
```

---

## Task 10: Frontend — memboot trigger + progress UI

**Files:**
- Create: `src/lib/memboot.ts`, `src/components/MembootButton.tsx`, `src/components/MembootButton.test.tsx`
- Modify: `src/App.tsx`

- [ ] **Step 1: Typed wrapper**

Create `src/lib/memboot.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type MembootProgress =
  | { phase: "fetchingPayload" }
  | { phase: "connecting" }
  | { phase: "initDram" }
  | { phase: "loadingImage" }
  | { phase: "loadingUboot" }
  | { phase: "executing" }
  | { phase: "waitingForExit" }
  | { phase: "success" }
  | { phase: "failed"; message: string };

export function startMemboot(): Promise<void> {
  return invoke<void>("memboot");
}

export function onMembootProgress(
  handler: (p: MembootProgress) => void,
): Promise<UnlistenFn> {
  return listen<MembootProgress>("memboot-progress", (e) => handler(e.payload));
}
```

- [ ] **Step 2: Failing test for the button**

Create `src/components/MembootButton.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { MembootButton } from "./MembootButton";

describe("MembootButton", () => {
  it("is disabled when device not connected", () => {
    render(<MembootButton connected={false} />);
    expect(screen.getByRole("button", { name: /memboot/i })).toBeDisabled();
  });

  it("is enabled when connected", () => {
    render(<MembootButton connected={true} />);
    expect(screen.getByRole("button", { name: /memboot/i })).toBeEnabled();
  });

  it("shows a progress label when a phase is set", () => {
    render(<MembootButton connected={true} initialPhase={{ phase: "initDram" }} />);
    expect(screen.getByText(/init.*dram/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run to verify it fails**

Run: `npm test`
Expected: FAIL — cannot resolve `./MembootButton`.

- [ ] **Step 4: Implement the button**

Create `src/components/MembootButton.tsx`:

```tsx
import { useEffect, useState } from "react";
import {
  onMembootProgress,
  startMemboot,
  type MembootProgress,
} from "@/lib/memboot";

const LABELS: Record<MembootProgress["phase"], string> = {
  fetchingPayload: "Fetching boot payload…",
  connecting: "Connecting…",
  initDram: "Initializing DRAM…",
  loadingImage: "Loading boot image…",
  loadingUboot: "Loading U-Boot…",
  executing: "Executing…",
  waitingForExit: "Waiting for device to boot…",
  success: "Booted — check the TV.",
  failed: "Memboot failed.",
};

export function MembootButton({
  connected,
  initialPhase,
}: {
  connected: boolean;
  initialPhase?: MembootProgress;
}) {
  const [progress, setProgress] = useState<MembootProgress | undefined>(initialPhase);

  useEffect(() => {
    const unlisten = onMembootProgress(setProgress);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const running =
    progress !== undefined &&
    progress.phase !== "success" &&
    progress.phase !== "failed";

  return (
    <div className="mt-4 flex flex-col gap-2">
      <button
        type="button"
        disabled={!connected || running}
        onClick={() => {
          setProgress({ phase: "fetchingPayload" });
          startMemboot().catch(() => setProgress({ phase: "failed", message: "invoke failed" }));
        }}
        className="rounded-md border px-4 py-2 text-sm font-medium disabled:opacity-50"
      >
        Memboot (test)
      </button>
      {progress && (
        <p className="text-sm text-gray-600">
          {LABELS[progress.phase]}
          {progress.phase === "failed" ? ` (${progress.message})` : ""}
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `npm test`
Expected: all pass (StatusPanel + MembootButton).

- [ ] **Step 6: Wire into `App.tsx`**

In `src/App.tsx`, render the button below the StatusPanel, passing connected state:

```tsx
import { MembootButton } from "@/components/MembootButton";
// ...inside the returned <main>, after <StatusPanel ... />:
        <MembootButton connected={status.state === "connected"} />
```

- [ ] **Step 7: Verify**

Run: `npx tsc --noEmit` → clean.
Run: `npm run build` → succeeds.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(ui): add memboot trigger button with progress"
```

---

## Task 11: gitignore the blob cache

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Ignore the hmod cache**

Append to `.gitignore`:

```gitignore
# Cached runtime boot blobs (downloaded hakchi hmod)
*.hmod
```

- [ ] **Step 2: Commit**

```bash
git add .gitignore
git commit -m "chore: gitignore cached hmod blobs"
```

---

## Task 12: Hardware acceptance for memboot

**Files:**
- Create: `docs/HARDWARE-ACCEPTANCE-MEMBOOT.md`

- [ ] **Step 1: Write the checklist**

Create `docs/HARDWARE-ACCEPTANCE-MEMBOOT.md`:

```markdown
# Hardware Acceptance — Memboot Foundation (slice 2)

Verifies memboot against a real NES Classic Mini. Memboot is RAM-only; a failure
just hangs the device (recover by unplug → hold RESET → replug into FEL).

## Prepare
1. Device in FEL mode (hold RESET, plug USB, release), WinUSB bound, panel green
   "Connected — Allwinner R16" (see HARDWARE-ACCEPTANCE.md).
2. TV/monitor connected to the console's HDMI so you can see it boot.
3. Internet available (first run downloads hakchi-latest.hmod).

## Run
4. `npm run tauri dev`.
5. Click "Memboot (test)". Watch the progress labels:
   Fetching payload → Connecting → Initializing DRAM → Loading boot image →
   Loading U-Boot → Executing → Waiting for device to boot.
6. PASS criteria:
   - The app reaches "Booted — check the TV." (the FEL device left the bus), AND
   - The console visibly boots the loaded image on the TV.

## If it fails
- "fetch hakchi hmod" error → check internet; the hmod URL is
  https://hakchi.net/hakchi/hmods/hakchi-latest.hmod.
- "entry not found" → the hmod layout changed; confirm `boot/uboot.bin` and
  `boot/boot.img` entry names and the archive compression (gzip vs plain tar) and
  adjust `hmod::extract_entry`.
- Timeout (device never leaves FEL) → likely a blob/address mismatch. Confirm the
  memory map (FES1_BASE/UBOOT_BASE/TRANSFER_BASE) and the `bootcmd=` marker against
  the reference, and that the "normal" (non-SD) U-Boot is used.
- Unplug → hold RESET → replug to return to FEL and retry.

## Capture
7. Record the outcome here (PASS/FAIL + any address/marker corrections applied).
```

- [ ] **Step 2: Commit**

```bash
git add docs/HARDWARE-ACCEPTANCE-MEMBOOT.md
git commit -m "docs: add memboot hardware acceptance checklist"
```

---

## Done criteria

- `cargo test` (all units incl. memboot orchestration + hmod extraction), `cargo clippy -D warnings`, `npm test`, `tsc --noEmit` all pass.
- Slice-1 hardware re-verify after the Task-2 refactor: panel still reaches green.
- Memboot hardware acceptance: FEL device leaves the bus and the console boots on the TV.
