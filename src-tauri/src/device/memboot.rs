use crate::device::fel;
use crate::error::RfError;

/// Low-level FEL byte channel. `FelTransport` implements this against real USB;
/// tests use a recording mock. Everything above this trait is hardware-free.
pub trait FelIo {
    fn fel_write(&mut self, payload: &[u8]) -> Result<(), RfError>;
    fn fel_read(&mut self, len: usize) -> Result<Vec<u8>, RfError>;
}

fn status_ok(status: &[u8]) -> bool {
    status.get(4).copied() == Some(0)
}

/// Write `data` to device memory starting at `addr`, chunked at MAX_BULK,
/// padded up to a 4-byte boundary. Mirrors the reference WriteMemory.
pub fn write_memory<T: FelIo>(io: &mut T, addr: u32, data: &[u8]) -> Result<(), RfError> {
    let mut buf = data.to_vec();
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
    let mut pos = 0usize;
    while pos < buf.len() {
        let chunk_len = (buf.len() - pos).min(fel::MAX_BULK);
        io.fel_write(&fel::fel_request(
            fel::FEL_DOWNLOAD,
            addr + pos as u32,
            chunk_len as u32,
        ))?;
        io.fel_write(&buf[pos..pos + chunk_len])?;
        let status = io.fel_read(8)?;
        if !status_ok(&status) {
            return Err(RfError::MemoryWriteFailed(format!(
                "bad status at {:#x}",
                addr + pos as u32
            )));
        }
        pos += chunk_len;
    }
    Ok(())
}

/// Read `len` bytes from device memory at `addr` (4-byte padded), chunked.
pub fn read_memory<T: FelIo>(io: &mut T, addr: u32, len: u32) -> Result<Vec<u8>, RfError> {
    let len = (len + 3) & !3;
    let mut out = Vec::with_capacity(len as usize);
    let mut addr = addr;
    let mut remaining = len;
    while remaining > 0 {
        let l = remaining.min(fel::MAX_BULK as u32);
        io.fel_write(&fel::fel_request(fel::FEL_UPLOAD, addr, l))?;
        out.extend_from_slice(&io.fel_read(l as usize)?);
        let status = io.fel_read(8)?;
        if !status_ok(&status) {
            return Err(RfError::MemoryReadFailed(format!("bad status at {addr:#x}")));
        }
        remaining -= l;
        addr += l;
    }
    Ok(out)
}

/// Execute code at `addr` (FEL_RUN). No status follows; the device runs.
pub fn exec<T: FelIo>(io: &mut T, addr: u32) -> Result<(), RfError> {
    io.fel_write(&fel::fel_request(fel::FEL_RUN, addr, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Records every fel_write payload; returns scripted (or zero) reads.
    pub(super) struct MockFelIo {
        pub writes: Vec<Vec<u8>>,
        pub reads: VecDeque<Vec<u8>>,
    }
    impl MockFelIo {
        pub fn new() -> Self {
            Self {
                writes: Vec::new(),
                reads: VecDeque::new(),
            }
        }
    }
    impl FelIo for MockFelIo {
        fn fel_write(&mut self, payload: &[u8]) -> Result<(), RfError> {
            self.writes.push(payload.to_vec());
            Ok(())
        }
        fn fel_read(&mut self, len: usize) -> Result<Vec<u8>, RfError> {
            // Default read is zeros (status byte[4]==0 => ok).
            Ok(self.reads.pop_front().unwrap_or_else(|| vec![0u8; len]))
        }
    }

    #[test]
    fn write_memory_single_chunk_issues_msg_then_data_then_status() {
        let mut io = MockFelIo::new();
        write_memory(&mut io, 0x4740_0000, &[0xaa, 0xbb, 0xcc, 0xdd]).unwrap();
        // Two writes: the FEL_DOWNLOAD message, then the 4-byte payload.
        assert_eq!(io.writes.len(), 2);
        assert_eq!(&io.writes[0][0..4], &[0x01, 0x01, 0x00, 0x00]); // FEL_DOWNLOAD
        assert_eq!(&io.writes[0][4..8], &0x4740_0000u32.to_le_bytes());
        assert_eq!(&io.writes[0][8..12], &4u32.to_le_bytes());
        assert_eq!(io.writes[1], vec![0xaa, 0xbb, 0xcc, 0xdd]);
    }

    #[test]
    fn write_memory_chunks_above_max_bulk() {
        let mut io = MockFelIo::new();
        let data = vec![0u8; fel::MAX_BULK + 0x8000]; // 1.5 chunks
        write_memory(&mut io, fel::TRANSFER_BASE, &data).unwrap();
        // chunk1 msg+data, chunk2 msg+data = 4 writes.
        assert_eq!(io.writes.len(), 4);
        assert_eq!(&io.writes[0][4..8], &fel::TRANSFER_BASE.to_le_bytes());
        assert_eq!(
            &io.writes[2][4..8],
            &(fel::TRANSFER_BASE + fel::MAX_BULK as u32).to_le_bytes()
        );
    }

    #[test]
    fn write_memory_errors_on_bad_status() {
        let mut io = MockFelIo::new();
        io.reads.push_back(vec![0xff, 0xff, 0, 0, 1, 0, 0, 0]); // State=1
        let err = write_memory(&mut io, 0x4740_0000, &[1, 2, 3, 4]).unwrap_err();
        assert!(matches!(err, RfError::MemoryWriteFailed(_)));
    }

    #[test]
    fn exec_issues_fel_run() {
        let mut io = MockFelIo::new();
        exec(&mut io, 0x4700_0000).unwrap();
        assert_eq!(io.writes.len(), 1);
        assert_eq!(&io.writes[0][0..4], &[0x02, 0x01, 0x00, 0x00]); // FEL_RUN
        assert_eq!(&io.writes[0][4..8], &0x4700_0000u32.to_le_bytes());
    }
}
