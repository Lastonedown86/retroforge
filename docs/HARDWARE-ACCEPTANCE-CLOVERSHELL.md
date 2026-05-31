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
