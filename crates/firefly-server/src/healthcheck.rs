//! Built-in container health probe (`firefly-server --healthcheck`, #99).
//!
//! The Docker `HEALTHCHECK` used to shell out to `curl` — which the
//! deliberately slim runtime image does not contain, so every container
//! reported `unhealthy` forever while the server was perfectly fine: a
//! health signal that always lies is worse than none (FHA class
//! "misleading"). Instead of fattening the image, the thermometer is
//! built into the binary: the subcommand performs one plain-HTTP `GET
//! /health` against the *local* server and maps the outcome to the exit
//! code Docker expects (0 = healthy, 1 = not). No extra dependencies —
//! a raw `std::net::TcpStream` request is all `/health` needs.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Overall budget for one probe: connect + write + read. Docker's default
/// `--timeout` is 3 s; staying well under it means Docker sees OUR verdict
/// (exit code), never its own timeout kill.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Probe `http://127.0.0.1:{port}/health` once. `true` iff the server
/// answered `HTTP/1.1 200`. Every failure mode — refused connection,
/// timeout, garbage response — is `false`, never a panic: the probe's
/// verdict must be an exit code, not a crash.
pub fn probe_local_health(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT) else {
        return false;
    };
    if stream.set_read_timeout(Some(PROBE_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(PROBE_TIMEOUT)).is_err()
    {
        return false;
    }
    let request =
        format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    // The status line is all we need; a bounded read keeps a misbehaving
    // peer from stalling the probe.
    let mut buf = [0u8; 64];
    let mut filled = 0;
    while filled < buf.len() {
        match stream.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => {
                filled += n;
                if buf[..filled].windows(2).any(|w| w == b"\r\n") {
                    break;
                }
            }
            Err(_) => return false,
        }
    }
    let head = String::from_utf8_lossy(&buf[..filled]);
    head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A one-shot local HTTP server answering with `status_line`.
    fn serve_once(status_line: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    format!("{status_line}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                        .as_bytes(),
                );
            }
        });
        port
    }

    /// A healthy server (200) probes true. REQ: FR-OPS-010
    #[test]
    fn healthy_server_probes_true() {
        let port = serve_once("HTTP/1.1 200 OK");
        assert!(probe_local_health(port));
    }

    /// The probe measures for real (issue #99 acceptance: "der Check misst
    /// wirklich"): a failing endpoint and a dead port both probe false.
    /// REQ: FR-OPS-010
    #[test]
    fn unhealthy_and_absent_servers_probe_false() {
        let port = serve_once("HTTP/1.1 503 Service Unavailable");
        assert!(!probe_local_health(port));

        let dead = TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_port = dead.local_addr().unwrap().port();
        drop(dead); // port now closed
        assert!(!probe_local_health(dead_port));
    }
}
