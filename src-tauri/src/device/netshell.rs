use crate::error::RfError;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

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

/// Poll a TCP connect to `host:port` until it succeeds or `within` elapses.
/// Split out so tests can target an arbitrary port; production calls
/// `wait_for_ssh` with `SSH_PORT`.
pub fn wait_for_ssh_addr(host: &str, port: u16, within: Duration) -> Result<(), RfError> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| RfError::SshError(e.to_string()))?;
    let deadline = Instant::now() + within;
    loop {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(RfError::ShellNotFound);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Wait until the device's SSH port is reachable, or give up after `within`.
pub fn wait_for_ssh(host: &str, within: Duration) -> Result<(), RfError> {
    wait_for_ssh_addr(host, SSH_PORT, within)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn wait_for_ssh_ok_when_port_open() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // A listening socket accepts the connect via the backlog without an
        // explicit accept(), so no background thread is needed.
        let r = wait_for_ssh_addr("127.0.0.1", port, Duration::from_secs(2));
        assert!(r.is_ok(), "expected Ok, got {r:?}");
    }

    #[test]
    fn wait_for_ssh_errs_when_port_closed() {
        // Bind then drop to obtain a port nothing listens on.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let r = wait_for_ssh_addr("127.0.0.1", port, Duration::from_millis(300));
        assert!(matches!(r, Err(RfError::ShellNotFound)), "got {r:?}");
    }

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
