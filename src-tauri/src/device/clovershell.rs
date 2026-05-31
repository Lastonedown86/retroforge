use std::time::Duration;

use rusb::{Direction, TransferType, UsbContext};

use crate::device::usb::is_fel_device;
use crate::error::RfError;

// Clovershell command bytes.
pub const CMD_PING: u8 = 0;
pub const CMD_PONG: u8 = 1;
pub const CMD_SHELL_KILL_ALL: u8 = 8;
pub const CMD_EXEC_NEW_REQ: u8 = 9;
pub const CMD_EXEC_NEW_RESP: u8 = 10;
pub const CMD_EXEC_STDOUT: u8 = 13;
pub const CMD_EXEC_STDERR: u8 = 14;
pub const CMD_EXEC_RESULT: u8 = 15;
pub const CMD_EXEC_KILL_ALL: u8 = 17;

/// One clovershell packet.
#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub cmd: u8,
    pub arg: u8,
    pub data: Vec<u8>,
}

/// The 4-byte clovershell header for a payload of `len` bytes.
pub fn header(cmd: u8, arg: u8, len: usize) -> [u8; 4] {
    [cmd, arg, (len & 0xFF) as u8, ((len >> 8) & 0xFF) as u8]
}

/// A framed clovershell byte channel. The rusb transport implements this; tests
/// use a recording/replaying mock.
pub trait ClovershellIo {
    fn write_packet(&mut self, cmd: u8, arg: u8, data: &[u8]) -> Result<(), RfError>;
    fn read_packet(&mut self) -> Result<Packet, RfError>;
}

/// Result of running a command on the device.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

/// Liveness check: send PING, wait for PONG (bounded by read timeouts and a
/// packet cap so a chatty device cannot loop forever).
pub fn ping<T: ClovershellIo>(io: &mut T) -> Result<(), RfError> {
    io.write_packet(CMD_PING, 0, &[])?;
    for _ in 0..64 {
        if io.read_packet()?.cmd == CMD_PONG {
            return Ok(());
        }
    }
    Err(RfError::ClovershellProtocol("no PONG after PING".into()))
}

/// Run a command, accumulating stdout/stderr until the RESULT packet carries the
/// exit code (a single byte).
pub fn exec<T: ClovershellIo>(io: &mut T, command: &str) -> Result<ExecOutput, RfError> {
    io.write_packet(CMD_EXEC_NEW_REQ, 0, command.as_bytes())?;
    let mut out = ExecOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: -1,
    };
    loop {
        let p = io.read_packet()?;
        match p.cmd {
            CMD_EXEC_STDOUT => out.stdout.extend_from_slice(&p.data),
            CMD_EXEC_STDERR => out.stderr.extend_from_slice(&p.data),
            CMD_EXEC_RESULT => {
                out.exit_code = *p
                    .data
                    .first()
                    .ok_or_else(|| RfError::ClovershellProtocol("empty exec result".into()))?
                    as i32;
                return Ok(out);
            }
            // NEW_RESP assigns the exec id; PONG/others are not interesting here.
            _ => {}
        }
    }
}

const TIMEOUT: Duration = Duration::from_millis(3000);
pub const CLV_EP_IN: u8 = 0x81;
pub const CLV_EP_OUT: u8 = 0x01;

/// An open, interface-claimed clovershell device with a small read buffer so
/// `read_packet` can split multi-packet bulk reads and reassemble partial ones.
pub struct ClovershellTransport {
    handle: rusb::DeviceHandle<rusb::Context>,
    ep_in: u8,
    ep_out: u8,
    rbuf: Vec<u8>,
}

impl ClovershellTransport {
    /// Open the clovershell device (same VID/PID as FEL, but endpoints 0x81/0x01).
    /// Errors with ShellNotFound if no matching device is present.
    pub fn open() -> Result<Self, RfError> {
        let ctx = rusb::Context::new().map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
        let devices = ctx
            .devices()
            .map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
        for device in devices.iter() {
            let Ok(desc) = device.device_descriptor() else {
                continue;
            };
            // Same VID/PID as FEL; the clovershell device is told apart by its
            // bulk endpoints (IN 0x81 vs FEL's 0x82), checked below.
            if !is_fel_device(desc.vendor_id(), desc.product_id()) {
                continue;
            }
            let Ok(handle) = device.open() else { continue };
            let _ = handle.set_auto_detach_kernel_driver(true);
            if handle.claim_interface(0).is_err() {
                continue;
            }
            let (ep_in, ep_out) = match clv_endpoints(&device) {
                Some(eps) => eps,
                None => continue,
            };
            // Only the clovershell interface exposes IN 0x81 (FEL uses 0x82).
            if ep_in != CLV_EP_IN || ep_out != CLV_EP_OUT {
                continue;
            }
            let mut t = Self {
                handle,
                ep_in,
                ep_out,
                rbuf: Vec::new(),
            };
            t.kill_all_sessions()?;
            t.drain();
            return Ok(t);
        }
        Err(RfError::ShellNotFound)
    }

    /// Clear stale shell/exec sessions on the device, mirroring the reference.
    fn kill_all_sessions(&mut self) -> Result<(), RfError> {
        self.write_all(&header(CMD_SHELL_KILL_ALL, 0, 0))?;
        self.write_all(&header(CMD_EXEC_KILL_ALL, 0, 0))?;
        Ok(())
    }

