# Clovershell Comms Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Memboot the device into shell mode and run a command on it from the host over the clovershell USB protocol, returning the command's output.

**Architecture:** A pure Android boot-image cmdline patch (`bootimg.rs`) injects `hakchi-clovershell`; a `ClovershellIo`-trait-based protocol client (`clovershell.rs`) implements ping + exec, unit-tested with a mock and backed by an rusb transport; `lib.rs` chains the slice-2 memboot (with the patched cmdline) → poll for clovershell → exec.

**Tech Stack:** Rust (rusb, thiserror), Tauri v2, React/TS. Builds on slices 1–2.

**Reference:** `../Hakchi2-CE/hakchi_gui/Clovershell/ClovershellConnection.cs`. Clean-room reimplementation; no Hakchi code copied.

## Clovershell protocol (from the reference)

- USB `1F3A:EFE8` (same as FEL), config 1, interface 0, bulk endpoints **IN `0x81` / OUT `0x01`**.
- Packet = 4-byte header `[cmd, arg, len_lo, len_hi]` + `len` bytes payload. One bulk read may carry multiple packets or a partial one.
- Commands: `CMD_PING=0`, `CMD_PONG=1`, `CMD_SHELL_KILL_ALL=8`, `CMD_EXEC_NEW_REQ=9`, `CMD_EXEC_NEW_RESP=10`, `CMD_EXEC_STDOUT=13`, `CMD_EXEC_STDERR=14`, `CMD_EXEC_RESULT=15`, `CMD_EXEC_KILL_ALL=17`.
- Exec: send `EXEC_NEW_REQ(arg=0, data=command)`; device replies `NEW_RESP(arg=id, data=command)`, then `STDOUT(arg=id, data)` / `STDERR`, then `RESULT(arg=id, data[0]=exit code)`.
- Ping: send `PING(0)`, device replies `PONG`.
- On connect: send `SHELL_KILL_ALL` then `EXEC_KILL_ALL` (4-byte headers, no payload) and drain stale input.

## File Structure

| Path | Responsibility |
|------|----------------|
| `src-tauri/src/device/bootimg.rs` | Pure Android boot-image cmdline patch. |
| `src-tauri/src/device/clovershell.rs` | `ClovershellIo` trait, framing, ping, exec, mock + rusb transport. |
| `src-tauri/src/device/mod.rs` | Wire new modules. |
| `src-tauri/src/error.rs` | New RfError variants. |
| `src-tauri/src/lib.rs` | `open_shell_and_run` command; rename poll-gate to cover shell ops. |
| `src/lib/shell.ts`, `src/components/ShellRunner.tsx` | Frontend trigger + output. |

---

## Task 1: `bootimg.rs` — inject kernel cmdline

**Files:**
- Create: `src-tauri/src/device/bootimg.rs`
- Modify: `src-tauri/src/device/mod.rs`, `src-tauri/src/error.rs`

- [ ] **Step 1: Add error variant**

In `src-tauri/src/error.rs`, add to the `RfError` enum:

```rust
    #[error("kernel cmdline has no room for the injected token")]
    CmdlineTooLong,
```

- [ ] **Step 2: Create `bootimg.rs` with tests**

Create `src-tauri/src/device/bootimg.rs`:

