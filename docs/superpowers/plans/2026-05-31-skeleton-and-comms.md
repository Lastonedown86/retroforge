# Walking Skeleton + Device Comms Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a Tauri v2 (React/TS + Rust) Windows app that detects a Nintendo Classic Mini in FEL mode over USB and completes a FEL version handshake, showing live connection status in the UI.

**Architecture:** Rust core owns all device communication (rusb + a minimal FEL bulk protocol). Pure logic (VID/PID matching, FEL response parsing, status-transition detection) is isolated behind small units and unit-tested; rusb I/O is thin and verified against real hardware. A background poll thread emits Tauri events; the React frontend renders a single status panel from a typed command + event wrapper.

**Tech Stack:** Tauri v2, Rust (rusb, serde, thiserror), Vite + React + TypeScript, Tailwind CSS v3.4 + shadcn/ui, vitest + React Testing Library.

---

## Prerequisites (one-time, before Task 1)

The executing engineer must have, on Windows:
- Rust stable toolchain (`rustup`, `cargo`) + the MSVC build tools.
- Node.js 20+ and npm.
- Tauri v2 prerequisites (WebView2 is present on Windows 11 by default).
- Zadig (for the manual WinUSB driver bind, used only at the hardware acceptance step).

Verify before starting:

```bash
cargo --version
node --version
npm --version
```

---

## File Structure

| Path | Responsibility |
|------|----------------|
| `LICENSE` | GPLv3 text (ADR-0004) |
| `README.md` | Project intro + dev run instructions |
| `.gitignore` | Node + Rust + Tauri ignores |
| `.github/workflows/ci.yml` | cargo + npm checks |
| `package.json`, `vite.config.ts`, `index.html` | Frontend app shell |
| `tailwind.config.js`, `postcss.config.js`, `src/index.css` | Styling baseline |
| `src/main.tsx`, `src/App.tsx` | React entry + root |
| `src/lib/device.ts` | Typed Tauri command + event wrappers, status types |
| `src/components/StatusPanel.tsx` | The status card UI |
| `src-tauri/Cargo.toml`, `tauri.conf.json`, `build.rs` | Rust crate + Tauri config |
| `src-tauri/src/lib.rs` | Tauri builder, command, poll-thread spawn |
| `src-tauri/src/error.rs` | `RfError` typed errors |
| `src-tauri/src/device/mod.rs` | `DeviceStatus`, `SocInfo`, `DeviceProbe`, `Monitor` |
| `src-tauri/src/device/usb.rs` | VID/PID match, rusb FEL transport |
| `src-tauri/src/device/fel.rs` | FEL protocol constants, version parse, handshake |
| `docs/HARDWARE-ACCEPTANCE.md` | Manual verification checklist |

---

## Task 1: Repo hygiene — LICENSE, README, .gitignore

**Files:**
- Create: `LICENSE`, `README.md`, `.gitignore`

- [ ] **Step 1: Add the GPLv3 LICENSE**

Download the canonical text to the repo root:

```bash
curl -L -o LICENSE https://www.gnu.org/licenses/gpl-3.0.txt
```

Verify it starts with "GNU GENERAL PUBLIC LICENSE" and "Version 3":

```bash
head -3 LICENSE
```

- [ ] **Step 2: Create `.gitignore`**

```gitignore
# Node
node_modules/
dist/
.vite/

# Rust / Tauri
src-tauri/target/

# Editor / OS
.DS_Store
*.log
.env
```

- [ ] **Step 3: Create `README.md`**

```markdown
# RetroForge

A modern tool for modding the NES Classic Mini and related Classic-series consoles.
Tauri v2 desktop app (React/TypeScript frontend, Rust core). Windows-only. GPLv3.

See `CONTEXT.md` for domain vocabulary and `docs/adr/` for architectural decisions.

## Development

Prerequisites: Rust stable (MSVC), Node 20+, Tauri v2 prerequisites.

```bash
npm install
npm run tauri dev
```
```

- [ ] **Step 4: Commit**

```bash
git add LICENSE README.md .gitignore
git commit -m "chore: add LICENSE, README, gitignore"
```

---

## Task 2: Scaffold the Tauri v2 app

This task produces a running app shell. There is no unit test; verification is "it runs".

