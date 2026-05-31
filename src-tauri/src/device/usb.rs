use std::time::Duration;

use rusb::{Direction, TransferType, UsbContext};

use crate::device::fel::{self, AW_FEL_VERSION, AW_USB_READ, AW_USB_WRITE};
use crate::device::{DeviceProbe, ProbeOutcome, SocInfo};
use crate::error::RfError;

const TIMEOUT: Duration = Duration::from_millis(2000);

/// Allwinner FEL USB identity.
pub const FEL_VID: u16 = 0x1f3a;
pub const FEL_PID: u16 = 0xefe8;

/// True when a USB VID/PID pair is the Allwinner FEL device.
pub fn is_fel_device(vid: u16, pid: u16) -> bool {
    vid == FEL_VID && pid == FEL_PID
}

/// Real probe: scans the bus, and if a FEL device is present, attempts the
/// version handshake. Claim failure (driver not bound) maps to DetectedNoDriver.
pub struct UsbProbe;

impl DeviceProbe for UsbProbe {
    fn probe(&self) -> ProbeOutcome {
        match probe_once() {
            Ok(outcome) => outcome,
            // A bus error is treated as "nothing usable present" for the UI, but
            // logged so a silent grey state during hardware bring-up is diagnosable.
            Err(e) => {
                eprintln!("device probe error: {e}");
                ProbeOutcome::Absent
            }
        }
    }
}

fn probe_once() -> Result<ProbeOutcome, RfError> {
    let ctx = rusb::Context::new().map_err(|e| RfError::FelProtocolError(e.to_string()))?;
    let devices = ctx
        .devices()
        .map_err(|e| RfError::FelProtocolError(e.to_string()))?;

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

fn handshake<T: UsbContext>(device: &rusb::Device<T>) -> Result<SocInfo, RfError> {
    let handle = device
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
    let read_into = |buf: &mut [u8]| -> Result<usize, RfError> {
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