```rust
use crate::error::RfError;

// Android boot image header: the kernel command line is a NUL-terminated string
// in a 512-byte field at offset 64.
const CMDLINE_OFFSET: usize = 64;
const CMDLINE_SIZE: usize = 512;
const MAGIC: &[u8; 8] = b"ANDROID!";

/// Append ` {token}` to the kernel command line of an Android boot image,
/// returning a modified copy. Errors if the image is malformed or the cmdline
/// field has no room.
pub fn inject_cmdline(boot_img: &[u8], token: &str) -> Result<Vec<u8>, RfError> {
    if boot_img.len() < CMDLINE_OFFSET + CMDLINE_SIZE {
        return Err(RfError::ExecFailed(
            "boot image too small for an Android header".into(),
        ));
    }
    if &boot_img[0..8] != MAGIC {
        return Err(RfError::ExecFailed("not an Android boot image".into()));
    }
    let mut out = boot_img.to_vec();
    let field = &mut out[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE];
    let used = field.iter().position(|&b| b == 0).unwrap_or(CMDLINE_SIZE);
    let addition = format!(" {token}");
    let add = addition.as_bytes();
    // Need room for the addition plus a terminating NUL.
    if used + add.len() + 1 > CMDLINE_SIZE {
        return Err(RfError::CmdlineTooLong);
    }
    field[used..used + add.len()].copy_from_slice(add);
    field[used + add.len()] = 0;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot_img_with_cmdline(cmdline: &str) -> Vec<u8> {
        let mut v = vec![0u8; CMDLINE_OFFSET + CMDLINE_SIZE + 16];
        v[0..8].copy_from_slice(MAGIC);
        v[CMDLINE_OFFSET..CMDLINE_OFFSET + cmdline.len()].copy_from_slice(cmdline.as_bytes());
        v
    }

    fn read_cmdline(img: &[u8]) -> String {
        let field = &img[CMDLINE_OFFSET..CMDLINE_OFFSET + CMDLINE_SIZE];
        let end = field.iter().position(|&b| b == 0).unwrap_or(CMDLINE_SIZE);
        String::from_utf8_lossy(&field[..end]).to_string()
    }

    #[test]
    fn appends_token_after_existing_cmdline() {
        let img = boot_img_with_cmdline("console=ttyS0");
        let out = inject_cmdline(&img, "hakchi-clovershell").unwrap();
        assert_eq!(read_cmdline(&out), "console=ttyS0 hakchi-clovershell");
    }

    #[test]
    fn appends_to_empty_cmdline() {
        let img = boot_img_with_cmdline("");
        let out = inject_cmdline(&img, "x").unwrap();
        assert_eq!(read_cmdline(&out), " x");
    }

    #[test]
    fn errors_when_field_full() {
        let img = boot_img_with_cmdline(&"a".repeat(CMDLINE_SIZE - 2));
        let err = inject_cmdline(&img, "toolong").unwrap_err();
        assert!(matches!(err, RfError::CmdlineTooLong));
    }

    #[test]
    fn rejects_non_android_image() {
        let mut img = boot_img_with_cmdline("x");
        img[0] = b'X';
        assert!(inject_cmdline(&img, "y").is_err());
    }

    #[test]
    fn rejects_too_small() {
        assert!(inject_cmdline(&[0u8; 8], "y").is_err());
    }
}
```

- [ ] **Step 3: Wire the module**

In `src-tauri/src/device/mod.rs` add: `pub mod bootimg;`

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test device::bootimg`
Expected: 5 passed.

NOTE: `inject_cmdline` is unused by production until Task 5; if `cargo clippy -D warnings` flags it dead, add `#[allow(dead_code)] // consumed by the shell command in Task 5`. Just ensure `cargo test device::bootimg` passes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/device/bootimg.rs src-tauri/src/device/mod.rs src-tauri/src/error.rs
git commit -m "feat(core): add Android boot-image cmdline injection"
```

---

## Task 2: `clovershell.rs` — protocol core (trait, framing, ping, exec)

Pure protocol over a `ClovershellIo` trait, tested with a mock. No USB here.

**Files:**
- Create: `src-tauri/src/device/clovershell.rs`
- Modify: `src-tauri/src/device/mod.rs`, `src-tauri/src/error.rs`

- [ ] **Step 1: Add error variant**

In `src-tauri/src/error.rs`, add:

```rust
    #[error("clovershell protocol error: {0}")]
    ClovershellProtocol(String),
```

- [ ] **Step 2: Create `clovershell.rs` with the trait, framing, ping, exec, mock, and tests**

Create `src-tauri/src/device/clovershell.rs`:

```rust
use crate::error::RfError;

