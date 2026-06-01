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
