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

1. Device Manager -> find the `04E8:6863` device.
2. Update Driver -> "Let me pick" -> **Remote NDIS Compatible Device** (or
   "Remote NDIS based Internet Sharing Device").
3. Remove/disable the Samsung USB driver if it keeps re-binding.
4. Confirm a NIC appears with a `169.254.x` APIPA address.
5. Clean up stale RNDIS adapter instances (#1/#2/#3) left by repeated memboots.

Verify reachability:
- `ping 169.254.13.37`
- `ssh root@169.254.13.37` (empty password) -> shell prompt.

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