// Clovershell command bytes.
pub const CMD_PING: u8 = 0;
pub const CMD_PONG: u8 = 1;
pub const CMD_SHELL_KILL_ALL: u8 = 8;
pub const CMD_EXEC_NEW_REQ: u8 = 9;
pub const CMD_EXEC_NEW_RESP: u8 = 10;
pub const CMD_EXEC_STDOUT: u8 = 13;
pub const CMD_EXEC_STDERR: u8 = 14;
pub const CMD_EXEC_RESULT: u8 = 15;
pub const CMD_EXEC_KILL_ALL: u8 = 17;

/// One clovershell packet.
#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub cmd: u8,
    pub arg: u8,
    pub data: Vec<u8>,
}

/// The 4-byte clovershell header for a payload of `len` bytes.
pub fn header(cmd: u8, arg: u8, len: usize) -> [u8; 4] {
    [cmd, arg, (len & 0xFF) as u8, ((len >> 8) & 0xFF) as u8]
}

/// A framed clovershell byte channel. The rusb transport implements this; tests
/// use a recording/replaying mock.
pub trait ClovershellIo {
    fn write_packet(&mut self, cmd: u8, arg: u8, data: &[u8]) -> Result<(), RfError>;
    fn read_packet(&mut self) -> Result<Packet, RfError>;
}

/// Result of running a command on the device.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

/// Liveness check: send PING, wait for PONG (bounded by read timeouts and a
/// packet cap so a chatty device cannot loop forever).
pub fn ping<T: ClovershellIo>(io: &mut T) -> Result<(), RfError> {
    io.write_packet(CMD_PING, 0, &[])?;
    for _ in 0..64 {
        if io.read_packet()?.cmd == CMD_PONG {
            return Ok(());
        }
    }
    Err(RfError::ClovershellProtocol("no PONG after PING".into()))
}

/// Run a command, accumulating stdout/stderr until the RESULT packet carries the
/// exit code (a single byte).
pub fn exec<T: ClovershellIo>(io: &mut T, command: &str) -> Result<ExecOutput, RfError> {
    io.write_packet(CMD_EXEC_NEW_REQ, 0, command.as_bytes())?;
    let mut out = ExecOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: -1,
    };
    loop {
        let p = io.read_packet()?;
        match p.cmd {
            CMD_EXEC_STDOUT => out.stdout.extend_from_slice(&p.data),
            CMD_EXEC_STDERR => out.stderr.extend_from_slice(&p.data),
            CMD_EXEC_RESULT => {
                out.exit_code = *p
                    .data
                    .first()
                    .ok_or_else(|| RfError::ClovershellProtocol("empty exec result".into()))?
                    as i32;
                return Ok(out);
            }
            // NEW_RESP assigns the exec id; PONG/others are not interesting here.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    pub(super) struct MockClovershellIo {
        pub writes: Vec<Packet>,
        pub reads: VecDeque<Packet>,
    }
    impl MockClovershellIo {
        fn new(reads: Vec<Packet>) -> Self {
            Self {
                writes: Vec::new(),
                reads: reads.into(),
            }
        }
    }
    impl ClovershellIo for MockClovershellIo {
        fn write_packet(&mut self, cmd: u8, arg: u8, data: &[u8]) -> Result<(), RfError> {
            self.writes.push(Packet {
                cmd,
                arg,
                data: data.to_vec(),
            });
            Ok(())
        }
        fn read_packet(&mut self) -> Result<Packet, RfError> {
            self.reads
                .pop_front()
                .ok_or_else(|| RfError::ClovershellProtocol("no more packets".into()))
        }
    }

    fn pkt(cmd: u8, data: &[u8]) -> Packet {
        Packet {
            cmd,
            arg: 1,
            data: data.to_vec(),
        }
    }

    #[test]
    fn header_encodes_len_little_endian() {
        assert_eq!(header(CMD_EXEC_NEW_REQ, 0, 0x0102), [9, 0, 0x02, 0x01]);
    }

    #[test]
    fn ping_returns_on_pong() {
        let mut io = MockClovershellIo::new(vec![pkt(CMD_PONG, &[])]);
        ping(&mut io).unwrap();
        assert_eq!(io.writes[0].cmd, CMD_PING);
    }

    #[test]
    fn exec_accumulates_stdout_and_exit() {
        let mut io = MockClovershellIo::new(vec![
            pkt(CMD_EXEC_NEW_RESP, b"uname -a"),
            pkt(CMD_EXEC_STDOUT, b"Linux "),
            pkt(CMD_EXEC_STDOUT, b"clover 4.4.0"),
            pkt(CMD_EXEC_RESULT, &[0]),
        ]);
        let out = exec(&mut io, "uname -a").unwrap();
        assert_eq!(out.stdout, b"Linux clover 4.4.0");
        assert_eq!(out.exit_code, 0);
        assert_eq!(io.writes[0].cmd, CMD_EXEC_NEW_REQ);
        assert_eq!(io.writes[0].data, b"uname -a");
    }

    #[test]
    fn exec_reports_nonzero_exit_and_stderr() {
        let mut io = MockClovershellIo::new(vec![
            pkt(CMD_EXEC_STDERR, b"nope"),
            pkt(CMD_EXEC_RESULT, &[1]),
        ]);
        let out = exec(&mut io, "false").unwrap();
        assert_eq!(out.stderr, b"nope");
        assert_eq!(out.exit_code, 1);
    }
}
```

- [ ] **Step 3: Wire the module**

In `src-tauri/src/device/mod.rs` add: `pub mod clovershell;`

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test device::clovershell`
Expected: 4 passed.