**Files:**
- Create: `package.json`, `vite.config.ts`, `index.html`, `tsconfig.json`, `tsconfig.node.json`, `src/main.tsx`, `src/App.tsx`, `src/vite-env.d.ts`, `src-tauri/*` (Cargo.toml, tauri.conf.json, build.rs, src/main.rs, src/lib.rs)

- [ ] **Step 1: Scaffold with create-tauri-app**

Run from the repo root. Choose: TypeScript/JavaScript → npm → React → TypeScript.

```bash
npm create tauri-app@latest . -- --template react-ts --manager npm
```

If the tool refuses because the directory is non-empty, scaffold into a temp dir and move files in:

```bash
npm create tauri-app@latest rf-tmp -- --template react-ts --manager npm
# then move rf-tmp/* and rf-tmp/.* into the repo root, preserving existing
# LICENSE/README/.gitignore/CONTEXT.md/docs, and delete rf-tmp
```

- [ ] **Step 2: Install dependencies**

```bash
npm install
```

- [ ] **Step 3: Confirm the app builds and runs**

Run: `npm run tauri dev`
Expected: a desktop window opens showing the default Tauri+React template. Close it.

If the window opens, the toolchain is good. Stop the dev process.

- [ ] **Step 4: Confirm the Rust crate is named consistently**

Open `src-tauri/Cargo.toml`. Ensure `[package] name = "retroforge"` and that a `[lib]` section exists with `name = "retroforge_lib"` and `crate-type = ["staticlib", "cdylib", "rlib"]` (the v2 template includes this). Set `[package] version = "0.0.0"`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: scaffold Tauri v2 React/TS app"
```

---

## Task 3: Tailwind v3.4 + shadcn/ui baseline

**Files:**
- Create/Modify: `tailwind.config.js`, `postcss.config.js`, `src/index.css`, `components.json`, `tsconfig.json` (path alias), `vite.config.ts` (path alias), `src/lib/utils.ts`

- [ ] **Step 1: Install Tailwind v3.4 toolchain**

```bash
npm install -D tailwindcss@3.4 postcss autoprefixer
npx tailwindcss init -p
```

- [ ] **Step 2: Configure `tailwind.config.js`**

```js
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: { extend: {} },
  plugins: [],
};
```

- [ ] **Step 3: Replace `src/index.css` with Tailwind directives**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

Ensure `src/main.tsx` imports it: `import "./index.css";`

- [ ] **Step 4: Add the `@/` path alias**

In `tsconfig.json`, add under `compilerOptions`:

```json
"baseUrl": ".",
"paths": { "@/*": ["./src/*"] }
```

In `vite.config.ts`, add the resolve alias:

```ts
import path from "node:path";
// inside defineConfig({ ... })
resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
```

- [ ] **Step 5: Initialize shadcn/ui and add components**

```bash
npx shadcn@latest init -d
npx shadcn@latest add card badge
```

Accept defaults. This creates `components.json`, `src/lib/utils.ts`, and `src/components/ui/card.tsx` + `badge.tsx`.

- [ ] **Step 6: Verify styling works**

Temporarily set `src/App.tsx` body to a Tailwind-styled element:

```tsx
export default function App() {
  return <div className="p-8 text-2xl font-bold text-emerald-600">RetroForge</div>;
}
```

Run: `npm run tauri dev`
Expected: window shows "RetroForge" in bold green. Close it.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: add Tailwind v3.4 + shadcn/ui baseline"
```

---

## Task 4: Rust error type (`RfError`)

**Files:**
- Create: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod error;`), `src-tauri/Cargo.toml`

- [ ] **Step 1: Add dependencies to `src-tauri/Cargo.toml`**

Under `[dependencies]` (serde is already present from the template; add the rest):

```toml
thiserror = "1"
rusb = "0.9"
```

- [ ] **Step 2: Write the failing test**

Create `src-tauri/src/error.rs`:

```rust
use serde::Serialize;
use thiserror::Error;

/// Errors crossing the Tauri boundary. Serialized to a tagged object for the frontend.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum RfError {
    #[error("failed to claim USB interface: {0}")]
    UsbClaimFailed(String),
    #[error("FEL protocol error: {0}")]
    FelProtocolError(String),
    #[error("device disconnected during operation")]
    DeviceGone,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_tagged_object() {
        let e = RfError::UsbClaimFailed("busy".into());
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"kind":"UsbClaimFailed","message":"busy"}"#);
    }
}
```

Add `serde_json = "1"` under `[dev-dependencies]` in `src-tauri/Cargo.toml`.

- [ ] **Step 3: Wire the module**

In `src-tauri/src/lib.rs`, add near the top:

```rust
mod error;
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test error::tests::serializes_to_tagged_object`
Expected: PASS (1 passed).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/error.rs src-tauri/src/lib.rs
git commit -m "feat(core): add typed RfError"
```

