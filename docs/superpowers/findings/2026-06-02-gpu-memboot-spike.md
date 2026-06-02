# Brick-Safe GPU Memboot Tracer Bullet — Findings

> Records the host-side module-sourcing + vermagic-proof phase for
> `docs/superpowers/specs/2026-06-02-gpu-memboot-spike-design.md`.
> **Host-side outcome: PREBUILT MATCH FOUND** — `mali.ko` + `clovercon.ko`
> (+ companion `input-polldev.ko`) with the *exact* `3.4.113.29-madmonkey`
> vermagic were pulled from the **same `hakchi-latest.hmod` archive** that
> supplies our `boot.img`. On-device `insmod` / render / controller steps are
> still pending the device (placeholders below). No INCONCLUSIVE /
> build-from-source branch was needed.

## Module source + provenance

The gold-standard provenance: the modules, the boot image, and the hakchi
version stamp all live in **one archive** — the very file RetroForge downloads
at runtime.

- **Archive (= our boot.img source):**
  `https://hakchi.net/hakchi/hmods/hakchi-latest.hmod`
  - This is the exact URL RetroForge uses. See
    `src-tauri/src/device/hmod.rs` (`HMOD_URL`) and
    `src-tauri/src/device/blobs.rs` (`boot_img()` →
    `hmod::extract_entry(.., "boot/boot.img")`).
  - Format: gzip-compressed tar (magic `1f 8b`), matching the sniff in
    `hmod::extract_entry`.
  - Downloaded `2026-06-02`; `9 340 382` bytes;
    SHA256 `003994B46504959895F40B0B9E91B16951EF4A638FEB6312314C8FC4FDBFF592`.

- **Version stamp inside the archive** (`var/version`):
  ```
  bootVersion='1.0.3'
  hakchiVersion='v1.0.4-126'
  kernelVersion='3.4.113.29-madmonkey'
  ```
  So `hakchi-latest` currently resolves to **hakchi (ClusterM) `v1.0.4-126`**,
  kernel **`3.4.113.29-madmonkey`** — the exact kernel our memboot runs.

- **Modules inside the SAME archive** (all dated `2020-03-20 07:17`, same build
  batch as the `boot/boot.img` and `boot/uboot.bin`):
  | path in hmod | size |
  |---|---|
  | `lib/modules/3.4.113.29-madmonkey/extra/mali.ko` | 156 508 |
  | `lib/modules/3.4.113.29-madmonkey/extra/clovercon.ko` | 15 828 |
  | `lib/modules/3.4.113.29-madmonkey/kernel/drivers/input/input-polldev.ko` | 5 136 |

- **Provenance tie is mechanical, not just a string match.** The hmod's own
  `install` script copies `./lib/modules/` onto the rootfs **and** memboots
  `./boot/boot.img` in the same run — i.e. hakchi ships these `.ko` *as the
  module set for this exact boot.img/kernel*. Because they came out of the same
  `hakchi-latest.hmod` we fetch, the symbol CRCs are guaranteed to match the
  membooted kernel (no "same version string, different commit" risk). This is
  the best-case branch in the spec's module-source decision.

- **Companion modules:**
  - `mali.ko` — `depends:` is **empty**. UMP is built into this Utgard build;
    **no separate `ump.ko` exists anywhere in the archive** (the only "ump" hit
    is `ums-jumpshot.ko`, an unrelated USB-storage module). mali loads
    standalone.
  - `clovercon.ko` — `depends: input-polldev`. `input-polldev` is **NOT**
    built into the kernel (absent from `modules.builtin`); it ships as a
    separate `.ko` in the same tree. So the real companion in this set is
    **`input-polldev.ko`**, which must be `insmod`'d **before** `clovercon.ko`.
    It was extracted, verified, and staged alongside the others.

## Vermagic proof

