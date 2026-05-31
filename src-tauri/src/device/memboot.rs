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
    while !buf.len().is_multiple_of(4) {
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
#[allow(dead_code)] // reserved for slice 3 (NAND backup); not yet called by production
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

/// Bring up DRAM: load the fes1 blob into SRAM and execute it.
pub fn init_dram<T: FelIo>(io: &mut T, fes1: &[u8]) -> Result<(), RfError> {
    write_memory(io, fel::FES1_BASE, fes1)?;
    exec(io, fel::FES1_BASE)
}

/// Load U-Boot, patch a command string at its `bootcmd=` marker, and execute it.
pub fn run_uboot_cmd<T: FelIo>(io: &mut T, uboot: &[u8], cmd: &str) -> Result<(), RfError> {
    let marker = b"bootcmd=";
    let offset = uboot
        .windows(marker.len())
        .position(|w| w == marker)
        .map(|i| i + marker.len())
        .ok_or_else(|| RfError::ExecFailed("uboot 'bootcmd=' marker not found".into()))?;
    write_memory(io, fel::UBOOT_BASE, uboot)?;
    let mut cmd_buf = cmd.as_bytes().to_vec();
    cmd_buf.push(0); // NUL terminator
    write_memory(io, fel::UBOOT_BASE + offset as u32, &cmd_buf)?;
    exec(io, fel::UBOOT_BASE)
}

/// Full memboot: init DRAM, stage the boot image, then boota it via U-Boot.
/// This orchestration fn is the unit-tested reference path; the Tauri command
/// calls the individual steps to emit progress events between them.
#[allow(dead_code)] // unit-tested reference path; Tauri command calls steps individually
pub fn memboot<T: FelIo>(
    io: &mut T,
    fes1: &[u8],
    uboot: &[u8],
    boot_img: &[u8],
) -> Result<(), RfError> {
    init_dram(io, fes1)?;

    // Pad the boot image up to a sector boundary; bound by transfer window.
    let padded = boot_img.len().div_ceil(fel::SECTOR_SIZE) * fel::SECTOR_SIZE;
    if padded as u32 > fel::TRANSFER_MAX_SIZE {
        return Err(RfError::ExecFailed(format!(
            "boot image too large: {padded} > {}",
            fel::TRANSFER_MAX_SIZE
        )));
    }
    let mut kernel = boot_img.to_vec();
    kernel.resize(padded, 0);
    write_memory(io, fel::TRANSFER_BASE, &kernel)?;

    let cmd = format!("boota {:x}", fel::TRANSFER_BASE);
    run_uboot_cmd(io, uboot, &cmd)
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

    #[test]
    fn init_dram_writes_fes1_then_execs_it() {
        let mut io = MockFelIo::new();
        init_dram(&mut io, &[0x11, 0x22, 0x33, 0x44]).unwrap();
        // write msg + data, then exec msg.
        assert_eq!(io.writes.len(), 3);
        assert_eq!(&io.writes[0][4..8], &fel::FES1_BASE.to_le_bytes()); // download to 0x2000
        assert_eq!(&io.writes[2][0..4], &[0x02, 0x01, 0x00, 0x00]); // FEL_RUN
        assert_eq!(&io.writes[2][4..8], &fel::FES1_BASE.to_le_bytes());
    }

    #[test]
    fn run_uboot_cmd_patches_command_at_marker() {
        let mut io = MockFelIo::new();
        // uboot blob: 4 bytes, then "bootcmd=", then 16 spare bytes.
        let mut uboot = vec![0u8; 4];
        uboot.extend_from_slice(b"bootcmd=");
        uboot.extend_from_slice(&[0u8; 16]);
        let marker_off = 4 + b"bootcmd=".len(); // 12
        run_uboot_cmd(&mut io, &uboot, "boota 47400000").unwrap();
        // writes: uboot msg, uboot data, cmd msg, cmd data, exec msg.
        assert_eq!(io.writes.len(), 5);
        assert_eq!(&io.writes[0][4..8], &fel::UBOOT_BASE.to_le_bytes());
        // command is written at UBOOT_BASE + marker offset.
        assert_eq!(
            &io.writes[2][4..8],
            &(fel::UBOOT_BASE + marker_off as u32).to_le_bytes()
        );
        assert_eq!(&io.writes[3][0..14], b"boota 47400000");
        assert_eq!(io.writes[3][14], 0); // NUL
        assert_eq!(&io.writes[4][0..4], &[0x02, 0x01, 0x00, 0x00]); // FEL_RUN
        assert_eq!(&io.writes[4][4..8], &fel::UBOOT_BASE.to_le_bytes());
    }

    #[test]
    fn run_uboot_cmd_errors_without_marker() {
        let mut io = MockFelIo::new();
        let err = run_uboot_cmd(&mut io, &[0u8; 64], "boota 0").unwrap_err();
        assert!(matches!(err, RfError::ExecFailed(_)));
    }

    #[test]
    fn memboot_runs_dram_then_image_then_boota() {
        let mut io = MockFelIo::new();
        let mut uboot = vec![0u8; 4];
        uboot.extend_from_slice(b"bootcmd=");
        uboot.extend_from_slice(&[0u8; 16]);
        memboot(&mut io, &[0xaa; 8], &uboot, &[0xbb; 16]).unwrap();
        // First op is DRAM init: download to FES1_BASE.
        assert_eq!(&io.writes[0][4..8], &fel::FES1_BASE.to_le_bytes());
        // The boot image is staged at TRANSFER_BASE somewhere in the sequence.
        let staged = io
            .writes
            .iter()
            .any(|w| w.len() >= 8 && w[0..4] == [0x01, 0x01, 0x00, 0x00] && w[4..8] == fel::TRANSFER_BASE.to_le_bytes());
        assert!(staged, "boot image must be written to TRANSFER_BASE");
        // The final write is the FEL_RUN of U-Boot.
        let last = io.writes.last().unwrap();
        assert_eq!(&last[0..4], &[0x02, 0x01, 0x00, 0x00]);
        assert_eq!(&last[4..8], &fel::UBOOT_BASE.to_le_bytes());
    }
}