---

## Task 5: Device status types + transition Monitor

This task builds the pure status model and the change-detection logic, tested via a mock probe. No USB I/O here.

**Files:**
- Create: `src-tauri/src/device/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod device;`)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/device/mod.rs`:

```rust
use serde::Serialize;

/// SoC identity read from the FEL version handshake.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocInfo {
    pub soc_id: u16,
    pub name: String,
}

/// Live device state reported to the frontend.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum DeviceStatus {
    Disconnected,
    DetectedNoDriver,
    Connected { soc: SocInfo },
}

/// Outcome of a single probe attempt against the USB bus.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    Absent,
    DetectedNoDriver,
    Connected(SocInfo),
}

impl From<ProbeOutcome> for DeviceStatus {
    fn from(o: ProbeOutcome) -> Self {
        match o {
            ProbeOutcome::Absent => DeviceStatus::Disconnected,
            ProbeOutcome::DetectedNoDriver => DeviceStatus::DetectedNoDriver,
            ProbeOutcome::Connected(soc) => DeviceStatus::Connected { soc },
        }
    }
}

/// Probes the bus for a FEL device. Real impl talks to rusb; tests use a mock.
pub trait DeviceProbe {
    fn probe(&self) -> ProbeOutcome;
}

/// Detects status transitions so the poll loop only emits on change.
pub struct Monitor<P: DeviceProbe> {
    probe: P,
    last: Option<DeviceStatus>,
}

impl<P: DeviceProbe> Monitor<P> {
    pub fn new(probe: P) -> Self {
        Self { probe, last: None }
    }

    /// Returns `Some(status)` when the status changed since the previous tick, else `None`.
    pub fn tick(&mut self) -> Option<DeviceStatus> {
        let current: DeviceStatus = self.probe.probe().into();
        if self.last.as_ref() == Some(&current) {
            None
        } else {
            self.last = Some(current.clone());
            Some(current)
        }
    }

