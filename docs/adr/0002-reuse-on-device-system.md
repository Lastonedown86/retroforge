# Reuse Hakchi's on-device system image; rewrite only the host app

"Rewrite" applies only to the host application (the Windows desktop tool). The on-device
Linux system — ramdisk scripts, init, RetroArch glue, controller driver — is forked from
Hakchi's `ce-data` and modernized incrementally, NOT rewritten. These two codebases are
independent and evolve separately. A future reader will see a brand-new Rust/React host app
and be surprised to find inherited shell/C running on the device; the reason is that the
on-device system is language-agnostic Linux, mature, and has already debugged the device's
boot timing, NAND layout, and hardware quirks. Re-solving those from scratch would add brick
risk and slow time-to-first-boot for no UX benefit.

## Consequences

- Build path is "inherit a working boot first, then swap in newer RetroArch/cores and
  features" — not "author a new system image up front."
- RetroForge's value-add on the device side is modernization (newer cores, better driver,
  save-states/cheats/overlays), not a from-scratch system.