NOTE: items unused by production until Task 3/5 may trip `clippy -D warnings`; add narrow `#[allow(dead_code)]` with a "consumed in Task 3/5" comment only where clippy complains. Ensure the 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/device/clovershell.rs src-tauri/src/device/mod.rs src-tauri/src/error.rs
git commit -m "feat(core): add clovershell protocol core (ping, exec)"
```

---

## Task 3: `ClovershellTransport` — rusb-backed `ClovershellIo`

The rusb transport: open `1F3A:EFE8`, claim interface 0, bulk endpoints `0x81/0x01`, frame packets, and a buffered `read_packet` that handles multi-packet and partial reads. Hardware-verified later; compiled + clippy-clean here.

**Files:**
- Modify: `src-tauri/src/device/clovershell.rs`, `src-tauri/src/error.rs`

- [ ] **Step 1: Add error variant**

In `src-tauri/src/error.rs`, add:

```rust
    #[error("device shell not found (no clovershell on the bus)")]
    ShellNotFound,
```

- [ ] **Step 2: Add the transport (append to `clovershell.rs`)**

Append to `src-tauri/src/device/clovershell.rs`:

```rust
use std::time::Duration;

use rusb::{Direction, TransferType, UsbContext};

use crate::device::usb::is_fel_device;

const TIMEOUT: Duration = Duration::from_millis(3000);
pub const CLV_EP_IN: u8 = 0x81;
pub const CLV_EP_OUT: u8 = 0x01;

/// An open, interface-claimed clovershell device with a small read buffer so
/// `read_packet` can split multi-packet bulk reads and reassemble partial ones.
pub struct ClovershellTransport {
    handle: rusb::DeviceHandle<rusb::Context>,
    ep_in: u8,
    ep_out: u8,
    rbuf: Vec<u8>,
}

