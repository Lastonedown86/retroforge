use std::cell::RefCell;
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

/// Real probe over rusb.
///
/// The FEL version handshake (open + claim interface + bulk exchange) is run
/// only **once** per connection; the resulting `SocInfo` is cached. While the
/// device stays present, later polls report `Connected` from the cache without
/// re-claiming the interface — repeatedly re-claiming a WinUSB device every poll
/// is unreliable and caused a green/amber flicker. The cache clears when the
/// device disappears, so a replug re-runs the handshake.
pub struct UsbProbe {
    cached: RefCell<Option<SocInfo>>,
}

impl UsbProbe {
    pub fn new() -> Self {
        Self {
            cached: RefCell::new(None),
        }
    }
}

impl Default for UsbProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceProbe for UsbProbe {
    fn probe(&self) -> ProbeOutcome {
        match probe_once(&self.cached) {
            Ok(outcome) => outcome,
            // A bus error is treated as "nothing usable present" for the UI, but
            // logged so a silent grey state during hardware bring-up is diagnosable.
            Err(e) => {
                eprintln!("device probe error: {e}");
                *self.cached.borrow_mut() = None;
                ProbeOutcome::Absent
            }
        }
    }
}

fn probe_once(cache: &RefCell<Option<SocInfo>>) -> Result<ProbeOutcome, RfError> {
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

        // Device present. If we already identified it this connection, report
        // from cache without touching the interface again.
        let cached = cache.borrow().clone();
        if let Some(soc) = cached {
            return Ok(ProbeOutcome::Connected(soc));
        }

        // First sighting: run the handshake once. Claim/transfer failure (e.g.
        // WinUSB not bound yet) maps to DetectedNoDriver and is retried next poll.
        return match handshake(&device) {
            Ok(soc) => {
                *cache.borrow_mut() = Some(soc.clone());
                Ok(ProbeOutcome::Connected(soc))
            }
            Err(e) => {
                eprintln!("FEL handshake failed: {e}");
                Ok(ProbeOutcome::DetectedNoDriver)
            }
        };
    }

    // No FEL device on the bus — forget any prior identity.
    *cache.borrow_mut() = None;
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
    // A single bulk transfer may return only the first packet, so loop until the
    // whole buffer is filled (mirrors the reference ReadFromUSB accumulation).
    let read_full = |buf: &mut [u8]| -> Result<(), RfError> {
        let mut pos = 0;
        while pos < buf.len() {
            let n = handle
                .read_bulk(ep_in, &mut buf[pos..], TIMEOUT)
                .map_err(|e| RfError::FelProtocolError(e.to_string()))?;
            if n == 0 {
                return Err(RfError::FelProtocolError(format!(
                    "zero-length bulk read at {pos}/{}",
                    buf.len()
                )));
            }
            pos += n;
        }
        Ok(())
    };

    // 1. FelWrite(FEL_VERIFY_DEVICE): AWUC WRITE envelope + 16-byte request,
    //    then the 13-byte AWUS acknowledgement.
    let fel_req = fel::fel_request(AW_FEL_VERSION, 0, 0);
    w(&fel::aw_usb_request(AW_USB_WRITE, fel_req.len() as u32))?;
    w(&fel_req)?;
    let mut status = [0u8; 13];
    read_full(&mut status)?;

    // 2. FelRead(32): AWUC READ envelope + 32-byte version payload + AWUS ack.
    w(&fel::aw_usb_request(AW_USB_READ, 32))?;
    let mut version = [0u8; 32];
    read_full(&mut version)?;
    read_full(&mut status)?;

    // 3. FelRead(8): drain the trailing FEL status. The reference VerifyDevice
    //    issues this second read; omitting it leaves bytes in the bulk pipe and
    //    desyncs the endpoint, so a later exchange reads stale/zero data.
    w(&fel::aw_usb_request(AW_USB_READ, 8))?;
    let mut fel_status = [0u8; 8];
    read_full(&mut fel_status)?;
    read_full(&mut status)?;

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
