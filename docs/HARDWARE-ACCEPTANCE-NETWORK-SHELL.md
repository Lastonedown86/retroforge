# Hardware Acceptance — Network Shell

The device shell runner memboots the plain `boot.img`, which brings up the
hakchi RNDIS USB-ethernet gadget. The host then reaches dropbear over that link.

## Device side (automatic)
- After memboot, the device self-assigns `169.254.13.37/16` on `rndis0`.
- dropbear listens on `:22`; `root` has an **empty** password (`dropbear -B`).
- **Auth method = SSH `none`** (verified on HW: `Authenticated ... using "none"`).
  The empty root password means the server grants the `none` method outright;
  the `password` method with an empty string is **rejected**. `run_ssh_command`
  tries `authenticate_none` first (empty-password fallback for resilience).
- Kernel string returned: `Linux madmonkey 3.4.113.29-madmonkey ... armv7l`.
- Device does **not** answer ICMP — `ping` fails even when `:22` is open. Probe
  with a TCP test to port 22, not ping.

## Host setup (manual, one-time — analogous to the FEL WinUSB/Zadig step)
The gadget advertises `VID_04E8 & PID_6863` (Samsung's RNDIS IDs). On Windows
the Samsung drivers hijack it: `ssudbus.inf` (oem*, composite) +
`ssudrnds.inf` (oem*, RNDIS net) bind it as "SAMSUNG Mobile USB ...", leaving
the RNDIS NIC **Not Present** with no host IP.

Proven-working sequence on this machine:
1. Remove the Samsung **RNDIS** driver package so the inbox RNDIS driver wins:
   `pnputil /enum-drivers` → find `Original Name: ssudrnds.inf` → its
   `Published Name` (e.g. `oem77.inf`) → `pnputil /delete-driver oem77.inf /uninstall`.
   (Keep `ssudbus.inf` unless the composite keeps re-grabbing — that only
   affects Samsung USB tethering, reversible.)
2. Remove stale `VID_04E8&PID_6863&RNDIS` device nodes:
   `pnputil /remove-device "<InstanceId>"` for each.
3. **Reboot the host.** This was the step that actually mattered: stale
   Not-Present RNDIS netadapter instances (`Ethernet 2`, …) block the live
   miniport from starting. A reboot clears them; the miniport then comes up
   **Up** with a `169.254.x` APIPA. (Driver-switching the live device to
   "Remote NDIS Compatible Device" did **not** persist across re-enumeration on
   this machine — the reboot/ghost-clear is what fixed it.)
4. After reboot, confirm the RNDIS adapter is `Up` with a `169.254.x/16` IP
   (`Get-NetAdapter`, `Get-NetIPAddress`).

Do NOT churn the device nodes mid-session (`remove-device` on the live RNDIS
function) — it tears down the miniport and de-enumerates the child; recovery
needs a fresh memboot.

Verify reachability (PowerShell):
- `Test-NetConnection 169.254.13.37 -Port 22`  → `TcpTestSucceeded : True`
- `ssh -o PreferredAuthentications=none root@169.254.13.37 "uname -a"` → exit 0.
- Or headless: `cargo test run_ssh_command_uname -- --ignored --nocapture`.

## Acceptance
1. Launch the app, open the device shell runner.
2. Run `uname -a`.
3. Expect Linux/clover kernel string in stdout, exit code 0.

**Status (2026-05-31):** SSH transport PASSED on hardware via the headless
`run_ssh_command_uname` ignored test (`uname -a` → exit 0 over the real RNDIS
link). memboot→RNDIS and the `ShellNotFound` localization also verified live.
Full one-shot UI run pending a stable link window.

## Recovery
- A timed-out FEL transfer wedges the FEL state machine — physical RESET replug
  to recover (`handle.reset()` does not).
- `MembootTimeout` = image never booted; retry on a healthy USB session.
- `ShellNotFound` = booted but `:22` unreachable; re-check the host RNDIS NIC
  binding above.