    /// The most recently observed status (Disconnected if never ticked).
    pub fn current(&self) -> DeviceStatus {
        self.last.clone().unwrap_or(DeviceStatus::Disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct ScriptedProbe {
        script: Vec<ProbeOutcome>,
        idx: Cell<usize>,
    }
    impl DeviceProbe for ScriptedProbe {
        fn probe(&self) -> ProbeOutcome {
            let i = self.idx.get();
            self.idx.set(i + 1);
            self.script[i].clone()
        }
    }

    #[test]
    fn emits_only_on_transition() {
        let soc = SocInfo { soc_id: 0x1667, name: "Allwinner R16".into() };
        let probe = ScriptedProbe {
            script: vec![
                ProbeOutcome::Absent,
                ProbeOutcome::Connected(soc.clone()),
                ProbeOutcome::Connected(soc.clone()),
                ProbeOutcome::Absent,
            ],
            idx: Cell::new(0),
        };
        let mut m = Monitor::new(probe);
        assert_eq!(m.tick(), Some(DeviceStatus::Disconnected));
        assert_eq!(m.tick(), Some(DeviceStatus::Connected { soc: soc.clone() }));
        assert_eq!(m.tick(), None);
        assert_eq!(m.tick(), Some(DeviceStatus::Disconnected));
    }
}
```

- [ ] **Step 2: Wire the module**

In `src-tauri/src/lib.rs`, add:

```rust
mod device;
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `cd src-tauri && cargo test device::tests::emits_only_on_transition`
Expected: PASS (1 passed).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/device/mod.rs src-tauri/src/lib.rs
git commit -m "feat(core): add device status model and transition Monitor"
```

---

## Task 6: FEL protocol — constants, SoC mapping, version parser

This task isolates the parsing of a 32-byte FEL version response (pure, unit-tested). The wire handshake lives in Task 7.

**Files:**
- Create: `src-tauri/src/device/fel.rs`
- Modify: `src-tauri/src/device/mod.rs` (add `pub mod fel;`)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/device/fel.rs`:

```rust
use crate::device::SocInfo;
use crate::error::RfError;

// FEL USB request wrapper (sunxi-tools "AWUC" envelope).
pub const AW_USB_READ: u16 = 0x11;
pub const AW_USB_WRITE: u16 = 0x12;

// FEL request types.
pub const AW_FEL_VERSION: u32 = 0x001;

// Default bulk endpoints (overridden by descriptor scan when claiming).
pub const FEL_EP_OUT: u8 = 0x01;
pub const FEL_EP_IN: u8 = 0x82;

/// Map an Allwinner SoC id to a human name. R16 (sun8iw5) is the Classic Mini's SoC.
pub fn soc_name(soc_id: u16) -> &'static str {
    match soc_id {
        0x1667 => "Allwinner R16",
        0x1651 => "Allwinner A20",
        0x1689 => "Allwinner A64",
        _ => "Unknown Allwinner SoC",
    }
}

/// Parse the 32-byte `aw_fel_version` response. The soc id is stored shifted left by 8.
pub fn parse_version(buf: &[u8]) -> Result<SocInfo, RfError> {
    if buf.len() < 32 {
        return Err(RfError::FelProtocolError(format!(
            "version response too short: {} bytes",
            buf.len()
        )));
    }
    if &buf[0..8] != b"AWUSBFEX" {
        return Err(RfError::FelProtocolError(
            "missing AWUSBFEX signature".into(),
        ));
    }
    let raw = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let soc_id = ((raw >> 8) & 0xFFFF) as u16;
    Ok(SocInfo {
        soc_id,
        name: soc_name(soc_id).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic but structurally correct version response for R16.
    fn r16_response() -> Vec<u8> {
        let mut b = vec![0u8; 32];
        b[0..8].copy_from_slice(b"AWUSBFEX");
        // soc_id 0x1667 stored as 0x00166700 little-endian at offset 8.
        b[8..12].copy_from_slice(&0x0016_6700u32.to_le_bytes());
        b
    }

    #[test]
    fn parses_r16_soc() {
        let soc = parse_version(&r16_response()).unwrap();
        assert_eq!(soc.soc_id, 0x1667);
        assert_eq!(soc.name, "Allwinner R16");
    }

    #[test]
    fn rejects_bad_signature() {
        let mut b = r16_response();
        b[0] = b'X';
        assert!(parse_version(&b).is_err());
    }

    #[test]
    fn rejects_short_buffer() {
        assert!(parse_version(&[0u8; 8]).is_err());
    }
}
```

- [ ] **Step 2: Wire the module**

In `src-tauri/src/device/mod.rs`, add near the top:

```rust
pub mod fel;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test device::fel::tests`
Expected: PASS (3 passed).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/device/fel.rs src-tauri/src/device/mod.rs
git commit -m "feat(core): add FEL version parser and SoC mapping"
```

---

## Task 7: rusb transport — VID/PID match + real FEL probe

The pure matcher is unit-tested. The rusb I/O is thin, compiled and clippy-clean here, and verified against hardware in Task 11.

**Files:**
- Create: `src-tauri/src/device/usb.rs`
- Modify: `src-tauri/src/device/mod.rs` (add `pub mod usb;`)

- [ ] **Step 1: Write the failing test (pure matcher)**

First wire the module: in `src-tauri/src/device/mod.rs`, add `pub mod usb;`.

Then create `src-tauri/src/device/usb.rs` with only the pure matcher and its test (the rusb imports are added in Step 4 alongside the code that uses them, to keep this step warning-free):

```rust
/// Allwinner FEL USB identity.
pub const FEL_VID: u16 = 0x1f3a;
pub const FEL_PID: u16 = 0xefe8;

/// True when a USB VID/PID pair is the Allwinner FEL device.
pub fn is_fel_device(vid: u16, pid: u16) -> bool {
    vid == FEL_VID && pid == FEL_PID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_fel_identity() {
        assert!(is_fel_device(0x1f3a, 0xefe8));
        assert!(!is_fel_device(0x1f3a, 0x0001));
        assert!(!is_fel_device(0x0000, 0xefe8));
    }
}
```

- [ ] **Step 2: Run the matcher test**

Run: `cd src-tauri && cargo test device::usb::tests::matches_fel_identity`
Expected: PASS (1 passed).

- [ ] **Step 3: Commit the pure matcher**

```bash
git add src-tauri/src/device/usb.rs src-tauri/src/device/mod.rs
git commit -m "feat(core): add FEL VID/PID matcher"
```

- [ ] **Step 4: Add the rusb-backed probe (no unit test; hardware-verified later)**

First add the imports the probe needs to the **top** of `src-tauri/src/device/usb.rs`, above the `FEL_VID` constant:

```rust
use std::time::Duration;

use rusb::{Direction, TransferType, UsbContext};

use crate::device::fel::{self, AW_FEL_VERSION, AW_USB_READ, AW_USB_WRITE};
use crate::device::{DeviceProbe, ProbeOutcome, SocInfo};
use crate::error::RfError;

const TIMEOUT: Duration = Duration::from_millis(2000);
```

Then append the probe implementation to the end of the file:

```rust
/// Real probe: scans the bus, and if a FEL device is present, attempts the
/// version handshake. Claim failure (driver not bound) maps to DetectedNoDriver.
pub struct UsbProbe;

impl DeviceProbe for UsbProbe {
    fn probe(&self) -> ProbeOutcome {
        match probe_once() {
            Ok(outcome) => outcome,
            // A transient bus error is treated as "nothing usable present".
            Err(_) => ProbeOutcome::Absent,
        }
    }
}

fn probe_once() -> Result<ProbeOutcome, RfError> {
    let ctx = rusb::Context::new().map_err(|e| RfError::FelProtocolError(e.to_string()))?;
    let devices = ctx.devices().map_err(|e| RfError::FelProtocolError(e.to_string()))?;

    for device in devices.iter() {
        let desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };
        if !is_fel_device(desc.vendor_id(), desc.product_id()) {
            continue;
        }
        // FEL device present. Try to open + claim; failure => driver not bound.
        return match handshake(&device) {
            Ok(soc) => Ok(ProbeOutcome::Connected(soc)),
            Err(_) => Ok(ProbeOutcome::DetectedNoDriver),
        };
    }
    Ok(ProbeOutcome::Absent)
}

/// Find the bulk IN/OUT endpoint addresses on the first interface.
fn bulk_endpoints<T: UsbContext>(device: &rusb::Device<T>) -> Result<(u8, u8), RfError> {
    let config = device
        .active_config_descriptor()
        .map_err(|e| RfError::FelProtocolError(e.to_string()))?;
    let (mut ep_in, mut ep_out) = (fel::FEL_EP_IN, fel::FEL_EP_OUT);
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
    Ok((ep_in, ep_out))
}

/// Build the 32-byte AWUC request envelope.
fn aw_usb_request(req: u16, len: u32) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[0..4].copy_from_slice(b"AWUC");
    b[8..12].copy_from_slice(&len.to_le_bytes());
    b[12..16].copy_from_slice(&0x0c00_0000u32.to_le_bytes());
    b[16..18].copy_from_slice(&req.to_le_bytes());
    b[18..22].copy_from_slice(&len.to_le_bytes());
    b
}

