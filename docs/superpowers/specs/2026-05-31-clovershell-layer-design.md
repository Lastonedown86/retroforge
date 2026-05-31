# Slice 3 — Clovershell Comms Layer

**Status:** Approved (2026-05-31)
**Slice:** Third buildable slice. Establishes a shell connection to the device over USB
(the clovershell protocol) so host code can run commands on the memboot'd system.

**Depends on:** Slice 2 (memboot), which is hardware-verified. Reuses the FEL transport,
the memboot orchestration, and the runtime blob fetch.

## Goal

Bring up a working **clovershell** connection: memboot the device into shell mode and run a
command on it from the host, returning the command's output. The tracer-bullet end-state: a
UI action that memboots the device with the clovershell cmdline, connects, runs `uname -a`,
and shows the real kernel string returned by the device.

This is a foundational comms layer — NAND backup (slice 4) and Sync (later) run their work
through it. This slice deliberately stops at "run a bounded command and read its output".

## Background (from the Hakchi reference)

`../Hakchi2-CE/hakchi_gui/Clovershell/ClovershellConnection.cs` is the reference. Clovershell
is a custom USB bulk protocol the memboot'd Hakchi system exposes. Key facts:

- USB identity `1F3A:EFE8` — the **same VID/PID as FEL**. The device is distinguished by
  protocol/endpoints, not by id: clovershell uses bulk endpoints **IN `0x81` / OUT `0x01`**
  (FEL used IN `0x82`), config 1, interface 0.
- Packet framing: a 4-byte header `[cmd, arg, len_lo, len_hi]` followed by `len` bytes of
  payload, sent/received over the bulk endpoints.
- Command set (`ClovershellCommand`): `CMD_PING=0`, `CMD_PONG=1`, shell commands
  `CMD_SHELL_*` (2–8), and exec commands `CMD_EXEC_NEW_REQ=9`, `CMD_EXEC_NEW_RESP=10`,
  `CMD_EXEC_PID=11`, `CMD_EXEC_STDIN=12`, `CMD_EXEC_STDOUT=13`, `CMD_EXEC_STDERR=14`,
  `CMD_EXEC_RESULT=15`, `CMD_EXEC_KILL=16/17`, stdin flow-control `CMD_EXEC_STDIN_FLOW_STAT=18/19`.
- The device only exposes clovershell when the kernel cmdline contains `hakchi-clovershell`.
  Hakchi injects this by editing the Android boot-image cmdline field (offset 64, 512 bytes)
  before memboot.

## Architecture

```
src-tauri/src/device/
  bootimg.rs     NEW. Pure Android boot-image cmdline patch. Appends a token to the
                 NUL-terminated cmdline string in the 512-byte field at offset 64.
  clovershell.rs NEW. Clovershell USB client. A ClovershellIo trait (write_packet /
                 read_packet) abstracts the byte channel; ping and exec are built on it
                 and unit-tested with a mock. The rusb-backed impl opens 1F3A:EFE8,
                 claims interface 0, uses bulk endpoints 0x81/0x01, and frames packets.
  usb.rs         Small helper to open the device handle with clovershell endpoints.
  fel.rs         Clovershell endpoint constants (CLV_EP_IN 0x81, CLV_EP_OUT 0x01) live
                 alongside the existing FEL endpoint constants.
error.rs         New RfError variants (see Error handling).
lib.rs           New open_shell_and_run command + progress/result events; the MEMBOOT_ACTIVE
                 poll-gate is reused (renamed to cover all privileged device ops).
```

## Components and boundaries

### `bootimg.rs`
`inject_cmdline(boot_img: &[u8], token: &str) -> Result<Vec<u8>, RfError>`. The Android boot
header carries the kernel command line as a NUL-terminated string in a 512-byte field at
offset 64. The function copies the image, locates the existing cmdline (up to the first NUL
within the field), appends `" {token}"` if it fits in the remaining field space, and returns
the modified image. If the token does not fit, it returns `CmdlineTooLong`. If the buffer is
shorter than `64 + 512` or lacks the `ANDROID!` magic, it returns `ExecFailed` with a clear
message. Pure and unit-tested.

### `clovershell.rs`
- **`ClovershellIo` trait** — `write_packet(cmd: u8, arg: u8, data: &[u8]) -> Result<(), RfError>`
  and `read_packet() -> Result<(u8, u8, Vec<u8>), RfError>`. The rusb implementation prepends
  the 4-byte header on write and parses it on read; the mock records writes and replays
  scripted reads for tests.
