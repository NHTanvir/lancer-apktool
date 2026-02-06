//! Minimal local HTTP server for cross-tool Lancer integration.
//!
//! Exposes `GET /v1/state` and writes the chosen port to
//! `%TEMP%/lancer-apktool.port` so the Lancer mothership can discover us.
//! Deliberately tiny (no async runtime, no deps) — it answers a single
//! line-based request per connection.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

/// Start the discovery server on a free loopback port. Best-effort: returns the
/// bound port, or `None` if binding failed.
pub fn start() -> Option<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();

    // Advertise the port for the Lancer hub.
    let port_file = std::env::temp_dir().join("lancer-apktool.port");
    let _ = std::fs::write(&port_file, port.to_string());

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let body = route(&req);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    Some(port)
}

fn route(req: &str) -> String {
    let path = req.split_whitespace().nth(1).unwrap_or("/");
    match path {
        p if p.starts_with("/v1/state") => {
            format!("{{\"tool\":\"lancer-apktool\",\"version\":\"{}\",\"jobs\":[]}}", env!("CARGO_PKG_VERSION"))
        }
        p if p.starts_with("/v1/export") => "{\"items\":[]}".to_string(),
        _ => "{\"ok\":true}".to_string(),
    }
}
