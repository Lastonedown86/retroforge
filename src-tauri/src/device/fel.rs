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

    /// Real 32-byte FEL version response captured from an NES Classic Mini
    /// (Allwinner R16) over FEL. Signature "AWUSBFEX", soc_id 0x00166700.
    fn r16_response() -> Vec<u8> {
        vec![
            0x41, 0x57, 0x55, 0x53, 0x42, 0x46, 0x45, 0x58, // "AWUSBFEX"
            0x00, 0x67, 0x16, 0x00, // soc_id (0x00166700 LE) -> 0x1667
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x44, 0x08, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
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