/// Build the 16-byte FEL request.
fn fel_request(request: u32, address: u32, length: u32) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&request.to_le_bytes());
    b[4..8].copy_from_slice(&address.to_le_bytes());
    b[8..12].copy_from_slice(&length.to_le_bytes());
    b
}

fn handshake<T: UsbContext>(device: &rusb::Device<T>) -> Result<crate::device::SocInfo, RfError> {
    let mut handle = device
        .open()
        .map_err(|e| RfError::UsbClaimFailed(e.to_string()))?;
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle
        .claim_interface(0)
        .map_err(|e| RfError::UsbClaimFailed(e.to_string()))?;

    let (ep_in, ep_out) = bulk_endpoints(device)?;

    let w = |data: &[u8]| -> Result<(), RfError> {
        handle
            .write_bulk(ep_out, data, TIMEOUT)
            .map(|_| ())
            .map_err(|e| RfError::FelProtocolError(e.to_string()))
    };
    let mut read_into = |buf: &mut [u8]| -> Result<usize, RfError> {
        handle
            .read_bulk(ep_in, buf, TIMEOUT)
            .map_err(|e| RfError::FelProtocolError(e.to_string()))
    };

    // 1. Send the FEL VERSION request, wrapped in an AWUC WRITE envelope.
    let fel_req = fel_request(AW_FEL_VERSION, 0, 0);
    w(&aw_usb_request(AW_USB_WRITE, fel_req.len() as u32))?;
    w(&fel_req)?;
    let mut status = [0u8; 13];
    read_into(&mut status)?; // trailing "AWUS" response

    // 2. Read the 32-byte version structure (AWUC READ envelope, then payload).
    w(&aw_usb_request(AW_USB_READ, 32))?;
    let mut version = [0u8; 32];
    read_into(&mut version)?;
    read_into(&mut status)?; // trailing "AWUS" response

    fel::parse_version(&version)
}
```

- [ ] **Step 5: Verify it compiles and is clippy-clean**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no errors. (Hardware behavior is verified in Task 11.)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/device/usb.rs
git commit -m "feat(core): add rusb FEL probe and version handshake"
```

