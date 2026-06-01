# Network-Shell Pivot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the dead clovershell USB transport with SSH-over-USB-ethernet so the device shell runner works against the current hakchi image.

**Architecture:** memboot the plain `boot.img` (brings up the RNDIS gadget at `169.254.13.37`), then SSH (russh, `root`/empty-password) to run one command and collect stdout/stderr/exit. Frontend and the `open_shell_and_run` contract are unchanged — backend transport swap only.

**Tech Stack:** Rust, Tauri 2, rusb (FEL, unchanged), russh + tokio (new, SSH).

**Spec:** `docs/superpowers/specs/2026-05-31-network-shell-pivot-design.md`

---

## File Structure

- `src-tauri/Cargo.toml` — add `russh`, `tokio` deps.
- `src-tauri/src/device/netshell.rs` — **new.** SSH transport: `ExecOutput`,
  `ShellEvent`, `fold_events` (pure, tested), `wait_for_ssh` (tested),
  `run_ssh_command` (russh glue).
- `src-tauri/src/device/mod.rs` — register `netshell`, drop `clovershell` +
  `bootimg`.
- `src-tauri/src/lib.rs` — rewrite `run_shell`; delete `dev_bulk_in_ep`,
  `wait_for_clovershell`, clovershell/bootimg wiring.
- `src-tauri/src/device/clovershell.rs` — **delete.**
- `src-tauri/src/device/bootimg.rs` — **delete.**
- `src-tauri/src/error.rs` — drop `ClovershellProtocol`, `CmdlineTooLong`;
  reword `ShellNotFound`; add `SshError`.
- `docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md` — rewrite for network shell.

**Testing-strategy note (deviation from spec):** the spec floated an
in-process russh *server* test. russh's server API shifts across releases and is
brittle to author blind. Instead, the channel-accumulation logic is extracted
into a pure `fold_events(impl IntoIterator<Item = ShellEvent>)` that is unit
tested with a russh-free type; `wait_for_ssh` is unit tested against a local
`TcpListener`; the thin russh glue in `run_ssh_command` is covered by an
`#[ignore]` hardware test + manual UI acceptance. This yields robust CI coverage
without coupling tests to russh internals.

---

## Task 1: Add russh + tokio dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml:20-30`

- [ ] **Step 1: Add the dependencies**

Append to the `[dependencies]` table in `src-tauri/Cargo.toml`:

```toml
russh = "0.54"
tokio = { version = "1", features = ["rt", "macros", "io-util", "net", "time"] }
```

> If `russh = "0.54"` does not resolve, run `cargo search russh` and pin the
> current stable. The client API used in Task 4 (`authenticate_password`,
> `channel_open_session`, `ChannelMsg`) must be reconciled against the pinned
> version — see russh's `examples/client_exec_*.rs`.

- [ ] **Step 2: Verify it builds**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compiles (downloads russh/tokio). Warnings OK; no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build: add russh + tokio for network shell"
```

---

## Task 2: netshell module — ExecOutput, ShellEvent, fold_events

**Files:**
- Create: `src-tauri/src/device/netshell.rs`
- Modify: `src-tauri/src/device/mod.rs:1-7`

- [ ] **Step 1: Register the module**

In `src-tauri/src/device/mod.rs`, add to the module list (keep alphabetical
ordering near the others):

```rust
pub mod netshell;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/device/netshell.rs` with only the test module and a
not-yet-existing API referenced (it will fail to compile = the red state):

```rust
use crate::error::RfError;
use std::time::Duration;

pub const DEVICE_IP: &str = "169.254.13.37";
pub const SSH_PORT: u16 = 22;

/// Result of running one command on the device.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