Verified in WSL Ubuntu (`kmod` v31 `modinfo`, GNU `readelf` 2.42). Two
independent methods agree. Run against the **staged copies in `%TEMP%\mods\`**.

### `modinfo` (kmod v31)

```
===== mali.ko =====
filename:  .../mods/mali.ko
depends:
vermagic:  3.4.113.29-madmonkey SMP preempt mod_unload ARMv7

===== clovercon.ko =====
filename:  .../mods/clovercon.ko
depends:   input-polldev
vermagic:  3.4.113.29-madmonkey SMP preempt mod_unload ARMv7

===== input-polldev.ko =====
filename:  .../mods/input-polldev.ko
depends:
vermagic:  3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
```

### `readelf -h` (arch / class)

All three modules:
```
Class:    ELF32
Data:     2's complement, little endian
Type:     REL (Relocatable file)
Machine:  ARM
```

### `readelf -p .modinfo` (cross-check of the vermagic string)

```
mali.ko          vermagic=3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
clovercon.ko     depends=input-polldev
                 vermagic=3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
input-polldev.ko vermagic=3.4.113.29-madmonkey SMP preempt mod_unload ARMv7
```

**Result:** all three show `vermagic` **exactly** `3.4.113.29-madmonkey SMP
preempt mod_unload ARMv7`, `Class: ELF32`, `Machine: ARM`. Required criteria met.

### Staged files (`%TEMP%\mods\`)

| file | bytes | SHA256 |
|---|---|---|
| `mali.ko` | 156 508 | `FD33317B37E0CD495541029AD6534B0E8759E6E1E41D67CC2393D4DCFB0C764C` |
| `clovercon.ko` | 15 828 | `D3FB3AEA3E4FE6924CB58A91E30FF36E05D67DA7F3CF918D6266E852BAFB0A41` |
| `input-polldev.ko` | 5 136 | `BF402B7E2F469ED9D9493DF6266D38DCCC522146F8F8309A908DD2DEFF515393` |

`mali` parm of note for later: `mali_dedicated_mem_start` / `mali_dedicated_mem_size`
(stock dp-nes uses dedicated GPU mem). `clovercon` takes `module_params=` array
(the `con0_i2c_bus, con0_detect_gpio, ...` form) — stock dp-nes values come from
`/newroot/etc/init.d/S79clovercon`.

## insmod result

*(pending device — RAM-only)* Plan: tar-over-ssh `%TEMP%\mods\*.ko` →
`/tmp/mods/`, then by full path:
`insmod /tmp/mods/input-polldev.ko` → `insmod /tmp/mods/mali.ko`
(expect `/dev/mali`, `lsmod` shows `mali`, no `invalid module format`) →
`insmod /tmp/mods/clovercon.ko module_params=...` (dp-nes values; expect
`/dev/input/event*`). `modprobe` is broken on this image (no `modules.dep` for
`extra/`); `insmod` by full path. **Load order matters:** `input-polldev` before
`clovercon`.

## RA gl render

*(pending device)* Re-run staged RA (`/tmp/ra`) on `video_driver=gl`; expect the
Mali EGL surface to succeed (no `EGL_BAD_ALLOC`) and RGUI to render on HDMI.

## Controller

*(pending device)* After `clovercon.ko` loads with the dp-nes params, press
D-pad / A-B and confirm RGUI navigates.

## Verdict (pending device)

**Host-side: PREBUILT PATH SUCCEEDED.** A vermagic-exact, same-archive,
CRC-safe `mali.ko` + `clovercon.ko` (+ `input-polldev.ko` companion) were
sourced from the identical `hakchi-latest.hmod` that supplies our `boot.img` and
verified ARM/ELF32 with the exact `3.4.113.29-madmonkey` vermagic by two methods.
The spec's INCONCLUSIVE → build-from-source fallback was **not** required.

Final GO / PARTIAL / NO-GO awaits the on-device `insmod` + RA `gl` render +
controller-navigation run (the three placeholder sections above).
