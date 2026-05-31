# Ground-up Tauri rewrite instead of forking Hakchi2-CE

We are building RetroForge as a fresh Tauri app (React/TypeScript frontend, Rust core)
rather than forking or modernizing the mature Hakchi2-CE C# WinForms codebase. The target
is Windows-only — same reach as Hakchi — so this is NOT justified by cross-platform need;
it is justified by the UI/UX ceiling and a clean kernel/system-image story. A future reader
will reasonably ask "why throw away a working, battle-tested tool?" — the answer is that the
WinForms structure and .NET Framework caps the UX quality we want, and a web-UI frontend
buys the WYSIWYG home-menu canvas that is RetroForge's signature feature.

## Considered Options

- **Modernize C# in place** (WinForms → Avalonia/.NET 8): reuses all device code, but UX
  ceiling stays limited and we inherit legacy structure.
- **Hybrid** (new UI over headless C# core): keeps proven comms, but the glue layer is a
  permanent tax and the core stays C#.
- **Ground-up Tauri rewrite** (chosen): highest UX ceiling, Rust core for safe binary/USB
  handling, at the cost of reimplementing device comms and learning Rust for the core.

## Consequences

- All device communication (USB/FEL/SSH/boot-image packing) must be reimplemented in Rust;
  Hakchi2-CE's C# comms code is reference material only.
- Rust is a new language surface for the device layer (frontend stays in familiar React/TS).
