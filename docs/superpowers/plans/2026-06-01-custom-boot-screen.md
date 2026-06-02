# Custom Boot Screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user pick a PNG and see it as the device boot logo by repacking the cached hakchi `boot.img` (swap `boot.png` in its XZ-cpio ramdisk) and RAM-membooting it via the existing FEL path — non-persistent, zero NAND writes.

**Architecture:** Three new pure Rust modules — `bootimage` (Android image split/reassemble), `ramdisk` (XZ + newc-cpio unpack/repack), `bootscreen` (PNG validation + orchestration) — plus a `memboot_bootscreen` Tauri command that reuses the memboot staging path with a custom image. Frontend passes PNG bytes; no native file dialog.

**Tech Stack:** Rust, Tauri 2, `xz2` (liblzma) for ramdisk (de)compression; hand-rolled newc-cpio + Android-header + PNG-IHDR parsing (no extra crates).

**Spec:** `docs/superpowers/specs/2026-06-01-custom-boot-screen-design.md`

**Deviation from spec:** the command takes PNG **bytes** (`Vec<u8>`), not a path — the frontend reads the file via `<input type=file>` and sends bytes, avoiding a native-dialog plugin and webview path issues.

---

## File Structure

- `src-tauri/Cargo.toml` — add `xz2`.
- `src-tauri/src/device/bootimage.rs` — **new.** Android boot image: `extract_ramdisk`, `replace_ramdisk`.
- `src-tauri/src/device/ramdisk.rs` — **new.** `xz_decompress`, `xz_compress`, `cpio_replace`.
- `src-tauri/src/device/bootscreen.rs` — **new.** `PngDims`, `read_png_dims`, `validate_png`, `build_custom_bootimg`.
- `src-tauri/src/device/mod.rs` — register the three modules.
- `src-tauri/src/error.rs` — add `RamdiskError(String)`, `BootScreenInvalidPng(String)`.
- `src-tauri/src/lib.rs` — extract a shared `memboot_image(...)`; add `memboot_bootscreen` command + `BootscreenProgress` events.
- `src/lib/bootscreen.ts` — **new.** `membootBootscreen(bytes)`, progress listener.
- `src/components/BootScreenRunner.tsx` — **new.** PNG picker + trigger, reusing progress UI.
- `src/App.tsx` — mount the new component (follow how `ShellRunner` is mounted).

---

## Task 1: Add the xz2 dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add the dependency**

Append to the `[dependencies]` table in `src-tauri/Cargo.toml`:

```toml
xz2 = "0.1"
```

> `xz2` wraps liblzma via `lzma-sys`, which vendors and builds the C source with
> the existing MSVC toolchain. If the build fails for lack of a C compiler,
> stop and report — do not switch crates without guidance.

- [ ] **Step 2: Verify it builds**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compiles (builds liblzma). No errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build: add xz2 for ramdisk (de)compression"
```

---

## Task 2: Error variants

**Files:**
- Modify: `src-tauri/src/error.rs`

- [ ] **Step 1: Add the variants**

In `src-tauri/src/error.rs`, add these two variants inside the `RfError` enum
(place them after the existing `SshError` variant):

```rust
    #[error("ramdisk error: {0}")]
    RamdiskError(String),
    #[error("invalid boot-screen PNG: {0}")]
    BootScreenInvalidPng(String),
```

- [ ] **Step 2: Build + run the existing error test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib error`
Expected: `error::tests::serializes_to_tagged_object` still passes (it uses
`UsbClaimFailed`, untouched).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/error.rs
git commit -m "feat(error): add RamdiskError + BootScreenInvalidPng"
```

---

## Task 3: bootimage.rs — Android image ramdisk swap

**Files:**
- Create: `src-tauri/src/device/bootimage.rs`
- Modify: `src-tauri/src/device/mod.rs`

Android boot image v0 header (little-endian): magic `ANDROID!` @0; `kernel_size`
@8; `ramdisk_size` @16; `page_size` @36. Layout in pages: header(1) · kernel ·
ramdisk · second. We swap only the ramdisk, patch `ramdisk_size`, re-pad, and
preserve everything else (including the stale `id` SHA — `boota` ignores it).

- [ ] **Step 1: Register the module**

In `src-tauri/src/device/mod.rs`, add (alphabetical, before `clovershell`? note
clovershell is gone — place near the top of the `pub mod` list):

```rust
pub mod bootimage;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/device/bootimage.rs`:

```rust
use crate::error::RfError;

const MAGIC: &[u8; 8] = b"ANDROID!";

fn rd_u32(img: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([img[off], img[off + 1], img[off + 2], img[off + 3]])
}