---

## Task 8: Tauri command + event + poll thread

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Wire the command, event, and background poll thread**

Replace the body of `src-tauri/src/lib.rs` so it reads (keep the existing `mod` lines at top):

```rust
mod device;
mod error;

use std::thread;
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::device::usb::UsbProbe;
use crate::device::{DeviceStatus, Monitor};

/// Event name carrying live status changes to the frontend.
const STATUS_EVENT: &str = "device-status-changed";

/// One-shot status read for initial render. Builds a throwaway Monitor so a
/// single probe maps through the same `ProbeOutcome -> DeviceStatus` path.
#[tauri::command]
fn get_device_status() -> DeviceStatus {
    let mut m = Monitor::new(UsbProbe);
    m.tick().unwrap_or(DeviceStatus::Disconnected)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_device_status])
        .setup(|app| {
            let handle = app.handle().clone();
            thread::spawn(move || {
                let mut monitor = Monitor::new(UsbProbe);
                loop {
                    if let Some(status) = monitor.tick() {
                        let _ = handle.emit(STATUS_EVENT, status);
                    }
                    thread::sleep(Duration::from_millis(1000));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

If `src-tauri/src/main.rs` does not already delegate to `run()`, set it to:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    retroforge_lib::run();
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo build`
Expected: builds with no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/main.rs
git commit -m "feat(core): expose device status command, event, and poll thread"
```

---

## Task 9: Frontend device wrapper + status panel

**Files:**
- Create: `src/lib/device.ts`, `src/components/StatusPanel.tsx`, `src/components/StatusPanel.test.tsx`
- Modify: `src/App.tsx`
- Create/Modify: `package.json` (test deps + script), `vitest.config.ts`, `src/test/setup.ts`

- [ ] **Step 1: Install test tooling**

```bash
npm install -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom
```

- [ ] **Step 2: Add `vitest.config.ts`**

```ts
import { defineConfig } from "vitest/config";
import path from "node:path";

export default defineConfig({
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
});
```

Create `src/test/setup.ts`:

```ts
import "@testing-library/jest-dom";
```

Add to `package.json` `"scripts"`: `"test": "vitest run"`.

- [ ] **Step 3: Create the typed device wrapper**

Create `src/lib/device.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface SocInfo {
  socId: number;
  name: string;
}

export type DeviceStatus =
  | { state: "disconnected" }
  | { state: "detectedNoDriver" }
  | { state: "connected"; soc: SocInfo };

export function getDeviceStatus(): Promise<DeviceStatus> {
  return invoke<DeviceStatus>("get_device_status");
}

export function onDeviceStatusChanged(
  handler: (status: DeviceStatus) => void,
): Promise<UnlistenFn> {
  return listen<DeviceStatus>("device-status-changed", (event) =>
    handler(event.payload),
  );
}
```

- [ ] **Step 4: Write the failing test for the status panel**

Create `src/components/StatusPanel.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { StatusPanel } from "./StatusPanel";