- **`ping(io)`** — sends `CMD_PING`, expects `CMD_PONG`. Bounded by the read timeout.
- **`exec(io, cmd) -> ExecOutput`** — `ExecOutput { stdout: Vec<u8>, stderr: Vec<u8>,
  exit_code: i32 }`. Sends `CMD_EXEC_NEW_REQ` with the command string, then reads packets,
  accumulating `CMD_EXEC_STDOUT`/`CMD_EXEC_STDERR` payloads (keyed by the exec `arg`) until
  `CMD_EXEC_RESULT` carries the exit code. `CMD_EXEC_NEW_RESP`/`CMD_EXEC_PID` are consumed and
  ignored. This state machine is the testable core. Bounded output only (no stdin streaming or
  flow-control in this slice).

### `clovershell` rusb transport
Opens `1F3A:EFE8`, claims interface 0, discovers/uses bulk endpoints IN `0x81` / OUT `0x01`,
and implements `ClovershellIo`. On connect it drains any stale input. It reuses the small
chunked-write discipline established in slice 2 (the 4 KB bulk-OUT limit applies here too).

### `lib.rs`
`open_shell_and_run(command: String)` command spawns a worker that runs the same inlined
memboot steps as the slice-2 path, except it first applies
`bootimg::inject_cmdline(boot_img, "hakchi-clovershell")` to the staged image. (The slice-2
`memboot.rs` orchestration fn and its tests are unchanged; cmdline injection lives in this
shell flow.) After memboot it polls up to a deadline for the clovershell device to answer
`ping`, then `exec`s the command and emits the result. Progress events mark each phase. The existing memboot poll-gate
(an `AtomicBool`) is reused so the FEL status poll thread does not probe the device during the
shell operation.

## Data flow

Click "Open shell & run" → worker thread:
1. `MembootProgress`-style events: fetching payload → memboot (with clovershell cmdline) → the
   device boots to shell mode and re-presents as `1F3A:EFE8` on endpoints `0x81/0x01`.
2. Poll up to a deadline: open the clovershell device and `ping`; retry until it answers.
3. `exec(command)` → collect stdout/exit.
4. Emit a terminal `ShellResult { stdout, exit_code }` (or `Failed { message }`).

The frontend renders the phase labels and finally the command output text.

## Error handling

Every step returns `Result`; the command converts errors into a `Failed` event. New `RfError`
variants: `CmdlineTooLong`, `ShellNotFound` (clovershell never answered ping within the
deadline), `ClovershellProtocol(String)` (unexpected packet / framing error). `ExecFailed`
already exists and is reused for boot-image/format problems. Ping and reads are time-bounded so
a non-responsive device fails rather than hangs. No panics cross the Tauri boundary. Memboot
remains RAM-only here, so there is no brick path.

## Testing

- **Unit (no hardware):**
  - `bootimg::inject_cmdline`: appends the token after the existing cmdline within the field;
    returns `CmdlineTooLong` when the field is full; rejects a buffer without the `ANDROID!`
    magic or shorter than the header.
  - `clovershell::exec` via a mock `ClovershellIo`: scripted `STDOUT` chunks + `RESULT` produce
    the expected accumulated stdout and exit code; `NEW_RESP`/`PID` packets are skipped; a
    `ping` returns on `PONG`. Packet framing (header encode/decode) is table-tested.
- **Hardware acceptance (manual):** memboot into clovershell, run `uname -a`, confirm the real
  kernel string appears in the UI and the exit code is 0.

## Out of scope (later slices)

- NAND backup/dump (slice 4) — built on `exec` (`sntool sunxi_flash phy_read 0`), needs the
  streaming + stdin flow-control path that this slice does not implement.
- Interactive shell sessions (`CMD_SHELL_*`), stdin streaming, and exec flow-control.
- The SSH / network-shell alternative transport.
- Any menu-vs-shell mode UX beyond the single verification action.

## Open items (resolved during plan/implementation)

- Exact EXEC sub-protocol byte layout: the `CMD_EXEC_NEW_REQ` payload format (command string
  encoding, any leading id/flags), the `CMD_EXEC_RESULT` exit-code encoding, and which of
  `CMD_EXEC_NEW_RESP`/`CMD_EXEC_PID` must be awaited — read from `ClovershellConnection.cs`
  (the `Execute` method and `ExecConnection`) during writing-plans.
- Whether the clovershell device needs `killAll` (kill stale sessions) on connect before exec,
  as the reference does — confirm and include if required.
- Confirm at hardware acceptance that the `hakchi-clovershell` cmdline brings the device up on
  clovershell endpoints `0x81/0x01`, and that a replug/FEL re-entry cleanly cycles back.