    /// Discard any buffered/in-flight input left from a previous session.
    fn drain(&mut self) {
        self.rbuf.clear();
        let mut scratch = [0u8; 4096];
        while let Ok(n) = self
            .handle
            .read_bulk(self.ep_in, &mut scratch, Duration::from_millis(50))
        {
            if n == 0 {
                break;
            }
        }
    }

    fn write_all(&self, data: &[u8]) -> Result<(), RfError> {
        let mut pos = 0;
        while pos < data.len() {
            let n = self
                .handle
                .write_bulk(self.ep_out, &data[pos..], TIMEOUT)
                .map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
            if n == 0 {
                return Err(RfError::ClovershellProtocol("zero-length write".into()));
            }
            pos += n;
        }
        Ok(())
    }

    /// True when `rbuf` already holds at least one complete packet.
    fn has_full_packet(&self) -> Option<usize> {
        if self.rbuf.len() < 4 {
            return None;
        }
        let len = self.rbuf[2] as usize | ((self.rbuf[3] as usize) << 8);
        if self.rbuf.len() >= 4 + len {
            Some(len)
        } else {
            None
        }
    }
}

/// Find the first interface's bulk IN/OUT endpoint addresses.
fn clv_endpoints<T: UsbContext>(device: &rusb::Device<T>) -> Option<(u8, u8)> {
    let config = device.active_config_descriptor().ok()?;
    let (mut ep_in, mut ep_out) = (0u8, 0u8);
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
    Some((ep_in, ep_out))
}

impl ClovershellIo for ClovershellTransport {
    fn write_packet(&mut self, cmd: u8, arg: u8, data: &[u8]) -> Result<(), RfError> {
        self.write_all(&header(cmd, arg, data.len()))?;
        if !data.is_empty() {
            self.write_all(data)?;
        }
        Ok(())
    }

    fn read_packet(&mut self) -> Result<Packet, RfError> {
        let len = loop {
            if let Some(len) = self.has_full_packet() {
                break len;
            }
            let mut scratch = [0u8; 65536];
            let n = self
                .handle
                .read_bulk(self.ep_in, &mut scratch, TIMEOUT)
                .map_err(|e| RfError::ClovershellProtocol(e.to_string()))?;
            if n == 0 {
                return Err(RfError::ClovershellProtocol("zero-length read".into()));
            }
            self.rbuf.extend_from_slice(&scratch[..n]);
        };
        let cmd = self.rbuf[0];
        let arg = self.rbuf[1];
        let data = self.rbuf[4..4 + len].to_vec();
        self.rbuf.drain(0..4 + len);
        Ok(Packet { cmd, arg, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    pub(super) struct MockClovershellIo {
        pub writes: Vec<Packet>,
        pub reads: VecDeque<Packet>,
    }
    impl MockClovershellIo {
        fn new(reads: Vec<Packet>) -> Self {
            Self {
                writes: Vec::new(),
                reads: reads.into(),
            }
        }
    }
    impl ClovershellIo for MockClovershellIo {
        fn write_packet(&mut self, cmd: u8, arg: u8, data: &[u8]) -> Result<(), RfError> {
            self.writes.push(Packet {
                cmd,
                arg,
                data: data.to_vec(),
            });
            Ok(())
        }
        fn read_packet(&mut self) -> Result<Packet, RfError> {
            self.reads
                .pop_front()
                .ok_or_else(|| RfError::ClovershellProtocol("no more packets".into()))
        }
    }

    fn pkt(cmd: u8, data: &[u8]) -> Packet {
        Packet {
            cmd,
            arg: 1,
            data: data.to_vec(),
        }
    }

    #[test]
    fn header_encodes_len_little_endian() {
        assert_eq!(header(CMD_EXEC_NEW_REQ, 0, 0x0102), [9, 0, 0x02, 0x01]);
    }

    #[test]
    fn ping_returns_on_pong() {
        let mut io = MockClovershellIo::new(vec![pkt(CMD_PONG, &[])]);
        ping(&mut io).unwrap();
        assert_eq!(io.writes[0].cmd, CMD_PING);
    }

    #[test]
    fn exec_accumulates_stdout_and_exit() {
        let mut io = MockClovershellIo::new(vec![
            pkt(CMD_EXEC_NEW_RESP, b"uname -a"),
            pkt(CMD_EXEC_STDOUT, b"Linux "),
            pkt(CMD_EXEC_STDOUT, b"clover 4.4.0"),
            pkt(CMD_EXEC_RESULT, &[0]),
        ]);
        let out = exec(&mut io, "uname -a").unwrap();
        assert_eq!(out.stdout, b"Linux clover 4.4.0");
        assert_eq!(out.exit_code, 0);
        assert_eq!(io.writes[0].cmd, CMD_EXEC_NEW_REQ);
        assert_eq!(io.writes[0].data, b"uname -a");
    }

    #[test]
    fn exec_reports_nonzero_exit_and_stderr() {
        let mut io = MockClovershellIo::new(vec![
            pkt(CMD_EXEC_STDERR, b"nope"),
            pkt(CMD_EXEC_RESULT, &[1]),
        ]);
        let out = exec(&mut io, "false").unwrap();
        assert_eq!(out.stderr, b"nope");
        assert_eq!(out.exit_code, 1);
    }
}
