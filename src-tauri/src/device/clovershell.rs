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
