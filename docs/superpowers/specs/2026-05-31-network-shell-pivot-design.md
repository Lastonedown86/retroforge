# Network-Shell Pivot — Slice 3 Redo

**Date:** 2026-05-31
**Branch:** `slice/clovershell-layer` (continues; rename at merge optional)
**Supersedes:** `2026-05-31-clovershell-layer-design.md` (clovershell transport)

## Background

Slice 3 built a clovershell USB bulk transport to run commands on the device
after FEL memboot. Hardware acceptance (session 3, 2026-05-31) proved the
transport is **dead against the current image**: `hakchi-latest.hmod`'s
`boot.img` hard-disables clovershell in preinit (`cf_clovershell='n'`,
unconditional), so no cmdline can enable it. The device instead comes up as an
**RNDIS USB-ethernet gadget** running dropbear/telnet.

The FEL/memboot layer is correct and bug-fixed (`exec` now drains the 8-byte
`FEL_RUN` status — committed `7040665`). Only the comms transport must change.

User chose **option A: network shell**. This spec covers replacing the
clovershell transport with SSH over USB-ethernet while preserving the existing
UI and command contract.

## Verified facts (from cached ramdisk)

Extracted from `hakchi-latest.hmod` → `boot/boot.img` → XZ cpio ramdisk:

- Device self-assigns **`169.254.13.37/16`** on `rndis0` (RNDIS gadget
  `idVendor 04e8 / idProduct 6863`, borrows Samsung's RNDIS VID/PID for Windows
  auto-bind).
- `/etc/inetd.conf` exposes: `21 ftpd`, `22 dropbear`, `23 telnetd`, `80 ttyd`.
- dropbear launched via inetd as `dropbear -iRB` (`-B` = allow blank-password
  logins).
- `/etc/passwd`: `root:x:0:0:root:/root:/bin/sh`.
- `/etc/shadow`: `root::...` — **root has an empty password.**

So SSH auth = user `root`, **empty password**, password method. No keys, no
real secret.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Transport | **SSH (dropbear, :22)** | Exec channel gives clean stdout/stderr/exit-code — exact match for the existing `ExecOutput` shape. |
| SSH crate | **russh** (pure-Rust, async) + tokio | Pure-Rust crypto builds clean on Windows (no openssl/cmake). `ssh2`/libssh2 would need vendored openssl. |
| Scope | **Parity: one-shot exec** | Replace transport behind the existing device-shell UI. No interactive/streaming session. |
| Old clovershell code | **Delete** | Dead against current image. Git history preserves it. |
| `bootimg::inject_cmdline` | **Delete** | Pivot memboots the plain image (no token injection). Its only consumer is gone. |
| stderr in UI | **stdout-only (parity)** | Matches current `Done` payload; YAGNI. |
| Host NIC driver bind | **Out of code scope** | Manual, analogous to the FEL WinUSB/Zadig step. Documented only. |

## Architecture

### What does NOT change
- FEL/memboot primitives (`device/memboot.rs`, `device/fel.rs`, `device/blobs.rs`).
- Frontend `src/lib/shell.ts` — `openShellAndRun(command)`, `onShellProgress`.
- The `open_shell_and_run` Tauri command signature.
- The `ShellProgress` event variants (`fetchingPayload`, `membooting`,
  `waitingForShell`, `running`, `done`, `failed`). Phase names kept to avoid
  frontend churn; `waitingForShell` now means "waiting for network shell".

### Boot path (simplified)
memboot the **plain** `boot/boot.img` — **no cmdline injection**. The default
image brings up RNDIS unconditionally (`S92rndis`, unless clovershell active or
usb-host mode — neither applies). The device is then reachable at
`169.254.13.37:22`.

### New module: `device/netshell.rs`
The comms layer. Hardware-touching but small.

```
pub const DEVICE_IP: &str = "169.254.13.37";
pub const SSH_PORT: u16 = 22;

pub struct ExecOutput { pub stdout: Vec<u8>, pub stderr: Vec<u8>, pub exit_code: i32 }

/// Poll TCP-connect to host:22 until reachable or `within` elapses.
pub fn wait_for_ssh(host: &str, within: Duration) -> Result<(), RfError>;

/// Connect, password-auth root/"", exec one command, collect output.
pub fn run_ssh_command(host: &str, user: &str, command: &str) -> Result<ExecOutput, RfError>;
```

