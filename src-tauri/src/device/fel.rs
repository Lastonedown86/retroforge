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

// FEL request types (AWFELStandardRequest).
pub const FEL_DOWNLOAD: u32 = 0x101; // write data to device memory
pub const FEL_RUN: u32 = 0x102; // execute code at address
pub const FEL_UPLOAD: u32 = 0x103; // read data from device memory

// Memboot memory map (Allwinner R16 / Hakchi reference).
pub const FES1_BASE: u32 = 0x2000; // SRAM: DRAM-init blob load+exec address
pub const DRAM_BASE: u32 = 0x4000_0000;
pub const UBOOT_BASE: u32 = DRAM_BASE + 0x0700_0000; // 0x4700_0000
pub const TRANSFER_BASE: u32 = DRAM_BASE + 0x0740_0000; // 0x4740_0000
pub const SECTOR_SIZE: usize = 0x2_0000;
pub const TRANSFER_MAX_SIZE: u32 = (SECTOR_SIZE as u32) * 0x100; // 0x200_0000

/// Per-transfer chunk size. Kept at 4 KB: a single large bulk-OUT (e.g. 64 KB)
/// to the device stalls under the Windows WinUSB backend, so memory writes are
/// chunked small. Hardware-verified — larger values hang the DRAM image upload.
pub const MAX_BULK: usize = 0x1000;

/// Build the 32-byte AWUC USB request envelope.
pub fn aw_usb_request(req: u16, len: u32) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[0..4].copy_from_slice(b"AWUC");
    b[8..12].copy_from_slice(&len.to_le_bytes());
    b[12..16].copy_from_slice(&0x0c00_0000u32.to_le_bytes());
    b[16..18].copy_from_slice(&req.to_le_bytes());
    b[18..22].copy_from_slice(&len.to_le_bytes());
    b
}

/// Build the 16-byte FEL request (AWFELMessage): cmd (u16) + tag(0) + address + length + flags(0).
pub fn fel_request(request: u32, address: u32, length: u32) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&request.to_le_bytes());
    b[4..8].copy_from_slice(&address.to_le_bytes());
    b[8..12].copy_from_slice(&length.to_le_bytes());
    b
}

#[cfg(test)]
mod request_tests {
    use super::*;

    #[test]
    fn fel_request_encodes_download_addr_len() {
        let m = fel_request(FEL_DOWNLOAD, 0x4740_0000, 0x100);
        assert_eq!(&m[0..4], &[0x01, 0x01, 0x00, 0x00]); // cmd 0x101, tag 0
        assert_eq!(&m[4..8], &0x4740_0000u32.to_le_bytes()); // address
        assert_eq!(&m[8..12], &0x100u32.to_le_bytes()); // length
        assert_eq!(&m[12..16], &[0, 0, 0, 0]); // flags 0
    }

    #[test]
    fn fel_request_run_has_zero_len() {
        let m = fel_request(FEL_RUN, 0x4700_0000, 0);
        assert_eq!(&m[0..4], &[0x02, 0x01, 0x00, 0x00]); // cmd 0x102
        assert_eq!(&m[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn aw_usb_request_read_envelope() {
        let e = aw_usb_request(AW_USB_READ, 32);
        assert_eq!(&e[0..4], b"AWUC");
        assert_eq!(&e[8..12], &32u32.to_le_bytes());
        assert_eq!(&e[16..18], &AW_USB_READ.to_le_bytes());
    }
}

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
