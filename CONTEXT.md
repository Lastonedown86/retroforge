# RetroForge — Context

A modern tool for modding the NES Classic Mini (and related Classic-series consoles):
custom UI/UX and kernel upgrades. Spiritual successor to Hakchi2-CE, built ground-up
on a new stack rather than forking the C# WinForms codebase.

## Glossary

### RetroForge
The application being built (this repo). Ground-up rewrite. Hakchi2-CE is a reference
implementation, not a code base — its device-protocol knowledge is reused, its code is not.

### Hakchi2-CE
The existing forked tool (`../Hakchi2-CE`). C# WinForms, .NET Framework, Windows-first.
Talks to Classic consoles over USB via FEL recovery mode + a custom Clovershell/SSH
protocol. Serves as the reference for how device communication works.

### Classic Mini / the device
Nintendo's plug-and-play retro console (NES Classic Mini and siblings). Allwinner R16
SoC running a custom Linux. The hardware RetroForge mods.

### FEL mode
The Allwinner SoC's USB recovery/boot mode. Entry point for loading a custom boot image
onto the device without permanent reflashing of stock firmware.

### Custom boot image
A `boot.img` = Nintendo's stock R16 kernel binary + a modified ramdisk. RetroForge loads
this over FEL (memboot) and/or installs it persistently. RetroForge does NOT replace the
kernel binary.

### Kernel upgrade  (RetroForge meaning)
RESOLVED: a modernized **system image**, not a new kernel binary. Keep Nintendo's R16
Linux 3.4 kernel; ship a far better ramdisk/system layer — newer RetroArch + cores,
better controller driver, modern save-states/cheats/overlays, cleaner install. A true
mainline-kernel port and feature-backporting were both explicitly rejected for v1 as
out-of-scope effort/risk.

### System image
The on-device Linux userland RetroForge installs: ramdisk contents, system scripts,
RetroArch + cores, controller driver. Distinct from "the core" (the host-side Rust backend).
SOURCING: forked from Hakchi's `ce-data` on-device system (mature, language-agnostic
shell/C), modernized incrementally. Build path: get a working boot from the inherited
system first, THEN swap in newer RetroArch/cores and improvements. The two codebases —
host app (rewritten in Rust/React) and system image (forked from Hakchi) — evolve
independently.

### hmod
A kernel/system modification package applied on top of the running custom image.

### Install model
RESOLVED: persistent NAND install is primary — custom boot written to NAND once, device
boots modded standalone, games sync over USB/FTP afterward. Temporary RAM-memboot is
retained for testing and recovery/restore. (Same proven model as Hakchi.)

### Platform target
Windows-only (matching Hakchi's reach). Cross-platform is explicitly NOT a goal — the
rewrite is justified by UI/UX quality and kernel work, not OS reach. USB device access
uses the Windows WinUSB/libwdi driver path, as Hakchi2-CE does.

### Stack
Tauri application: React/TypeScript frontend (UI/UX), Rust backend ("the core"). The Rust
core owns all device communication — raw USB (rusb/libusb), FEL protocol, boot-image
packing, and SSH. Frontend talks to the core over Tauri commands/events.

### The core
The Rust backend. Owns device comms, boot-image manipulation, and kernel operations.
No device logic lives in the frontend.

### UX paradigm
RESOLVED: guided-first with power underneath (progressive disclosure). Default experience
is a novice-friendly wizard (connect → install → add games). Advanced operations (memboot
modes, NAND dump, recovery/restore) live behind an "Advanced" surface, not the main flow.
Single coherent UI, not two separate modes.

### Game
A single title in the Library: its ROM, canonical title, box art/metadata, and per-game
settings (emulator routing, etc.). Created via Import.

### Import
The pipeline that brings a ROM into the Library: detect/normalize the iNES header, compute
CRC, match against No-Intro/libretro for canonical title + art, dedup, flag bad/non-NES
files, user confirms. Soft-patching (IPS/BPS) is deferred post-v1.

### Library
The user's local collection of games plus their art, per-game settings, and menu layout —
the source of truth on the host that gets synced to the device. (Stored as a SQLite catalog
referencing a content-addressed asset folder; ROMs/art on disk keyed by CRC/hash.)

### Folder
A single-level grouping on the device home menu: the top level holds games and folders;
a folder holds games (one level deep, no nesting in v1). Realized on-device by the inherited
Hakchi folder-launcher system.

### Layout
The arrangement of games and folders on the home menu — order, folder membership, position.
Authored on the canvas, synced to the device. Optional auto-grouping (by first letter,
publisher, or user tags) generates a starting layout the user then hand-tweaks.

### Sync
Pushing the Library + Layout from host to device. Incremental: RetroForge tracks device
state (a manifest of what's installed, by hash) and pushes only the diff. The first sync to
a device is full; subsequent edits push only what changed.

### Adopt (import-from-device)
On connecting to a device that already holds content RetroForge didn't create (e.g. an
existing Hakchi install), RetroForge offers to pull that content — ROMs, art, layout — back
into the local Library so the canvas reflects reality before any Sync. The user may adopt or
start fresh. This is what makes the device-state manifest trustworthy on non-blank devices.

### Menu renderer (stock CLV)
The device home menu is Nintendo's stock CLV UI (the box-art grid), inherited via the forked
system — NOT a custom launcher. RetroForge themes it through established hooks (boot
logo/splash, background, system music, fonts/colors, console skin). A custom on-device
launcher is explicitly out of scope for v1 (possible opt-in far later). Because the menu is
the stock grid, the home-menu canvas can mirror it faithfully.

### Home-menu canvas
The central UI screen: a live, WYSIWYG representation of the device's stock CLV home menu.
Box-art grid, drag-to-arrange, drag-into-folders, drag-drop game import from disk. What the
user arranges here is what the console displays. The signature feature of RetroForge.

### Art & metadata sourcing
RESOLVED: on game import, auto-match the ROM (filename/CRC) against the libretro thumbnail
repo (free, no API key) to fill box art/title/snap. Per-game manual art override supported.
A richer, key-gated source (ScreenScraper/IGDB) is a deferred enhancement, not v1.

### Brick safety / recovery
RESOLVED: a full, verified NAND backup (size/hash-checked, stored locally) is MANDATORY
and gates any persistent NAND write — offered automatically, one-time. A prominent
one-click restore is always available. Non-negotiable safety policy for v1.

### Emulator routing
RESOLVED: new games default to a modern RetroArch core (Nestopia/FCEUmm/Mesen) — enabling
save-states, cheats, shaders, rewind (the "kernel upgrade" feature payload). Per-game
routing back to the stock kachikachi emulator is offered for authenticity/compatibility.

### Distribution & updates (host app)
RESOLVED: signed Windows installer (MSI/NSIS) published to GitHub Releases, with Tauri's
built-in auto-updater for notify + one-click update. Code-signing may come later
(SmartScreen warnings until signed).

### Theme
A bundle of device home-menu assets — boot logo/splash, background, system music,
fonts/colors, console skin — plus a manifest, applied to the stock CLV menu. RetroForge
ships built-in presets, imports shareable theme packs, and allows per-asset swap in v1
(no full visual editor until post-v1). Distinct from the host app's simple dark/light theme.
FORMAT: RetroForge reads the existing Hakchi theme/hmod theme format (so the community
back-catalog works day one) and layers a native superset manifest (metadata, previews,
versioning) for new themes.

<!-- Add terms as they are resolved during grilling. -->