`run_ssh_command` internals:
- Build a current-thread tokio runtime, `block_on` the async body (bridges the
  sync worker thread to russh's async API).
- russh client `connect`, `Handler::check_server_key → Ok(true)` (link-local
  device, nothing to pin).
- `authenticate_password(user, "")`; non-success → `RfError::SshError`.
- `channel_open_session`, `channel.exec(true, command)`.
- Drain `ChannelMsg`: `Data → stdout`, `ExtendedData{ext:1} → stderr`,
  `ExitStatus → exit_code`, `Eof`/`Close` → finish.

### `lib.rs::run_shell` rewrite
1. `FetchingPayload` — ensure blobs; fes1, SD-uboot, **plain** boot_img.
2. `Membooting` — open_fel → init_dram → 5s DRAM settle → write image @
   `TRANSFER_BASE` → `boota`. Drop the FEL transport.
3. Poll `fel_present()` until it leaves the bus (≤15s) else `MembootTimeout`
   *(image never booted: corrupt upload / failed boota)*.
4. `WaitingForShell` — `netshell::wait_for_ssh(DEVICE_IP, 30s)` else
   `ShellNotFound` *(booted but no network/SSH — typically the host RNDIS NIC
   driver is not bound)*.
5. `Running` — `netshell::run_ssh_command(DEVICE_IP, "root", command)`.
6. `Done { stdout, exit_code }`.

Two-phase failure localization (FEL-stuck vs shell-unreachable) is preserved,
now via `fel_present()` + TCP-poll instead of the `0x82`/`0x81` endpoint watch.

## Deletions

- `src-tauri/src/device/clovershell.rs` — whole file; remove `pub mod
  clovershell` from `device/mod.rs`.
- `src-tauri/src/device/bootimg.rs` — whole file (only held `inject_cmdline`);
  remove `pub mod bootimg`. **Includes the ignored
  `inject_real_boot_img_is_byte_correct` test.**
- `lib.rs`: `dev_bulk_in_ep`, `wait_for_clovershell`, clovershell/bootimg
  imports, `CLV_EP_IN` usage.
- `error.rs`: remove `ClovershellProtocol`, `CmdlineTooLong`. Keep
  `ShellNotFound` (reword message → "device shell unreachable over network").
  Add `SshError(String)`.

## New dependencies

```toml
russh = "0.54"   # pin to the current stable at implementation time
tokio = { version = "1", features = ["rt", "macros", "io-util", "net", "time"] }
```

Verify the russh client API (`ChannelMsg` variants, `authenticate_password`
signature) against the pinned version during implementation — russh's API has
shifted across minor releases.

## Error model

| Error | Meaning |
|---|---|
| `MembootTimeout` | FEL device never left the bus after boota — image didn't boot. |
| `ShellNotFound` (reworded) | Booted, but `169.254.13.37:22` unreachable within 30s — host NIC unbound, RNDIS down, or SSH not up. |
| `SshError(String)` | Connected to :22 but auth/channel/exec failed. |

## Testing

### CI-safe (no hardware)
- `wait_for_ssh` unit tests: against a local `TcpListener` — open port returns
  `Ok` quickly; a dead port returns `ShellNotFound` by the deadline.
- `run_ssh_command` integration test: **stand up an in-process russh server**
  on `127.0.0.1` that accepts `root`/`""` and answers an `exec` request with
  canned stdout + exit code. Validates the client's collect loop
  (stdout/stderr/exit) end-to-end without hardware. Replaces the clovershell
  mock coverage being deleted.

### Hardware acceptance (manual)
- Update `docs/HARDWARE-ACCEPTANCE-CLOVERSHELL.md` → network shell:
  - Document the host RNDIS NIC bind (Device Manager → bind generic "Remote
    NDIS Compatible Device" to `04E8:6863`; remove/avoid Samsung USB driver
    hijack; clean up stale RNDIS adapter instances).
  - Acceptance: launch UI shell runner, run `uname -a`, expect Linux/clover
    stdout + exit 0.

## Out of scope

- Host RNDIS NIC driver binding (manual host setup).
- Interactive / streaming / multi-command sessions.
- FTP (NAND backup) over `:21`, ttyd over `:80`.
- Renaming the branch / clovershell-named docs (cosmetic; defer to merge).
