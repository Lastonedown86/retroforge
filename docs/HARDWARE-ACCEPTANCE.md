# Hardware Acceptance — Skeleton + Comms

Verifies the FEL detection + version handshake against a real Classic Mini.

## Result — 2026-05-31: PASS

Verified against a real NES Classic Mini. Confirmed:
- FEL entry: hold RESET, plug USB, release once it enumerates.
- Device enumerates as `1F3A:EFE8`; an unbound device shows amber
  "driver not bound" (the DetectedNoDriver path works on Windows).
- After binding WinUSB (Zadig, manual for now) the FEL version handshake
  returns `AWUSBFEX` + soc_id `0x00166700` → "Allwinner R16"; panel goes
  steady green.

Three handshake/poll bugs were found and fixed during bring-up (see commit
"fix(core): correct FEL handshake and stabilize device polling"): short
bulk reads (16/32 bytes), a missing trailing FEL status read that desynced
the pipe, and per-poll re-claim causing a green/amber flicker. The unit
test fixture now uses the real captured 32-byte response.

## Prepare
1. Power the console off; connect USB to the PC.
2. Enter FEL mode (hold the device RESET while powering on via USB; see Hakchi2-CE
   reference for the exact button/timing for this console model).
3. In Device Manager, confirm a new USB device with VID `1F3A` PID `EFE8` appears.
4. Run Zadig → select that device → install the **WinUSB** driver.

## Verify
5. `npm run tauri dev`.
6. With the device absent: panel shows grey "No device detected".
7. Plug the device in FEL mode: within ~1s the panel turns green
   "Connected — Allwinner R16 (FEL)".
8. Before binding WinUSB (or with the wrong driver): panel shows amber
   "Device found — driver not bound" with Zadig guidance.
9. Unplug: panel returns to grey within ~1s.

## Capture for regression
10. If the handshake fails or the SoC name is wrong, capture the raw 32-byte version
    response (add a temporary `eprintln!("{:02x?}", version)` in `handshake`), and:
    - correct `soc_name` / `parse_version` from the real bytes, and
    - replace the synthetic fixture in `device::fel::tests` with the captured bytes.
