#[allow(unused_imports)]
use crate::error::RfError;
#[allow(unused_imports)]
use std::time::Duration;

pub const DEVICE_IP: &str = "169.254.13.37";
pub const SSH_PORT: u16 = 22;

/// Result of running one command on the device.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

/// One unit of SSH channel output, decoupled from russh types so the
/// accumulation logic is testable without standing up an SSH server.
#[derive(Debug, Clone, PartialEq)]
pub enum ShellEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(i32),
}

/// Accumulate channel events into one ExecOutput. A run with no Exit event
/// reports -1 (mirrors the old clovershell default).
pub fn fold_events(events: impl IntoIterator<Item = ShellEvent>) -> ExecOutput {
    let mut out = ExecOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: -1,
    };
    for e in events {
        match e {
            ShellEvent::Stdout(d) => out.stdout.extend_from_slice(&d),
            ShellEvent::Stderr(d) => out.stderr.extend_from_slice(&d),
            ShellEvent::Exit(c) => out.exit_code = c,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_accumulates_stdout_chunks_and_exit() {
        let out = fold_events([
            ShellEvent::Stdout(b"Linux ".to_vec()),
            ShellEvent::Stdout(b"clover 4.4.0".to_vec()),
            ShellEvent::Exit(0),
        ]);
        assert_eq!(out.stdout, b"Linux clover 4.4.0");
        assert_eq!(out.stderr, b"");
        assert_eq!(out.exit_code, 0);
    }

    #[test]
    fn fold_captures_stderr_and_nonzero_exit() {
        let out = fold_events([
            ShellEvent::Stderr(b"nope".to_vec()),
            ShellEvent::Exit(1),
        ]);
        assert_eq!(out.stderr, b"nope");
        assert_eq!(out.exit_code, 1);
    }

    #[test]
    fn fold_defaults_exit_to_minus_one_when_absent() {
        let out = fold_events([ShellEvent::Stdout(b"x".to_vec())]);
        assert_eq!(out.exit_code, -1);
    }
}