fn pages(n: usize, page: usize) -> usize {
    n.div_ceil(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a synthetic Android image: page=2048, kernel=`k`, ramdisk=`r`.
    fn make_img(page: usize, kernel: &[u8], ramdisk: &[u8]) -> Vec<u8> {
        let mut img = vec![0u8; page]; // header page
        img[..8].copy_from_slice(MAGIC);
        img[8..12].copy_from_slice(&(kernel.len() as u32).to_le_bytes());
        img[16..20].copy_from_slice(&(ramdisk.len() as u32).to_le_bytes());
        img[36..40].copy_from_slice(&(page as u32).to_le_bytes());
        let mut k = kernel.to_vec();
        k.resize(pages(kernel.len(), page) * page, 0);
        let mut r = ramdisk.to_vec();
        r.resize(pages(ramdisk.len(), page) * page, 0);
        img.extend_from_slice(&k);
        img.extend_from_slice(&r);
        img
    }

    #[test]
    fn extract_ramdisk_returns_exact_bytes() {
        let img = make_img(2048, b"KERNELDATA", b"RAMDISKBYTES");
        assert_eq!(extract_ramdisk(&img).unwrap(), b"RAMDISKBYTES");
    }

    #[test]
    fn replace_ramdisk_updates_size_and_bytes_keeps_kernel() {
        let img = make_img(2048, b"KERNELDATA", b"OLDRAMDISK");
        let out = replace_ramdisk(&img, b"A_MUCH_LONGER_NEW_RAMDISK_PAYLOAD").unwrap();
        // ramdisk_size field updated.
        assert_eq!(rd_u32(&out, 16) as usize, b"A_MUCH_LONGER_NEW_RAMDISK_PAYLOAD".len());
        // kernel unchanged, new ramdisk present.
        assert_eq!(extract_ramdisk(&out).unwrap(), b"A_MUCH_LONGER_NEW_RAMDISK_PAYLOAD");
        let page = 2048usize;
        assert_eq!(&out[page..page + b"KERNELDATA".len()], b"KERNELDATA");
    }

    #[test]
    fn rejects_non_android() {
        let mut img = make_img(2048, b"K", b"R");
        img[0] = b'X';
        assert!(extract_ramdisk(&img).is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib bootimage`
Expected: FAIL to compile — `extract_ramdisk`/`replace_ramdisk` not found.

- [ ] **Step 4: Implement**

Add to `bootimage.rs` above the test module:

```rust
struct Layout {
    page: usize,
    kernel_off: usize,
    kernel_size: usize,
    ramdisk_off: usize,
    ramdisk_size: usize,
}

fn parse(img: &[u8]) -> Result<Layout, RfError> {
    if img.len() < 40 || &img[0..8] != MAGIC {
        return Err(RfError::ExecFailed("not an Android boot image".into()));
    }
    let page = rd_u32(img, 36) as usize;
    if page == 0 {
        return Err(RfError::ExecFailed("zero page size".into()));
    }
    let kernel_size = rd_u32(img, 8) as usize;
    let ramdisk_size = rd_u32(img, 16) as usize;
    let kernel_off = page;
    let ramdisk_off = kernel_off + pages(kernel_size, page) * page;
    if ramdisk_off + ramdisk_size > img.len() {
        return Err(RfError::ExecFailed("truncated boot image".into()));
    }
    Ok(Layout { page, kernel_off, kernel_size, ramdisk_off, ramdisk_size })
}

/// The compressed ramdisk bytes from an Android boot image.
pub fn extract_ramdisk(img: &[u8]) -> Result<Vec<u8>, RfError> {
    let l = parse(img)?;
    Ok(img[l.ramdisk_off..l.ramdisk_off + l.ramdisk_size].to_vec())
}

/// Return a new boot image with the ramdisk replaced, `ramdisk_size` patched,
/// and all regions page-padded. Kernel, header (except size), second area, and
/// any trailing bytes are preserved. The `id` SHA is intentionally left stale —
/// hakchi `boota` does not verify it.
pub fn replace_ramdisk(img: &[u8], new_ramdisk: &[u8]) -> Result<Vec<u8>, RfError> {
    let l = parse(img)?;
    let page = l.page;
    let second_off = l.ramdisk_off + pages(l.ramdisk_size, page) * page;

    let mut out = Vec::with_capacity(img.len() + new_ramdisk.len());
    // Header page, with ramdisk_size patched.
    out.extend_from_slice(&img[0..page]);
    out[16..20].copy_from_slice(&(new_ramdisk.len() as u32).to_le_bytes());
    // Kernel region (original, already page-aligned in the source).
    out.extend_from_slice(&img[l.kernel_off..l.kernel_off + l.kernel_size]);
    pad_to_page(&mut out, page);
    // New ramdisk, page-padded.
    out.extend_from_slice(new_ramdisk);
    pad_to_page(&mut out, page);
    // Everything from the original second area onward (second + any tail).
    if second_off < img.len() {
        out.extend_from_slice(&img[second_off..]);
    }
    Ok(out)
}

fn pad_to_page(buf: &mut Vec<u8>, page: usize) {
    let rem = buf.len() % page;
    if rem != 0 {
        buf.resize(buf.len() + (page - rem), 0);
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib bootimage`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/device/bootimage.rs src-tauri/src/device/mod.rs
git commit -m "feat(bootimage): Android image ramdisk extract/replace"
```

---

## Task 4: ramdisk.rs — newc cpio replace-entry

**Files:**
- Create: `src-tauri/src/device/ramdisk.rs`
- Modify: `src-tauri/src/device/mod.rs`

newc cpio: 110-byte ASCII-hex header (`filesize` @54, `namesize` @94), then
name (incl NUL) padded to 4, then data padded to 4. Archive ends at the
`TRAILER!!!` entry.

- [ ] **Step 1: Register the module**

In `src-tauri/src/device/mod.rs` add:

```rust
pub mod ramdisk;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/device/ramdisk.rs`:

```rust
use crate::error::RfError;

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal newc entry writer for test fixtures.
    fn entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let f = |n: u32| format!("{n:08x}");
        v.extend_from_slice(b"070701");
        v.extend_from_slice(f(1).as_bytes()); // ino
        v.extend_from_slice(f(0o100644).as_bytes()); // mode
        for _ in 0..6 {
            v.extend_from_slice(f(0).as_bytes()); // uid,gid,nlink,mtime,filesize(placeholder),devmajor
        }
        // The loop above wrote filesize as 0 at the wrong spot; rebuild precisely:
        v.clear();
        v.extend_from_slice(b"070701");
        let fields = [
            1u32,            // ino
            0o100644,        // mode
            0,               // uid
            0,               // gid
            1,               // nlink
            0,               // mtime
            data.len() as u32, // filesize  (offset 54)
            0, 0, 0, 0,      // devmajor, devminor, rdevmajor, rdevminor
            (name.len() + 1) as u32, // namesize (offset 94)
            0,               // check
        ];
        for x in fields {
            v.extend_from_slice(f(x).as_bytes());
        }
        v.extend_from_slice(name.as_bytes());
        v.push(0);
        while v.len() % 4 != 0 {
            v.push(0);
        }
        v.extend_from_slice(data);
        while v.len() % 4 != 0 {
            v.push(0);
        }
        v
    }

    fn trailer() -> Vec<u8> {
        entry("TRAILER!!!", &[])
    }

    fn archive(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut v = Vec::new();
        for e in entries {
            v.extend_from_slice(e);
        }
        v.extend_from_slice(&trailer());
        v
    }

    #[test]
    fn cpio_replace_swaps_target_keeps_siblings() {
        let a = entry("etc/a.txt", b"AAAA");
        let target = entry("etc/boot.png", b"OLD");
        let c = entry("etc/c.txt", b"CCCC");
        let arc = archive(&[a.clone(), target, c.clone()]);
        let out = cpio_replace(&arc, "etc/boot.png", b"NEWLONGERPNGDATA").unwrap();
        // Re-extract the replaced entry and confirm new bytes + siblings intact.
        assert_eq!(cpio_read(&out, "etc/boot.png").unwrap(), b"NEWLONGERPNGDATA");
        assert_eq!(cpio_read(&out, "etc/a.txt").unwrap(), b"AAAA");
        assert_eq!(cpio_read(&out, "etc/c.txt").unwrap(), b"CCCC");
    }

    #[test]
    fn cpio_replace_errs_when_missing() {
        let arc = archive(&[entry("x", b"1")]);
        assert!(matches!(
            cpio_replace(&arc, "nope", b"z"),
            Err(RfError::RamdiskError(_))
        ));
    }

    // Test-only reader used to verify replace output.
    fn cpio_read(cpio: &[u8], path: &str) -> Option<Vec<u8>> {
        for ent in CpioIter::new(cpio) {
            if ent.name == path {
                return Some(ent.data.to_vec());
            }
        }
        None
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ramdisk`
Expected: FAIL to compile — `cpio_replace` / `CpioIter` not found.

- [ ] **Step 4: Implement**

Add to `ramdisk.rs` above the test module:

```rust
const NEWC_MAGIC: &[u8; 6] = b"070701";
const HDR_LEN: usize = 110;

fn hex8(b: &[u8], off: usize) -> usize {
    let s = std::str::from_utf8(&b[off..off + 8]).unwrap_or("0");
    usize::from_str_radix(s, 16).unwrap_or(0)
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// A parsed newc entry view into the archive.
struct Entry<'a> {
    name: String,
    data: &'a [u8],
    start: usize,
    end: usize, // byte after this entry (next entry start)
}

/// Iterates newc entries up to (and including) TRAILER!!!.
struct CpioIter<'a> {
    buf: &'a [u8],
    pos: usize,
    done: bool,
}

impl<'a> CpioIter<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0, done: false }
    }
}

impl<'a> Iterator for CpioIter<'a> {
    type Item = Entry<'a>;
    fn next(&mut self) -> Option<Entry<'a>> {
        if self.done || self.pos + HDR_LEN > self.buf.len() {
            return None;
        }
        let h = &self.buf[self.pos..];
        if &h[0..6] != NEWC_MAGIC {
            return None;
        }
        let filesize = hex8(h, 54);
        let namesize = hex8(h, 94);
        let name_off = self.pos + HDR_LEN;
        let name_end = name_off + namesize;
        let name = String::from_utf8_lossy(&self.buf[name_off..name_end - 1]).to_string();
        let data_off = align4(name_end);
        let data_end = data_off + filesize;
        let next = align4(data_end);
        let ent = Entry {
            name: name.clone(),
            data: &self.buf[data_off..data_end],
            start: self.pos,
            end: next,
        };
        if name == "TRAILER!!!" {
            self.done = true;
        }
        self.pos = next;
        Some(ent)
    }
}

/// Replace the data of the file named `path`, re-emitting all other entries
/// byte-for-byte. Errors if `path` is absent.
pub fn cpio_replace(cpio: &[u8], path: &str, new_data: &[u8]) -> Result<Vec<u8>, RfError> {
    let mut out = Vec::with_capacity(cpio.len() + new_data.len());
    let mut found = false;
    for ent in CpioIter::new(cpio) {
        if ent.name == path {
            found = true;
            // Copy the 110-byte header + name (+padding) verbatim, but patch
            // the filesize field, then append new data + 4-byte padding.
            let name_end = ent.start + HDR_LEN + hex8(&cpio[ent.start..], 94);
            let header_and_name_end = align4(name_end);
            let mut head = cpio[ent.start..header_and_name_end].to_vec();
            let fs = format!("{:08x}", new_data.len());
            head[54..62].copy_from_slice(fs.as_bytes());
            out.extend_from_slice(&head);
            out.extend_from_slice(new_data);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        } else {
            out.extend_from_slice(&cpio[ent.start..ent.end]);
        }
    }
    if !found {
        return Err(RfError::RamdiskError(format!("entry not found: {path}")));
    }
    Ok(out)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ramdisk`
Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/device/ramdisk.rs src-tauri/src/device/mod.rs
git commit -m "feat(ramdisk): newc cpio replace-entry"
```

---

## Task 5: ramdisk.rs — XZ (de)compress

**Files:**
- Modify: `src-tauri/src/device/ramdisk.rs`

The device consumes an XZ stream; build ours with a **CRC32** check and the
default LZMA2 filter to match the original container.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `ramdisk.rs`:

```rust
    #[test]
    fn xz_round_trips() {
        let data = b"the quick brown fox jumps over the lazy dog".repeat(50);
        let comp = xz_compress(&data).unwrap();
        assert_ne!(comp, data, "should actually compress/encode");
        let back = xz_decompress(&comp).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn xz_decompress_rejects_garbage() {
        assert!(matches!(
            xz_decompress(b"not an xz stream at all"),
            Err(RfError::RamdiskError(_))
        ));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ramdisk::tests::xz`
Expected: FAIL to compile — `xz_compress` / `xz_decompress` not found.

- [ ] **Step 3: Implement**

Add to `ramdisk.rs` (above the test module). Add the imports at the top of the
file:

```rust
use std::io::Read;
use xz2::read::XzDecoder;
use xz2::stream::{Check, Stream};
use xz2::write::XzEncoder;
use std::io::Write;
```

```rust
/// Decompress an XZ stream.
pub fn xz_decompress(data: &[u8]) -> Result<Vec<u8>, RfError> {
    let mut out = Vec::new();
    XzDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(|e| RfError::RamdiskError(format!("xz decompress: {e}")))?;
    Ok(out)
}

/// Compress to an XZ stream with a CRC32 integrity check (matches the kernel's
/// expectations for an XZ-compressed ramdisk).
pub fn xz_compress(data: &[u8]) -> Result<Vec<u8>, RfError> {
    let stream = Stream::new_easy_encoder(6, Check::Crc32)
        .map_err(|e| RfError::RamdiskError(format!("xz init: {e}")))?;
    let mut enc = XzEncoder::new_stream(Vec::new(), stream);
    enc.write_all(data)
        .map_err(|e| RfError::RamdiskError(format!("xz write: {e}")))?;
    enc.finish()
        .map_err(|e| RfError::RamdiskError(format!("xz finish: {e}")))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ramdisk`
Expected: 4 passed (2 cpio + 2 xz).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/device/ramdisk.rs
git commit -m "feat(ramdisk): XZ compress/decompress (crc32)"
```

---

## Task 6: bootscreen.rs — PNG validation

**Files:**
- Create: `src-tauri/src/device/bootscreen.rs`
- Modify: `src-tauri/src/device/mod.rs`

PNG: 8-byte signature, then the IHDR chunk (length=13, type `IHDR`, then
width(4 BE), height(4 BE), bit-depth(1), colour-type(1), compression(1),
filter(1), interlace(1)). We require the new PNG to match the stock logo's
dimensions and be `decodepng`-friendly: 8-bit, non-interlaced, RGB(2)/RGBA(6).

- [ ] **Step 1: Register the module**

In `src-tauri/src/device/mod.rs` add:

```rust
pub mod bootscreen;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/device/bootscreen.rs`:

```rust
use crate::error::RfError;

/// Width/height of a PNG, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngDims {
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

    // Minimal PNG with a controllable IHDR (no real image data needed for the
    // header parser).
    fn png(width: u32, height: u32, bit_depth: u8, colour: u8, interlace: u8) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&SIG);
        v.extend_from_slice(&13u32.to_be_bytes()); // IHDR length
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&height.to_be_bytes());
        v.push(bit_depth);
        v.push(colour);
        v.push(0); // compression
        v.push(0); // filter
        v.push(interlace);
        v.extend_from_slice(&[0, 0, 0, 0]); // fake CRC
        v
    }

    #[test]
    fn read_png_dims_parses_ihdr() {
        let p = png(1280, 720, 8, 6, 0);
        assert_eq!(read_png_dims(&p).unwrap(), PngDims { width: 1280, height: 720 });
    }

    #[test]
    fn validate_accepts_matching_rgba() {
        let p = png(1280, 720, 8, 6, 0);
        validate_png(&p, PngDims { width: 1280, height: 720 }).unwrap();
    }

    #[test]
    fn validate_rejects_wrong_dims() {
        let p = png(640, 480, 8, 6, 0);
        assert!(matches!(
            validate_png(&p, PngDims { width: 1280, height: 720 }),
            Err(RfError::BootScreenInvalidPng(_))
        ));
    }

    #[test]
    fn validate_rejects_interlaced_and_bad_depth() {
        let interlaced = png(1280, 720, 8, 6, 1);
        assert!(validate_png(&interlaced, PngDims { width: 1280, height: 720 }).is_err());
        let depth4 = png(1280, 720, 4, 6, 0);
        assert!(validate_png(&depth4, PngDims { width: 1280, height: 720 }).is_err());
    }

    #[test]
    fn read_png_dims_rejects_non_png() {
        assert!(read_png_dims(b"not a png").is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib bootscreen`
Expected: FAIL to compile — `read_png_dims` / `validate_png` not found.

- [ ] **Step 4: Implement**

Add to `bootscreen.rs` above the test module:

```rust
const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Parse width/height from a PNG's IHDR chunk.
pub fn read_png_dims(png: &[u8]) -> Result<PngDims, RfError> {
    if png.len() < 33 || png[0..8] != PNG_SIG || &png[12..16] != b"IHDR" {
        return Err(RfError::BootScreenInvalidPng("not a PNG (bad signature/IHDR)".into()));
    }
    let be = |o: usize| u32::from_be_bytes([png[o], png[o + 1], png[o + 2], png[o + 3]]);
    Ok(PngDims { width: be(16), height: be(20) })
}

/// Ensure the PNG matches the stock logo's dimensions and is decodepng-friendly:
/// 8-bit depth, non-interlaced, RGB (colour type 2) or RGBA (colour type 6).
pub fn validate_png(png: &[u8], expect: PngDims) -> Result<(), RfError> {
    let dims = read_png_dims(png)?;
    if dims != expect {
        return Err(RfError::BootScreenInvalidPng(format!(
            "dimensions {}x{} must match the stock logo {}x{}",
            dims.width, dims.height, expect.width, expect.height
        )));
    }
    let bit_depth = png[24];
    let colour = png[25];
    let interlace = png[28];
    if bit_depth != 8 {
        return Err(RfError::BootScreenInvalidPng(format!("bit depth {bit_depth} != 8")));
    }
    if colour != 2 && colour != 6 {
        return Err(RfError::BootScreenInvalidPng(format!(
            "colour type {colour} (need 2=RGB or 6=RGBA)"
        )));
    }
    if interlace != 0 {
        return Err(RfError::BootScreenInvalidPng("interlaced PNG not supported".into()));
    }
    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib bootscreen`
Expected: 5 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/device/bootscreen.rs src-tauri/src/device/mod.rs
git commit -m "feat(bootscreen): PNG IHDR parse + validation"
```

---

## Task 7: bootscreen.rs — build_custom_bootimg

**Files:**
- Modify: `src-tauri/src/device/bootscreen.rs`

Ties the pieces together: extract ramdisk → xz-decompress → read the stock
`boot.png` dims → validate the new PNG against them → cpio-replace → xz-compress
→ replace the ramdisk in the image.

- [ ] **Step 1: Write the failing test (ignored, real-asset)**

Add to the `tests` module in `bootscreen.rs`. This mirrors the existing
hardware-adjacent ignored-test pattern; it needs the machine-local cached hmod.

```rust
    // Real-asset check: extract boot/boot.img from the cached hmod, swap its
    // boot.png with a generated same-dims PNG, and assert the rebuilt image
    // re-extracts a ramdisk whose boot.png is the new bytes. CI-safe (skips
    // without the env var).
    //   set RF_HMOD_PATH=%LOCALAPPDATA%\com.retroforge.app\hakchi-latest.hmod
    //   cargo test --manifest-path src-tauri/Cargo.toml build_custom_bootimg_real -- --ignored --nocapture
    #[test]
    #[ignore = "needs the cached hmod; set RF_HMOD_PATH"]
    fn build_custom_bootimg_real() {
        let path = std::env::var("RF_HMOD_PATH").expect("set RF_HMOD_PATH");
        let archive = std::fs::read(&path).expect("read hmod");
        let stock = crate::device::hmod::extract_entry(&archive, "boot/boot.img")
            .expect("extract boot/boot.img");

        // Discover the stock logo dims, then synthesize a same-dims RGBA PNG.
        let ramdisk = crate::device::ramdisk::xz_decompress(
            &crate::device::bootimage::extract_ramdisk(&stock).unwrap(),
        )
        .unwrap();
        let stock_png = current_boot_png(&ramdisk).expect("stock boot.png present");
        let dims = read_png_dims(&stock_png).unwrap();
        let new_png = make_solid_rgba_png(dims);

        let out = build_custom_bootimg(&stock, &new_png).expect("build");
        // Re-extract and confirm the swap landed.
        let rd2 = crate::device::ramdisk::xz_decompress(
            &crate::device::bootimage::extract_ramdisk(&out).unwrap(),
        )
        .unwrap();
        let got = current_boot_png(&rd2).expect("new boot.png present");
        assert_eq!(got, new_png);
    }

    // Helper: build a valid 8-bit RGBA PNG of the given dims (a single IDAT of
    // zlib-stored filtered rows). Small + self-contained for the test.
    fn make_solid_rgba_png(dims: PngDims) -> Vec<u8> {
        use std::io::Write;
        fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut crc_in = kind.to_vec();
            crc_in.extend_from_slice(data);
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
        }
        fn crc32(b: &[u8]) -> u32 {
            let mut c: u32 = 0xffff_ffff;
            for &x in b {
                c ^= x as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
                }
            }
            !c
        }
        let (w, h) = (dims.width, dims.height);
        let mut raw = Vec::new();
        for _ in 0..h {
            raw.push(0u8); // filter: none
            for _ in 0..w {
                raw.extend_from_slice(&[0, 0, 0, 255]); // opaque black RGBA
            }
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        z.write_all(&raw).unwrap();
        let idat = z.finish().unwrap();

        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, no interlace
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &idat);
        chunk(&mut png, b"IEND", &[]);
        png
    }
```

> `flate2` is already a dependency of the crate, so the test helper can use it.

- [ ] **Step 2: Run the ignored test to confirm it compiles + is skipped**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib bootscreen`
Expected: FAIL to compile — `build_custom_bootimg` / `current_boot_png` not found.

- [ ] **Step 3: Implement**

Add to `bootscreen.rs` above the test module. Add `use crate::device::{bootimage, ramdisk};` at the top of the file.

```rust
/// Path of the boot logo inside the hakchi ramdisk overlay.
const BOOT_PNG_PATH: &str = "hakchi/rootfs/etc/boot.png";

/// The current boot.png bytes inside a decompressed ramdisk cpio, if present.
fn current_boot_png(cpio: &[u8]) -> Option<Vec<u8>> {
    // Reuse ramdisk::cpio_replace's parser by attempting a no-op read: we expose
    // a tiny reader here to avoid duplicating the iterator.
    ramdisk::cpio_read(cpio, BOOT_PNG_PATH)
}

/// Build a boot image identical to `stock` except its ramdisk's boot.png is
/// replaced by `new_png`. Validates the PNG against the stock logo's dimensions.
pub fn build_custom_bootimg(stock: &[u8], new_png: &[u8]) -> Result<Vec<u8>, RfError> {
    let comp = bootimage::extract_ramdisk(stock)?;
    let cpio = ramdisk::xz_decompress(&comp)?;
    let stock_png = current_boot_png(&cpio)
        .ok_or_else(|| RfError::RamdiskError(format!("{BOOT_PNG_PATH} not in ramdisk")))?;
    let dims = read_png_dims(&stock_png)?;
    validate_png(new_png, dims)?;
    let new_cpio = ramdisk::cpio_replace(&cpio, BOOT_PNG_PATH, new_png)?;
    let new_comp = ramdisk::xz_compress(&new_cpio)?;
    bootimage::replace_ramdisk(stock, &new_comp)
}
```

Also add a public reader to `src-tauri/src/device/ramdisk.rs` (promote the
test-only reader to production so `current_boot_png` can use it). Add above the
`tests` module:

```rust
/// Return the data bytes of the named entry, if present.
pub fn cpio_read(cpio: &[u8], path: &str) -> Option<Vec<u8>> {
    for ent in CpioIter::new(cpio) {
        if ent.name == path {
            return Some(ent.data.to_vec());
        }
    }
    None
}
```

Then in `ramdisk.rs` tests, delete the local `cpio_read` helper (now provided by
the module) so the Task-4 test uses the production one.

- [ ] **Step 4: Run the lib test suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`
Expected: all pass; `build_custom_bootimg_real` shows as ignored.

- [ ] **Step 5: (Optional, machine-local) run the real-asset test**

Run:
```bash
RF_HMOD_PATH="$LOCALAPPDATA/com.retroforge.app/hakchi-latest.hmod" \
  cargo test --manifest-path src-tauri/Cargo.toml build_custom_bootimg_real -- --ignored --nocapture
```
Expected: PASS (proves extract→decompress→swap→recompress→replace on the real
image). If it fails, the boot.png path or compression differs — report before
proceeding.

- [ ] **Step 6: Clippy + commit**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: clean.

```bash
git add src-tauri/src/device/bootscreen.rs src-tauri/src/device/ramdisk.rs
git commit -m "feat(bootscreen): build_custom_bootimg (swap boot.png in ramdisk)"
```

---

## Task 8: lib.rs — memboot_bootscreen command

**Files:**
- Modify: `src-tauri/src/lib.rs`

Refactor the memboot staging into a shared helper that takes the boot-image
bytes, then add a command that builds the custom image and stages it.

- [ ] **Step 1: Extract a shared memboot-from-bytes helper**

In `src-tauri/src/lib.rs`, locate `run_memboot`. It currently fetches blobs,
opens FEL, inits DRAM, sleeps, writes the boot image, runs `boota`, and waits
for FEL to leave. Refactor the staging half into a helper and have `run_memboot`
call it. Replace the body of `run_memboot` with:

```rust
fn run_memboot(app: &AppHandle, cache_dir: std::path::PathBuf) -> Result<(), RfError> {
    emit_progress(app, MembootProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let boot_img = blobs::boot_img(&cache_dir)?;
    stage_boot_image(app, &cache_dir, &boot_img)
}

/// Shared memboot staging: init DRAM, write the given boot image, boota it, and
/// wait for the FEL device to leave the bus. Used by both the stock memboot and
/// the custom boot-screen memboot.
fn stage_boot_image(
    app: &AppHandle,
    cache_dir: &std::path::Path,
    boot_img: &[u8],
) -> Result<(), RfError> {
    let fes1 = blobs::fes1();
    let uboot = to_sd_uboot(&blobs::uboot(cache_dir)?);

    emit_progress(app, MembootProgress::Connecting);
    let mut transport = open_fel()?;

    emit_progress(app, MembootProgress::InitDram);
    memboot_ops::init_dram(&mut transport, fes1)?;
    thread::sleep(Duration::from_millis(2000));

    emit_progress(app, MembootProgress::LoadingImage);
    let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
    if padded as u32 > fel::TRANSFER_MAX_SIZE {
        return Err(RfError::ExecFailed("boot image too large".into()));
    }
    let mut kernel = boot_img.to_vec();
    kernel.resize(padded, 0);
    memboot_ops::write_memory(&mut transport, fel::TRANSFER_BASE, &kernel)?;

    emit_progress(app, MembootProgress::LoadingUboot);
    emit_progress(app, MembootProgress::Executing);
    let cmd = format!("boota {:x}", fel::TRANSFER_BASE);
    memboot_ops::run_uboot_cmd(&mut transport, &uboot, &cmd)?;

    drop(transport);
    emit_progress(app, MembootProgress::WaitingForExit);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if !fel_present() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(RfError::MembootTimeout)
}
```

> Confirm against the existing `run_memboot` that this preserves its exact
> behavior (it should — same steps, same 2000ms settle, same constants). If the
> current `run_memboot` differs (e.g. uses a different sleep), keep its values.

- [ ] **Step 2: Add the boot-screen command**

Add near `run_memboot` / the `memboot` command in `src-tauri/src/lib.rs`:

```rust
use crate::device::bootscreen;

fn run_bootscreen(app: &AppHandle, cache_dir: std::path::PathBuf, png: &[u8]) -> Result<(), RfError> {
    emit_progress(app, MembootProgress::FetchingPayload);
    blobs::ensure_blobs(&cache_dir)?;
    let stock = blobs::boot_img(&cache_dir)?;
    let custom = bootscreen::build_custom_bootimg(&stock, png)?;
    stage_boot_image(app, &cache_dir, &custom)
}

#[tauri::command]
fn memboot_bootscreen(app: AppHandle, png: Vec<u8>) {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    std::thread::spawn(move || {
        DEVICE_BUSY.store(true, Ordering::SeqCst);
        let result = run_bootscreen(&app, cache_dir, &png);
        DEVICE_BUSY.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => emit_progress(&app, MembootProgress::Success),
            Err(e) => emit_progress(&app, MembootProgress::Failed { message: e.to_string() }),
        }
    });
}
```

> This reuses the existing `MembootProgress` events and `memboot-progress`
> channel, so the frontend can listen with the same handler the stock memboot
> uses.

- [ ] **Step 3: Register the command**

In the `tauri::generate_handler![...]` list in `run()`, add `memboot_bootscreen`:

```rust
        .invoke_handler(tauri::generate_handler![
            get_device_status,
            memboot,
            open_shell_and_run,
            memboot_bootscreen
        ])
```

- [ ] **Step 4: Build + test + clippy**

Run:
```bash
cargo build --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
Expected: builds, all tests pass, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(core): memboot_bootscreen command + shared staging"
```

---

## Task 9: Frontend — boot-screen control

**Files:**
- Create: `src/lib/bootscreen.ts`
- Create: `src/components/BootScreenRunner.tsx`
- Modify: `src/App.tsx`

- [ ] **Step 1: Add the IPC wrapper**

Create `src/lib/bootscreen.ts`. Reuse the existing `memboot-progress` event
shape (look at `src/lib/shell.ts` for the listener pattern; the memboot progress
type lives wherever the stock memboot UI consumes it — match that import).

```ts
import { invoke } from "@tauri-apps/api/core";

/** Send PNG bytes to the backend; it builds a custom boot.img and memboots it. */
export function membootBootscreen(bytes: Uint8Array): Promise<void> {
  return invoke<void>("memboot_bootscreen", { png: Array.from(bytes) });
}
```

- [ ] **Step 2: Add the component**

Create `src/components/BootScreenRunner.tsx`. It reads a chosen PNG file to
bytes and calls `membootBootscreen`. Reuse the memboot progress listener the
stock memboot UI uses (import it the same way that component does). Minimal:

```tsx
import { useState } from "react";
import { membootBootscreen } from "@/lib/bootscreen";

export function BootScreenRunner({ connected }: { connected: boolean }) {
  const [status, setStatus] = useState<string>("");

  async function onPick(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const bytes = new Uint8Array(await file.arrayBuffer());
    setStatus("Membooting custom boot screen…");
    try {
      await membootBootscreen(bytes);
    } catch {
      setStatus("Failed to start");
    }
  }

  return (
    <div className="mt-4 flex flex-col gap-2">
      <label className="rounded-md border px-4 py-2 text-sm font-medium">
        Choose boot-screen PNG…
        <input
          type="file"
          accept="image/png"
          disabled={!connected}
          onChange={onPick}
          className="hidden"
        />
      </label>
      {status && <p className="text-sm">{status}</p>}
    </div>
  );
}
```

> Listen to `memboot-progress` for richer status if the stock memboot UI exposes
> a reusable hook/listener — import and use it the same way rather than
> duplicating. Keep this component focused on the PNG-pick + invoke.

- [ ] **Step 3: Mount it**

In `src/App.tsx`, render `<BootScreenRunner connected={connected} />` next to the
existing `ShellRunner` (follow how `ShellRunner` receives its `connected` prop).

- [ ] **Step 4: Type-check + test**

Run: `npx tsc --noEmit` and `npm test`
Expected: no type errors; existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/bootscreen.ts src/components/BootScreenRunner.tsx src/App.tsx
git commit -m "feat(ui): boot-screen PNG picker + memboot trigger"
```

---

## Task 10: Final verification

- [ ] **Step 1: Full gate**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo build --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npx tsc --noEmit
npm test
```
Expected: fmt clean, build clean, all Rust tests pass (1+ ignored: the real-asset
boot-screen test), clippy clean, TS clean, frontend tests pass.

> Note: CI runs `cargo fmt --check` — run it locally before pushing (a prior
> branch tripped CI on formatting).

- [ ] **Step 2: Machine-local real-asset proof (optional but recommended)**

```bash
RF_HMOD_PATH="$LOCALAPPDATA/com.retroforge.app/hakchi-latest.hmod" \
  cargo test --manifest-path src-tauri/Cargo.toml build_custom_bootimg_real -- --ignored --nocapture
```
Expected: PASS.

- [ ] **Step 3: Hardware acceptance (manual, with device)**

Build a same-dimensions PNG, pick it in the app, trigger; confirm the custom
image membooots and the logo displays on the device HDMI, then boot continues.
**This is where the XZ-ramdisk-acceptance risk is settled** — if the device
rejects the repacked ramdisk, capture the symptom (no boot vs stock logo vs
hang) and revisit `xz_compress` options (check type, filters).

---

## Self-Review

- **Spec coverage:** repack pipeline (Tasks 3–7) ✓; Android split/reassemble
  with stale-id note (Task 3) ✓; XZ + cpio (Tasks 4–5) ✓; PNG validation vs
  stock dims (Task 6) ✓; `build_custom_bootimg` orchestration (Task 7) ✓;
  `memboot_bootscreen` command reusing memboot staging (Task 8) ✓; frontend
  picker (Task 9) ✓; CI-safe unit tests + ignored real-asset test + manual HW
  acceptance (Tasks 3–7, 10) ✓; error variants (Task 2) ✓; xz2 dep (Task 1) ✓.
- **Deviation:** command takes PNG bytes, not a path (documented in header) —
  spec's `memboot_bootscreen(png_path)` becomes `memboot_bootscreen(png: Vec<u8>)`.
- **Type consistency:** `extract_ramdisk`/`replace_ramdisk`,
  `xz_compress`/`xz_decompress`/`cpio_replace`/`cpio_read`, `PngDims`/
  `read_png_dims`/`validate_png`/`build_custom_bootimg`, `stage_boot_image`/
  `run_bootscreen`/`memboot_bootscreen` used consistently across tasks.
  `BOOT_PNG_PATH = "hakchi/rootfs/etc/boot.png"` matches the path verified in the
  ramdisk scan.
- **Note for the implementer:** `run_memboot`'s exact current body should be
  confirmed before refactoring (Task 8) — preserve its real sleep/constants if
  they differ from what's shown.
```