/// One unit of SSH channel output, decoupled from russh types so the
/// accumulation logic is testable without standing up an SSH server.
#[derive(Debug, Clone, PartialEq)]
pub enum ShellEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_accumulates_stdout_chunks_and_exit() {
        let out = fold_events([
            ShellEvent::Stdout(b"Linux ".to_vec()),
            ShellEvent::Stdout(b"clover 4.4.0".to_vec()),
            ShellEvent::Exit(0),
        ]);
        assert_eq!(out.stdout, b"Linux clover 4.4.0");
        assert_eq!(out.stderr, b"");
        assert_eq!(out.exit_code, 0);
    }

    #[test]
    fn fold_captures_stderr_and_nonzero_exit() {
        let out = fold_events([
            ShellEvent::Stderr(b"nope".to_vec()),
            ShellEvent::Exit(1),
        ]);
        assert_eq!(out.stderr, b"nope");
        assert_eq!(out.exit_code, 1);
    }

    #[test]
    fn fold_defaults_exit_to_minus_one_when_absent() {
        let out = fold_events([ShellEvent::Stdout(b"x".to_vec())]);
        assert_eq!(out.exit_code, -1);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fold_ -- --nocapture`
Expected: FAIL to compile — `cannot find function fold_events`.

- [ ] **Step 4: Implement fold_events**

Add above the `#[cfg(test)]` module in `netshell.rs`:

```rust
/// Accumulate channel events into one ExecOutput. A run with no Exit event
/// reports -1 (mirrors the old clovershell default).
pub fn fold_events(events: impl IntoIterator<Item = ShellEvent>) -> ExecOutput {
    let mut out = ExecOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: -1,
    };
    for e in events {
        match e {
            ShellEvent::Stdout(d) => out.stdout.extend_from_slice(&d),
            ShellEvent::Stderr(d) => out.stderr.extend_from_slice(&d),
            ShellEvent::Exit(c) => out.exit_code = c,
        }
    }
    out
}
```

> The `use crate::error::RfError;` and `use std::time::Duration;` at the top are
> not used yet — they are consumed in Tasks 3 and 4. Add
> `#[allow(unused_imports)]` above them if `cargo test` is run with `-D
> warnings` between tasks; remove the allow in Task 4.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fold_ -- --nocapture`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/device/netshell.rs src-tauri/src/device/mod.rs
git commit -m "feat(netshell): ExecOutput + fold_events accumulation"
```

---

## Task 3: wait_for_ssh — TCP reachability poll

**Files:**
- Modify: `src-tauri/src/device/netshell.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `netshell.rs`:

```rust
    use std::net::TcpListener;

    #[test]
    fn wait_for_ssh_ok_when_port_open() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // A listening socket accepts the connect via the backlog without an
        // explicit accept(), so no background thread is needed.
        let r = wait_for_ssh_addr("127.0.0.1", port, Duration::from_secs(2));
        assert!(r.is_ok(), "expected Ok, got {r:?}");
    }

    #[test]
    fn wait_for_ssh_errs_when_port_closed() {
        // Bind then drop to obtain a port nothing listens on.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let r = wait_for_ssh_addr("127.0.0.1", port, Duration::from_millis(300));
        assert!(matches!(r, Err(RfError::ShellNotFound)), "got {r:?}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml wait_for_ssh -- --nocapture`
Expected: FAIL to compile — `cannot find function wait_for_ssh_addr`.

- [ ] **Step 3: Implement wait_for_ssh_addr + wait_for_ssh**

Add to `netshell.rs` (above the test module). Add the needed imports to the
existing top-of-file `use` lines: `use std::net::{SocketAddr, TcpStream};` and
`use std::time::Instant;`.

```rust
/// Poll a TCP connect to `host:port` until it succeeds or `within` elapses.
/// Split out so tests can target an arbitrary port; production calls
/// `wait_for_ssh` with `SSH_PORT`.
pub fn wait_for_ssh_addr(host: &str, port: u16, within: Duration) -> Result<(), RfError> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| RfError::SshError(e.to_string()))?;
    let deadline = Instant::now() + within;
    loop {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(RfError::ShellNotFound);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Wait until the device's SSH port is reachable, or give up after `within`.
pub fn wait_for_ssh(host: &str, within: Duration) -> Result<(), RfError> {
    wait_for_ssh_addr(host, SSH_PORT, within)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml wait_for_ssh -- --nocapture`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/device/netshell.rs
git commit -m "feat(netshell): wait_for_ssh TCP reachability poll"
```

---

## Task 4: run_ssh_command — russh client glue

**Files:**
- Modify: `src-tauri/src/device/netshell.rs`

No CI unit test — this is the hardware/russh boundary. It gets an `#[ignore]`
hardware test and manual acceptance (Task 8). The testable logic it relies on
(`fold_events`) is already covered.

- [ ] **Step 1: Implement the russh client + run_ssh_command**

Add to `netshell.rs`. Remove any `#[allow(unused_imports)]` added in Task 2.

```rust
use std::sync::Arc;

/// Accept any server key — the device is a fixed link-local peer over a private
/// USB-ethernet link; there is no PKI to validate against.
struct AcceptAnyKey;

impl russh::client::Handler for AcceptAnyKey {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Connect to `host:22`, authenticate as `user` with an empty password
/// (dropbear runs `-B`, root has no password), run one command, and collect
/// stdout/stderr/exit. Bridges the sync worker thread to russh's async API via
/// a current-thread tokio runtime.
pub fn run_ssh_command(host: &str, user: &str, command: &str) -> Result<ExecOutput, RfError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| RfError::SshError(e.to_string()))?;
    rt.block_on(run_ssh_command_async(host, user, command))
}

async fn run_ssh_command_async(
    host: &str,
    user: &str,
    command: &str,
) -> Result<ExecOutput, RfError> {
    let config = Arc::new(russh::client::Config::default());
    let mut session = russh::client::connect(config, (host, SSH_PORT), AcceptAnyKey)
        .await
        .map_err(|e| RfError::SshError(format!("connect: {e}")))?;

    // dropbear `-B` accepts a blank password for root.
    let authed = session
        .authenticate_password(user, "")
        .await
        .map_err(|e| RfError::SshError(format!("auth: {e}")))?;
    if !authed.success() {
        return Err(RfError::SshError("password auth rejected".into()));
    }

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| RfError::SshError(format!("open channel: {e}")))?;
    channel
        .exec(true, command)
        .await
        .map_err(|e| RfError::SshError(format!("exec: {e}")))?;

    let mut events: Vec<ShellEvent> = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { ref data } => {
                events.push(ShellEvent::Stdout(data.to_vec()))
            }
            russh::ChannelMsg::ExtendedData { ref data, ext } if ext == 1 => {
                events.push(ShellEvent::Stderr(data.to_vec()))
            }
            russh::ChannelMsg::ExitStatus { exit_status } => {
                events.push(ShellEvent::Exit(exit_status as i32))
            }
            russh::ChannelMsg::Eof | russh::ChannelMsg::Close => {}
            _ => {}
        }
    }
    Ok(fold_events(events))
}
```

> **Reconcile with the pinned russh version:**
> - `authenticate_password` may return `bool` (older) instead of `AuthResult`.
>   If so, replace the `.success()` block with `if !authed { return Err(...) }`.
> - `check_server_key`'s key type may be `&russh::keys::key::PublicKey` or
>   `&ssh_key::PublicKey` depending on the russh-keys version.
> - If the trait still requires `#[async_trait::async_trait]`, add that crate
>   and annotate the impl. russh ≥0.45 uses native async-fn-in-trait.
> Compile errors here are expected to be small signature fixes; the control
> flow is stable.

- [ ] **Step 2: Add an ignored hardware smoke test**

Add to the `tests` module in `netshell.rs`:

```rust
    // Hardware-only: requires a memboot'd device reachable on the RNDIS link.
    // Run after a successful memboot with the host RNDIS NIC bound:
    //   cargo test --manifest-path src-tauri/Cargo.toml \
    //     run_ssh_command_uname -- --ignored --nocapture
    #[test]
    #[ignore = "needs a memboot'd device on 169.254.13.37"]
    fn run_ssh_command_uname() {
        wait_for_ssh(DEVICE_IP, Duration::from_secs(30)).expect("ssh reachable");
        let out = run_ssh_command(DEVICE_IP, "root", "uname -a").expect("exec");
        eprintln!("stdout: {}", String::from_utf8_lossy(&out.stdout));
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.windows(5).any(|w| w == b"Linux"));
    }
```

- [ ] **Step 3: Build + run the CI test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml` (the ignored HW test is
skipped automatically).
Expected: all prior tests pass; netshell compiles.

- [ ] **Step 4: Clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/device/netshell.rs
git commit -m "feat(netshell): run_ssh_command via russh (root/empty pw)"
```

---

## Task 5: Rewrite run_shell to use the network transport

**Files:**
- Modify: `src-tauri/src/lib.rs:215-286` (`run_shell` + `wait_for_clovershell`)
- Modify: `src-tauri/src/lib.rs:16-21` (imports)

This task swaps the body of `run_shell` only. The old clovershell/bootimg files
still exist after this task (deleted in Task 6) but are no longer referenced.

- [ ] **Step 1: Update imports**

In `src-tauri/src/lib.rs`, replace the clovershell/bootimg imports. Change:

```rust
use crate::device::bootimg;
use crate::device::clovershell::{self, ClovershellTransport};
use crate::device::usb::{is_fel_device, FelTransport, UsbProbe};
use crate::device::{blobs, fel, memboot as memboot_ops};
```

to:

```rust
use crate::device::netshell;
use crate::device::usb::{is_fel_device, FelTransport, UsbProbe};
use crate::device::{blobs, fel, memboot as memboot_ops};
```

- [ ] **Step 2: Rewrite run_shell**

Replace the entire `run_shell` function (currently `src-tauri/src/lib.rs:215`)
with:

```rust
/// Memboot the plain boot image (brings up the RNDIS gadget), then SSH to the
/// device and run the command. The default boot.img starts dropbear on
/// 169.254.13.37:22; no cmdline injection is needed.
fn run_shell(app: &AppHandle, cache_dir: std::path::PathBuf, command: &str) -> Result<(), RfError> {
    emit_shell(app, ShellProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let fes1 = blobs::fes1();
    let uboot = to_sd_uboot(&blobs::uboot(&cache_dir)?);
    let boot_img = blobs::boot_img(&cache_dir)?;

    emit_shell(app, ShellProgress::Membooting);
    {
        let mut transport = open_fel()?;
        memboot_ops::init_dram(&mut transport, fes1)?;
        // DRAM settle after fes1 exec. 5s (vs memboot's 2s) — marginal devices
        // need longer before the first DRAM write lands.
        thread::sleep(Duration::from_millis(5000));
        let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
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
    // Two-phase failure localization:
    //   - FEL never leaves the bus  -> image didn't boot (corrupt upload /
    //     failed boota) -> MembootTimeout, retry on a healthy session.
    //   - FEL left but SSH never reachable -> booted but no network shell
    //     (host RNDIS NIC unbound, RNDIS down) -> ShellNotFound.
    let deadline = Instant::now() + Duration::from_secs(15);
    while fel_present() {
        if Instant::now() >= deadline {
            return Err(RfError::MembootTimeout);
        }
        thread::sleep(Duration::from_millis(500));
    }
    netshell::wait_for_ssh(netshell::DEVICE_IP, Duration::from_secs(30))?;

    emit_shell(app, ShellProgress::Running);
    let out = netshell::run_ssh_command(netshell::DEVICE_IP, "root", command)?;
    emit_shell(
        app,
        ShellProgress::Done {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            exit_code: out.exit_code,
        },
    );
    Ok(())
}
```

- [ ] **Step 3: Delete wait_for_clovershell**

Delete the entire `wait_for_clovershell` function
(`src-tauri/src/lib.rs:274-286`, the `/// Poll until a clovershell device...`
fn). It is no longer called.

- [ ] **Step 4: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compiles. `dev_bulk_in_ep` and the `clovershell`/`bootimg` modules are
now unused — warnings are acceptable at this checkpoint (cleaned in Task 6). If
the build is configured to deny warnings, proceed directly to Task 6 before
committing; otherwise commit now.

- [ ] **Step 5: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(core): run_shell over SSH/RNDIS instead of clovershell"
```

---

## Task 6: Delete clovershell + bootimg modules and dead helpers

**Files:**
- Delete: `src-tauri/src/device/clovershell.rs`
- Delete: `src-tauri/src/device/bootimg.rs`
- Modify: `src-tauri/src/device/mod.rs:1-7`
- Modify: `src-tauri/src/lib.rs` (`dev_bulk_in_ep`, residual imports)

- [ ] **Step 1: Delete the module files**

```bash
git rm src-tauri/src/device/clovershell.rs src-tauri/src/device/bootimg.rs
```

- [ ] **Step 2: Remove module registrations**

In `src-tauri/src/device/mod.rs`, delete these two lines:

```rust
pub mod bootimg;
pub mod clovershell;
```

- [ ] **Step 3: Delete dev_bulk_in_ep**

In `src-tauri/src/lib.rs`, delete the entire `dev_bulk_in_ep` function (the
`/// Bulk-IN endpoint of the 1F3A:EFE8 device...` fn, currently around
`src-tauri/src/lib.rs:61-93`).

- [ ] **Step 4: Remove now-unused imports**

In `src-tauri/src/lib.rs`, the `use rusb::{Direction, TransferType, UsbContext};`
line was used only by `dev_bulk_in_ep`. Check remaining usages:

Run: `grep -nE "Direction|TransferType|UsbContext" src-tauri/src/lib.rs`
Expected: no matches outside the `use` line. If so, delete that `use rusb::...`
line. (`rusb` is still used transitively via `device::usb`; only this direct
import goes.)

- [ ] **Step 5: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compiles. It will still reference `RfError::ClovershellProtocol` /
`CmdlineTooLong`? No — those were only used by the deleted files. Build clean.

- [ ] **Step 6: Clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: clean (no dead-code/unused-import warnings).

- [ ] **Step 7: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all pass (clovershell/bootimg tests are gone with their files).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(core): delete dead clovershell + bootimg transports"
```

---

## Task 7: Clean error variants

**Files:**
- Modify: `src-tauri/src/error.rs:28-33`

- [ ] **Step 1: Edit the error enum**

In `src-tauri/src/error.rs`, delete these variants:

```rust
    #[error("kernel cmdline has no room for the injected token")]
    CmdlineTooLong,
    #[error("clovershell protocol error: {0}")]
    ClovershellProtocol(String),
```

Reword `ShellNotFound` and add `SshError`:

```rust
    #[error("device shell unreachable over the network")]
    ShellNotFound,
    #[error("SSH error: {0}")]
    SshError(String),
```

- [ ] **Step 2: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compiles (no remaining references to the deleted variants).

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all pass — including `error::tests::serializes_to_tagged_object`
(it uses `UsbClaimFailed`, untouched).

- [ ] **Step 4: Clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/error.rs
git commit -m "refactor(core): drop clovershell error variants, add SshError"
```

---

## Task 8: Hardware acceptance doc

**Files:**
- Modify: `docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md`

- [ ] **Step 1: Rewrite the doc for the network shell**

Replace the contents of `docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md` with a
network-shell acceptance checklist:

```markdown
# Hardware Acceptance — Network Shell

The device shell runner memboots the plain `boot.img`, which brings up the
hakchi RNDIS USB-ethernet gadget. The host then reaches dropbear over that link.

## Device side (automatic)
- After memboot, the device self-assigns `169.254.13.37/16` on `rndis0`.
- dropbear listens on `:22`; `root` has an empty password (`dropbear -B`).

## Host setup (manual, one-time — analogous to the FEL WinUSB/Zadig step)
The gadget advertises `VID_04E8 & PID_6863` (Samsung's RNDIS IDs). On Windows
the Samsung USB driver (`dg_ssudbus`) hijacks it as "SAMSUNG Mobile USB
Composite Device", leaving the RNDIS NIC "Not Present" and the device
unreachable.

1. Device Manager → find the `04E8:6863` device.
2. Update Driver → "Let me pick" → **Remote NDIS Compatible Device** (or
   "Remote NDIS based Internet Sharing Device").
3. Remove/disable the Samsung USB driver if it keeps re-binding.
4. Confirm a NIC appears with a `169.254.x` APIPA address.
5. Clean up stale RNDIS adapter instances (#1/#2/#3) left by repeated memboots.

Verify reachability:
- `ping 169.254.13.37`
- `ssh root@169.254.13.37` (empty password) → shell prompt.

## Acceptance
1. Launch the app, open the device shell runner.
2. Run `uname -a`.
3. Expect Linux/clover kernel string in stdout, exit code 0.

## Recovery
- A timed-out FEL transfer wedges the FEL state machine — physical RESET replug
  to recover (`handle.reset()` does not).
- `MembootTimeout` = image never booted; retry on a healthy USB session.
- `ShellNotFound` = booted but `:22` unreachable; re-check the host RNDIS NIC
  binding above.
```

- [ ] **Step 2: Rename the file to match (optional but tidy)**

```bash
git mv docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md docs/HARDWARE-ACCEPTANCE-NETWORK-SHELL.md
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "docs: network-shell hardware acceptance checklist"
```

---

## Task 9: Final verification

- [ ] **Step 1: Full build + test + clippy**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: build clean, all tests pass, clippy clean.

- [ ] **Step 2: Confirm frontend untouched**

Run: `grep -rn "openShellAndRun\|shell-progress\|ShellProgress" src/`
Expected: `src/lib/shell.ts` matches only — the frontend contract is unchanged.

- [ ] **Step 3: Update the project memory**

After the branch is verified green, the
`slice3-clovershell-pending-hw.md` memory note should be updated (or replaced)
to record that the network-shell pivot is implemented and what remains
(hardware acceptance with the host RNDIS NIC bound). This is a memory-tool
action, not a code commit.

---

## Self-Review

- **Spec coverage:** SSH transport (Task 4) ✓; plain-image memboot, no injection
  (Task 5) ✓; `wait_for_ssh` + two-phase localization (Tasks 3, 5) ✓; delete
  clovershell/bootimg (Task 6) ✓; error model `ShellNotFound`/`SshError` (Task
  7) ✓; deps (Task 1) ✓; CI tests `fold_events`/`wait_for_ssh` (Tasks 2, 3) ✓;
  HW acceptance doc + host NIC bind (Task 8) ✓; stdout-only `Done` parity (Task
  5) ✓; frontend untouched (Task 9) ✓.
- **Deviation:** in-process russh server test replaced by pure `fold_events`
  tests + ignored HW test — documented in File Structure note.
- **Type consistency:** `ExecOutput`, `ShellEvent`, `fold_events`,
  `wait_for_ssh`/`wait_for_ssh_addr`, `run_ssh_command`, `DEVICE_IP`, `SSH_PORT`
  used identically across Tasks 2–5. `RfError::{ShellNotFound, SshError,
  MembootTimeout, ExecFailed}` consistent.
```