describe("StatusPanel", () => {
  it("shows disconnected", () => {
    render(<StatusPanel status={{ state: "disconnected" }} />);
    expect(screen.getByText(/no device/i)).toBeInTheDocument();
  });

  it("shows driver-not-bound guidance", () => {
    render(<StatusPanel status={{ state: "detectedNoDriver" }} />);
    expect(screen.getByText(/driver not bound/i)).toBeInTheDocument();
    expect(screen.getByText(/zadig/i)).toBeInTheDocument();
  });

  it("shows connected SoC name", () => {
    render(
      <StatusPanel
        status={{ state: "connected", soc: { socId: 0x1667, name: "Allwinner R16" } }}
      />,
    );
    expect(screen.getByText(/connected/i)).toBeInTheDocument();
    expect(screen.getByText(/Allwinner R16/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 5: Run the test to verify it fails**

Run: `npm test`
Expected: FAIL — cannot resolve `./StatusPanel`.

- [ ] **Step 6: Implement the status panel**

Create `src/components/StatusPanel.tsx`:

```tsx
import type { DeviceStatus } from "@/lib/device";

const DOT: Record<DeviceStatus["state"], string> = {
  disconnected: "bg-gray-400",
  detectedNoDriver: "bg-amber-500",
  connected: "bg-emerald-500",
};

export function StatusPanel({ status }: { status: DeviceStatus }) {
  return (
    <div className="flex items-start gap-3 rounded-lg border p-4">
      <span className={`mt-1 h-3 w-3 rounded-full ${DOT[status.state]}`} />
      <div className="text-sm">
        {status.state === "disconnected" && (
          <p className="font-medium">No device detected</p>
        )}
        {status.state === "detectedNoDriver" && (
          <div>
            <p className="font-medium">Device found — driver not bound</p>
            <p className="text-gray-500">
              Bind the WinUSB driver to the FEL device using Zadig, then reconnect.
            </p>
          </div>
        )}
        {status.state === "connected" && (
          <p className="font-medium">
            Connected — {status.soc.name} (FEL)
          </p>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Run the test to verify it passes**

Run: `npm test`
Expected: PASS (3 passed).

- [ ] **Step 8: Wire `App.tsx` to live status**

Replace `src/App.tsx`:

```tsx
import { useEffect, useState } from "react";
import { StatusPanel } from "@/components/StatusPanel";
import {
  getDeviceStatus,
  onDeviceStatusChanged,
  type DeviceStatus,
} from "@/lib/device";

export default function App() {
  const [status, setStatus] = useState<DeviceStatus>({ state: "disconnected" });

  useEffect(() => {
    getDeviceStatus().then(setStatus).catch(() => {});
    const unlisten = onDeviceStatusChanged(setStatus);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <main className="mx-auto max-w-md p-8">
      <h1 className="mb-4 text-xl font-bold">RetroForge</h1>
      <StatusPanel status={status} />
    </main>
  );
}
```

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(ui): add device status wrapper and status panel"
```

---

## Task 10: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the CI workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  frontend:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - run: npm ci
      - run: npx tsc --noEmit
      - run: npm test

  core:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
      - run: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
      - run: cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 2: Run the same checks locally to confirm they pass**

```bash
npx tsc --noEmit
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all pass. If `cargo fmt --check` fails, run `cargo fmt --manifest-path src-tauri/Cargo.toml` and re-commit.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add cargo + npm checks"
```

---

## Task 11: Hardware acceptance (manual — the real gate)

**Files:**
- Create: `docs/HARDWARE-ACCEPTANCE.md`

This task validates the slice against the on-hand device and captures real bytes to harden the parser fixture.

- [ ] **Step 1: Write the acceptance checklist**

Create `docs/HARDWARE-ACCEPTANCE.md`:

```markdown
# Hardware Acceptance — Skeleton + Comms

Verifies the FEL detection + version handshake against a real Classic Mini.

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
   "driver not bound" with Zadig guidance.
9. Unplug: panel returns to grey within ~1s.

## Capture for regression
10. If the handshake fails or the SoC name is wrong, capture the raw 32-byte version
    response (add a temporary `eprintln!("{:02x?}", version)` in `handshake`), and:
    - correct `soc_name` / `parse_version` from the real bytes, and
    - replace the synthetic fixture in `device::fel::tests` with the captured bytes.
```

- [ ] **Step 2: Run the manual checklist**

Perform steps 1–9 above against the device. All must pass. Record any deviations.

- [ ] **Step 3: If bytes differed, harden the parser**

If step 10 fired, update `parse_version`/`soc_name` and the `fel` test fixture from the real capture, then:

Run: `cd src-tauri && cargo test device::fel::tests`
Expected: PASS with the real-bytes fixture.

- [ ] **Step 4: Commit**

```bash
git add docs/HARDWARE-ACCEPTANCE.md src-tauri/src/device/fel.rs
git commit -m "docs: add hardware acceptance checklist; harden FEL parser from capture"
```

---

## Done criteria

- `npm run tauri dev` opens the app; status panel reflects live device state.
- Plugging a Classic Mini in FEL mode (WinUSB bound) shows green "Connected — Allwinner R16".
- `cargo test`, `cargo clippy -D warnings`, `npm test`, `tsc --noEmit` all pass.
- CI is green on push.
