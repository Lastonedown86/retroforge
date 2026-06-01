use crate::device::{bootimage, ramdisk};
use crate::error::RfError;

/// Width/height of a PNG, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngDims {
    pub width: u32,
    pub height: u32,
}

const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Parse width/height from a PNG's IHDR chunk.
pub fn read_png_dims(png: &[u8]) -> Result<PngDims, RfError> {
    if png.len() < 33 || png[0..8] != PNG_SIG || &png[12..16] != b"IHDR" {
        return Err(RfError::BootScreenInvalidPng(
            "not a PNG (bad signature/IHDR)".into(),
        ));
    }
    let be = |o: usize| u32::from_be_bytes([png[o], png[o + 1], png[o + 2], png[o + 3]]);
    Ok(PngDims {
        width: be(16),
        height: be(20),
    })
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
        return Err(RfError::BootScreenInvalidPng(format!(
            "bit depth {bit_depth} != 8"
        )));
    }
    if colour != 2 && colour != 6 {
        return Err(RfError::BootScreenInvalidPng(format!(
            "colour type {colour} (need 2=RGB or 6=RGBA)"
        )));
    }
    if interlace != 0 {
        return Err(RfError::BootScreenInvalidPng(
            "interlaced PNG not supported".into(),
        ));
    }
    Ok(())
}

/// Path of the boot logo inside the hakchi ramdisk overlay. Verified against the
/// real cached hmod (`boot/boot.img` ramdisk): the only PNG asset is
/// `hakchi/rootfs/etc/hakchi.png` (consumed by `bin/decodepng` at boot).
const BOOT_PNG_PATH: &str = "hakchi/rootfs/etc/hakchi.png";

/// The current boot.png bytes inside a decompressed ramdisk cpio, if present.
fn current_boot_png(cpio: &[u8]) -> Option<Vec<u8>> {
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
        assert_eq!(
            read_png_dims(&p).unwrap(),
            PngDims {
                width: 1280,
                height: 720
            }
        );
    }

    #[test]
    fn validate_accepts_matching_rgba() {
        let p = png(1280, 720, 8, 6, 0);
        validate_png(
            &p,
            PngDims {
                width: 1280,
                height: 720,
            },
        )
        .unwrap();
    }

    #[test]
    fn validate_rejects_wrong_dims() {
        let p = png(640, 480, 8, 6, 0);
        assert!(matches!(
            validate_png(
                &p,
                PngDims {
                    width: 1280,
                    height: 720
                }
            ),
            Err(RfError::BootScreenInvalidPng(_))
        ));
    }

    #[test]
    fn validate_rejects_interlaced_and_bad_depth() {
        let interlaced = png(1280, 720, 8, 6, 1);
        assert!(validate_png(
            &interlaced,
            PngDims {
                width: 1280,
                height: 720
            }
        )
        .is_err());
        let depth4 = png(1280, 720, 4, 6, 0);
        assert!(validate_png(
            &depth4,
            PngDims {
                width: 1280,
                height: 720
            }
        )
        .is_err());
    }

    #[test]
    fn read_png_dims_rejects_non_png() {
        assert!(read_png_dims(b"not a png").is_err());
    }

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

        let ramdisk = crate::device::ramdisk::xz_decompress(
            &crate::device::bootimage::extract_ramdisk(&stock).unwrap(),
        )
        .unwrap();
        let stock_png = current_boot_png(&ramdisk).expect("stock boot.png present");
        let dims = read_png_dims(&stock_png).unwrap();
        let new_png = make_solid_rgba_png(dims);

        let out = build_custom_bootimg(&stock, &new_png).expect("build");
        let rd2 = crate::device::ramdisk::xz_decompress(
            &crate::device::bootimage::extract_ramdisk(&out).unwrap(),
        )
        .unwrap();
        let got = current_boot_png(&rd2).expect("new boot.png present");
        assert_eq!(got, new_png);
    }

    // Helper: build a valid 8-bit RGBA PNG of the given dims (single zlib IDAT of
    // filtered rows). Self-contained for the test.
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
                    c = if c & 1 != 0 {
                        (c >> 1) ^ 0xedb8_8320
                    } else {
                        c >> 1
                    };
                }
            }
            !c
        }
        let (w, h) = (dims.width, dims.height);
        let mut raw = Vec::new();
        for _ in 0..h {
            raw.extend_from_slice(&[0u8]); // filter: none
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
}