impl ClovershellTransport {
    /// Open the clovershell device (same VID/PID as FEL, but endpoints 0x81/0x01).
    /// Errors with ShellNotFound if no matching device is present.
    pub fn open() -> Result<Self, RfError> {
        let ctx = rusb::Context::new().map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
        let devices = ctx
            .devices()
            .map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
        for device in devices.iter() {
            let Ok(desc) = device.device_descriptor() else {
                continue;
            };
            // Same VID/PID as FEL; the clovershell device is told apart by its
            // bulk endpoints (IN 0x81 vs FEL's 0x82), checked below.
            if !is_fel_device(desc.vendor_id(), desc.product_id()) {
                continue;
            }
            let Ok(handle) = device.open() else { continue };
            let _ = handle.set_auto_detach_kernel_driver(true);
            if handle.claim_interface(0).is_err() {
                continue;
            }
            let (ep_in, ep_out) = match clv_endpoints(&device) {
                Some(eps) => eps,
                None => continue,
            };
            // Only the clovershell interface exposes IN 0x81 (FEL uses 0x82).
            if ep_in != CLV_EP_IN || ep_out != CLV_EP_OUT {
                continue;
            }
            let mut t = Self {
                handle,
                ep_in,
                ep_out,
                rbuf: Vec::new(),
            };
            t.kill_all_sessions()?;
            t.drain();
            return Ok(t);
        }
        Err(RfError::ShellNotFound)
    }

    /// Clear stale shell/exec sessions on the device, mirroring the reference.
    fn kill_all_sessions(&mut self) -> Result<(), RfError> {
        self.write_all(&header(CMD_SHELL_KILL_ALL, 0, 0))?;
        self.write_all(&header(CMD_EXEC_KILL_ALL, 0, 0))?;
        Ok(())
    }

    /// Discard any buffered/in-flight input left from a previous session.
    fn drain(&mut self) {
        self.rbuf.clear();
        let mut scratch = [0u8; 4096];
        while let Ok(n) = self
            .handle
            .read_bulk(self.ep_in, &mut scratch, Duration::from_millis(50))
        {
            if n == 0 {
                break;
            }
        }
    }

    fn write_all(&self, data: &[u8]) -> Result<(), RfError> {
        let mut pos = 0;
        while pos < data.len() {
            let n = self
                .handle
                .write_bulk(self.ep_out, &data[pos..], TIMEOUT)
                .map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
            if n == 0 {
                return Err(RfError::ClovershellProtocol("zero-length write".into()));
            }
            pos += n;
        }
        Ok(())
    }

    /// True when `rbuf` already holds at least one complete packet.
    fn has_full_packet(&self) -> Option<usize> {
        if self.rbuf.len() < 4 {
            return None;
        }
        let len = self.rbuf[2] as usize | ((self.rbuf[3] as usize) << 8);
        if self.rbuf.len() >= 4 + len {
            Some(len)
        } else {
            None
        }
    }
}

/// Find the first interface's bulk IN/OUT endpoint addresses.
fn clv_endpoints<T: UsbContext>(device: &rusb::Device<T>) -> Option<(u8, u8)> {
    let config = device.active_config_descriptor().ok()?;
    let (mut ep_in, mut ep_out) = (0u8, 0u8);
    for interface in config.interfaces() {
        for d in interface.descriptors() {
            for ep in d.endpoint_descriptors() {
                if ep.transfer_type() != TransferType::Bulk {
                    continue;
                }
                match ep.direction() {
                    Direction::In => ep_in = ep.address(),
                    Direction::Out => ep_out = ep.address(),
                }
            }
        }
    }
    Some((ep_in, ep_out))
}

impl ClovershellIo for ClovershellTransport {
    fn write_packet(&mut self, cmd: u8, arg: u8, data: &[u8]) -> Result<(), RfError> {
        self.write_all(&header(cmd, arg, data.len()))?;
        if !data.is_empty() {
            self.write_all(data)?;
        }
        Ok(())
    }

