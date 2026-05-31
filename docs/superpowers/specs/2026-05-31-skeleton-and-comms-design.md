# Slice 0+1 — Walking Skeleton + Device Comms

**Status:** Approved (2026-05-31)
**Slice:** First buildable slice of RetroForge. Combines the project walking skeleton
(slice 0) with the first device-comms layer (slice 1).

## Goal

Stand up a Tauri v2 application that compiles and runs on Windows, and that can detect a
Nintendo Classic Mini in FEL mode over USB and complete a FEL version handshake — proving
both the USB layer and the FEL protocol layer work end to end. The visible end-state: the
app shows a live connection status that reaches "Connected — Allwinner R16, FEL vX" when a
device in FEL mode is plugged in.

This slice establishes the stack every later slice builds on. It deliberately stops short
of memboot, NAND access, SSH, and boot-image packing.

## Decisions (locked during brainstorming)

- **Tauri v2** (current stable), Windows-only (per CONTEXT "Platform target" and ADR-0001).
- **Frontend baseline:** Vite + React + TypeScript + Tailwind CSS + shadcn/ui (copy-in
  components, not a runtime dependency). Chosen as a durable foundation that scales to the
  signature home-menu canvas in a later slice.
- **Rust core owns all device communication.** No device logic in the frontend (per CONTEXT
  "The core").
- **Comms depth for this slice:** detect FEL device (USB enumeration + VID/PID match) AND
  issue the FEL version command to read back SoC info. Stops short of memory reads / memboot.
- **WinUSB driver:** manual for now. The user binds WinUSB via Zadig once. If the app sees
  the FEL VID/PID but cannot claim the interface, it shows a first-class
  "driver not bound" state with instructions. Programmatic libwdi auto-install is deferred
  to its own later slice (needs admin elevation + native bundling; would bloat the
  foundation).
- **Hardware available:** a physical device is on hand, so the acceptance gate is real
  hardware verification.
- **License:** GPLv3 for the whole project (per ADR-0004); `LICENSE` committed at repo root
  in this slice.

## Architecture

```
retroforge/
  LICENSE                  GPLv3 (per ADR-0004)
  README.md
  .gitignore
  .github/workflows/ci.yml
  package.json             vite + tauri scripts
  vite.config.ts
  tailwind.config.* / postcss
  src/                     React/TS frontend
    main.tsx
    App.tsx
    components/ui/          shadcn copy-ins
    lib/device.ts          typed invoke wrappers + event listeners + types
  src-tauri/
    Cargo.toml
    tauri.conf.json
    build.rs
    src/
      lib.rs               tauri builder; registers commands + spawns poll thread
      device/mod.rs        Device trait, status types, state machine
      device/usb.rs        rusb enumeration, VID/PID match, poll thread
      device/fel.rs        FEL protocol: version handshake, parse SoC info
      error.rs             RfError, serde Serialize -> frontend
```

The two on-device/host codebases described in the ADRs remain separate; this slice touches
only the host app.

## Components and boundaries

Each unit has one job, a defined interface, and is testable in isolation.

### `device::Device` (trait)
The comms abstraction. Two implementations: the real rusb-backed device and a mock used in
unit tests. Responsibilities: report current device state and perform the FEL version
handshake. Consumers (the poll thread, commands) depend on the trait, not on rusb.

### `device/usb.rs`
Owns the rusb context. Enumerates USB devices and matches the Allwinner FEL identity
(VID/PID `0x1f3a:0xefe8`). Runs a background **poll thread** (~1s interval) because libusb
hotplug is unreliable on Windows; the thread detects state transitions and emits a Tauri
event when the state changes. Attempts to open and claim the device interface; a failed
claim is reported as the "detected but driver not bound" state, not a hard error.

### `device/fel.rs`
Implements the minimal FEL USB bulk protocol needed for a version handshake: send the
Allwinner USB version request, read the response, parse the SoC version structure into a
`SocInfo { soc_id, name, ... }` (e.g. mapped to "Allwinner R16"). The exact request byte
sequence and the response struct layout are sourced from the sunxi-tools / Hakchi2-CE
reference (`../Hakchi2-CE`) and confirmed against the on-hand device at acceptance.

### Frontend status panel
A single card showing the live connection state with a colored indicator. `lib/device.ts`
provides typed wrappers over the Tauri command and event, so React components never call
`invoke` directly with stringly-typed args.

## Data flow and states

Device plugged in → poll thread enumerates → matches FEL VID/PID → attempts to claim the
interface:

- **Disconnected** (grey) — no FEL device present.
- **DetectedNoDriver** (amber) — FEL VID/PID present but interface claim failed (WinUSB not
  bound). Shows Zadig instructions. A first-class state, not an error.
- **Connected** (green) — interface claimed and FEL version handshake succeeded. Shows
  "Allwinner R16, FEL vX".

Command `get_device_status()` provides the initial read on mount. Event
`device-status-changed` provides live updates from the poll thread. The frontend subscribes
on mount and unsubscribes on unmount.

## Error handling

A typed `RfError` enum (e.g. `UsbClaimFailed`, `FelProtocolError`, `DeviceGone`) is serde-
serialized so the frontend receives structured errors, not strings. No code panics across
the Tauri boundary — every command returns `Result`. `DetectedNoDriver` is modeled as a
normal status value carrying guidance, consistent with the manual-driver decision, rather
than as an error.

## Testing (TDD)

- **Rust unit tests:**
  - FEL response parser: feed a captured byte fixture, assert the parsed `SocInfo`.
  - VID/PID matching: pure function, table-tested.
  - State-machine logic: driven through the mock `Device` implementation.
- **Frontend tests:** vitest + React Testing Library — the status panel renders correctly
  for each of the three states.
- **Hardware acceptance (manual — the real gate):** documented FEL-entry steps; plug the
  device in FEL mode; run the app; confirm it reaches green "Connected — Allwinner R16".
  During this step, capture the real FEL response bytes to seed the parser unit-test
  fixture.
- **CI:** `cargo build` / `cargo test` / `clippy` / `fmt`; `npm run build` / `vitest` /
  `tsc --noEmit` / `eslint`. The hardware acceptance test is manual and excluded from CI.

## Out of scope (YAGNI — later slices)

- Memboot, NAND dump/restore, SSH, boot-image packing (slice 2 and beyond).
- Programmatic libwdi WinUSB auto-install (its own later slice).
- Any FEL memory reads beyond the version handshake.
- Home-menu canvas, import wizard, library — the UI in this slice is only the status panel.

## Open risk

The exact FEL version request byte sequence and the R16 SoC id mapping are taken from the
sunxi-tools / Hakchi2-CE reference and must be verified against the on-hand device at the
hardware acceptance step. If the captured bytes differ from the reference, the parser and
the SoC-id mapping are corrected from the real capture (which then becomes the test fixture).
