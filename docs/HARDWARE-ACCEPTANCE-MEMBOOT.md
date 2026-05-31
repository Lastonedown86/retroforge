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