    fn read_packet(&mut self) -> Result<Packet, RfError> {
        let len = loop {
            if let Some(len) = self.has_full_packet() {
                break len;
            }
            let mut scratch = [0u8; 65536];
            let n = self
                .handle
                .read_bulk(self.ep_in, &mut scratch, TIMEOUT)
                .map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
            if n == 0 {
                return Err(RfError::ClovershellProtocol("zero-length read".into()));
            }
            self.rbuf.extend_from_slice(&scratch[..n]);
        };
        let cmd = self.rbuf[0];
        let arg = self.rbuf[1];
        let data = self.rbuf[4..4 + len].to_vec();
        self.rbuf.drain(0..4 + len);
        Ok(Packet { cmd, arg, data })
    }
}
```

- [ ] **Step 3: Verify it compiles clippy-clean**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: clean. If clippy flags the redundant `is_fel_device`/`FEL_VID` guard, simplify to a single `is_fel_device(desc.vendor_id(), desc.product_id())` check (keep behavior: skip non-FEL-id devices). If any item is dead until Task 5, add a narrow `#[allow(dead_code)]`.

Run: `cd src-tauri && cargo test device::clovershell`
Expected: the 4 protocol tests still pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/device/clovershell.rs src-tauri/src/error.rs
git commit -m "feat(core): add rusb clovershell transport"
```

---

## Task 4: rename the device-busy poll gate

Slice 2 added `MEMBOOT_ACTIVE` to keep the FEL poll thread off the bus during memboot. The shell op needs the same gate, so rename it to a general name.

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Rename**

In `src-tauri/src/lib.rs`, rename the `MEMBOOT_ACTIVE` static and all its uses to `DEVICE_BUSY`:

```rust
/// Set while a privileged device operation (memboot, shell) holds the USB
/// interface, so the background poll thread does not contend for it.
static DEVICE_BUSY: AtomicBool = AtomicBool::new(false);
```

Update the three references (the poll-thread guard `if !DEVICE_BUSY.load(...)`, and the two `store(...)` calls in the memboot command).

- [ ] **Step 2: Verify**

Run: `cd src-tauri && cargo build` → succeeds.
Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings` → clean.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "refactor(core): rename memboot poll-gate to DEVICE_BUSY"
```

---

## Task 5: `open_shell_and_run` command + events

Chain memboot-with-clovershell-cmdline → poll for clovershell ping → exec → emit result.

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the command and helpers**

In `src-tauri/src/lib.rs`, add the imports `use crate::device::bootimg; use crate::device::clovershell::{self, ClovershellTransport};` (merge with existing `use crate::device::...` lines), then add:

```rust
const SHELL_PROGRESS_EVENT: &str = "shell-progress";

#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
enum ShellProgress {
    FetchingPayload,
    Membooting,
    WaitingForShell,
    Running,
    Done { stdout: String, exit_code: i32 },
    Failed { message: String },
}

fn emit_shell(app: &AppHandle, p: ShellProgress) {
    let _ = app.emit(SHELL_PROGRESS_EVENT, p);
}

/// Memboot with the clovershell cmdline, then connect + ping + exec the command.
fn run_shell(app: &AppHandle, cache_dir: std::path::PathBuf, command: &str) -> Result<(), RfError> {
    emit_shell(app, ShellProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let fes1 = blobs::fes1();
    let uboot = to_sd_uboot(&blobs::uboot(&cache_dir)?);
    let boot_img = bootimg::inject_cmdline(&blobs::boot_img(&cache_dir)?, "hakchi-clovershell")?;

    emit_shell(app, ShellProgress::Membooting);
    {
        let mut transport = open_fel()?;
        memboot_ops::init_dram(&mut transport, fes1)?;
        thread::sleep(Duration::from_millis(2000));
        let padded =
            boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
        if padded as u32 > fel::TRANSFER_MAX_SIZE {
            return Err(RfError::ExecFailed("boot image too large".into()));
        }
        let mut kernel = boot_img.clone();
        kernel.resize(padded, 0);
        memboot_ops::write_memory(&mut transport, fel::TRANSFER_BASE, &kernel)?;
        let cmd = format!("boota {:x}", fel::TRANSFER_BASE);
        memboot_ops::run_uboot_cmd(&mut transport, &uboot, &cmd)?;
    }

    emit_shell(app, ShellProgress::WaitingForShell);
    let mut transport = wait_for_clovershell(Duration::from_secs(30))?;

    emit_shell(app, ShellProgress::Running);
    let out = clovershell::exec(&mut transport, command)?;
    emit_shell(
        app,
        ShellProgress::Done {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            exit_code: out.exit_code,
        },
    );
    Ok(())
}

/// Poll until a clovershell device answers ping, or the deadline passes.
fn wait_for_clovershell(within: Duration) -> Result<ClovershellTransport, RfError> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Ok(mut t) = ClovershellTransport::open() {
            if clovershell::ping(&mut t).is_ok() {
                return Ok(t);
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(RfError::ShellNotFound)
}

#[tauri::command]
fn open_shell_and_run(app: AppHandle, command: String) {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    std::thread::spawn(move || {
        DEVICE_BUSY.store(true, Ordering::SeqCst);
        let result = run_shell(&app, cache_dir, &command);
        DEVICE_BUSY.store(false, Ordering::SeqCst);
        if let Err(e) = result {
            emit_shell(
                &app,
                ShellProgress::Failed {
                    message: e.to_string(),
                },
            );
        }
    });
}
```

Register `open_shell_and_run` in the existing `generate_handler!` list (alongside `get_device_status, memboot`).

- [ ] **Step 2: Verify**

Run: `cd src-tauri && cargo build` → succeeds. Remove any now-obsolete `#[allow(dead_code)]` on `bootimg::inject_cmdline`, `clovershell::{ping, exec, ClovershellTransport}` (now consumed) — then:
Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings` → clean (re-add a narrow allow only if something is still genuinely unused).
Run: `cd src-tauri && cargo test` → all prior tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(core): add open_shell_and_run command (memboot + clovershell exec)"
```

---

## Task 6: Frontend — shell runner

**Files:**
- Create: `src/lib/shell.ts`, `src/components/ShellRunner.tsx`, `src/components/ShellRunner.test.tsx`
- Modify: `src/App.tsx`

- [ ] **Step 1: Typed wrapper**

Create `src/lib/shell.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ShellProgress =
  | { phase: "fetchingPayload" }
  | { phase: "membooting" }
  | { phase: "waitingForShell" }
  | { phase: "running" }
  | { phase: "done"; stdout: string; exitCode: number }
  | { phase: "failed"; message: string };

export function openShellAndRun(command: string): Promise<void> {
  return invoke<void>("open_shell_and_run", { command });
}

export function onShellProgress(
  handler: (p: ShellProgress) => void,
): Promise<UnlistenFn> {
  return listen<ShellProgress>("shell-progress", (e) => handler(e.payload));
}
```

- [ ] **Step 2: Failing test**

Create `src/components/ShellRunner.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { ShellRunner } from "./ShellRunner";

describe("ShellRunner", () => {
  it("disabled when not connected", () => {
    render(<ShellRunner connected={false} />);
    expect(screen.getByRole("button", { name: /run.*uname/i })).toBeDisabled();
  });

  it("shows stdout when done", () => {
    render(
      <ShellRunner
        connected={true}
        initialProgress={{ phase: "done", stdout: "Linux clover 4.4.0", exitCode: 0 }}
      />,
    );
    expect(screen.getByText(/Linux clover 4\.4\.0/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run, verify it fails**

Run: `npm test`
Expected: FAIL — cannot resolve `./ShellRunner`.

- [ ] **Step 4: Implement**

Create `src/components/ShellRunner.tsx`:

```tsx
import { useEffect, useState } from "react";
import { onShellProgress, openShellAndRun, type ShellProgress } from "@/lib/shell";

function labelFor(phase: ShellProgress["phase"]): string {
  switch (phase) {
    case "fetchingPayload":
      return "Fetching boot payload…";
    case "membooting":
      return "Membooting into shell…";
    case "waitingForShell":
      return "Waiting for shell…";
    case "running":
      return "Running…";
    default:
      return "";
  }
}

export function ShellRunner({
  connected,
  initialProgress,
}: {
  connected: boolean;
  initialProgress?: ShellProgress;
}) {
  const [progress, setProgress] = useState<ShellProgress | undefined>(initialProgress);

  useEffect(() => {
    const unlisten = onShellProgress(setProgress);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const running =
    progress !== undefined && progress.phase !== "done" && progress.phase !== "failed";

  return (
    <div className="mt-4 flex flex-col gap-2">
      <button
        type="button"
        disabled={!connected || running}
        onClick={() => {
          setProgress({ phase: "fetchingPayload" });
          openShellAndRun("uname -a").catch(() =>
            setProgress({ phase: "failed", message: "invoke failed" }),
          );
        }}
        className="rounded-md border px-4 py-2 text-sm font-medium disabled:opacity-50"
      >
        Run `uname -a` on device
      </button>
      {progress?.phase === "done" && (
        <pre className="rounded bg-gray-100 p-2 text-xs">
          {progress.stdout}
          {"\n"}exit {progress.exitCode}
        </pre>
      )}
      {progress?.phase === "failed" && (
        <p className="text-sm text-red-600">Failed: {progress.message}</p>
      )}
      {progress && progress.phase !== "done" && progress.phase !== "failed" && (
        <p className="text-sm text-gray-600">{labelFor(progress.phase)}</p>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Run, verify pass**

Run: `npm test`
Expected: all pass.

- [ ] **Step 6: Wire into `App.tsx`**

In `src/App.tsx`, render below the MembootButton:

```tsx
import { ShellRunner } from "@/components/ShellRunner";
// after <MembootButton ... /> :
        <ShellRunner connected={status.state === "connected"} />
```

- [ ] **Step 7: Verify**

Run: `npm test` → pass.
Run: `npx tsc --noEmit` → clean.
Run: `npm run build` → succeeds.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(ui): add device shell runner (uname -a)"
```

---

## Task 7: Hardware acceptance

**Files:**
- Create: `docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md`

- [ ] **Step 1: Write the checklist**

Create `docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md`:

```markdown
# Hardware Acceptance — Clovershell Layer (slice 3)

Verifies the device shell against a real NES Classic Mini. RAM-only memboot; no
brick path. Recover by unplug → hold RESET → replug into FEL.

## Prepare
1. Device in FEL (hold RESET, plug USB, release), WinUSB bound, panel green
   "Connected — Allwinner R16".
2. Internet available (first run may fetch hakchi-latest.hmod).

## Run
3. `npm run tauri dev`.
4. Click "Run `uname -a` on device". Watch the phases:
   Fetching payload → Membooting into shell → Waiting for shell → Running → Done.
5. PASS criteria:
   - The output box shows a real kernel string, e.g. `Linux clover 4.4.0+ ... armv7l`, and
   - exit 0.

## If it fails
- "device shell not found" → the clovershell cmdline did not bring up the shell, or the
  device booted the menu instead. Confirm `hakchi-clovershell` was injected and that the
  memboot'd image supports clovershell. Check the dev console for the failure.
- Timeout during memboot → see the slice-2 memboot notes (DRAM settle, 4 KB chunks, SD uboot).
- After the run the FEL status panel may show grey/amber (the device is in shell mode, not
  FEL) — expected. Unplug → hold RESET → replug to return to FEL.

## Capture
6. Record PASS/FAIL + the actual `uname -a` string and any protocol corrections.
```

- [ ] **Step 2: Commit**

```bash
git add docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md
git commit -m "docs: add clovershell hardware acceptance checklist"
```

---

## Done criteria

- `cargo test`, `cargo clippy -D warnings`, `npm test`, `tsc --noEmit` all pass.
- Hardware acceptance: clicking the button memboots into clovershell and shows the real
  `uname -a` output with exit 0.
