//! Minimal QMP (QEMU Machine Protocol) client over a unix socket.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::time::{Duration, Instant};

pub struct Qmp {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Qmp {
    /// Connects to the abstract socket `name` (retrying while QEMU starts)
    /// and negotiates capabilities.
    pub fn connect(name: &str, timeout: Duration) -> Result<Qmp> {
        let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
        let deadline = Instant::now() + timeout;
        let stream = loop {
            match UnixStream::connect_addr(&addr) {
                Ok(s) => break s,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(e) => return Err(e).context("connecting to QMP socket"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut q = Qmp {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
        };
        q.read_message()?; // greeting
        q.execute("qmp_capabilities", json!({}))?;
        Ok(q)
    }

    fn read_message(&mut self) -> Result<Value> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            bail!("QMP connection closed");
        }
        Ok(serde_json::from_str(&line)?)
    }

    /// Runs a command and returns its `return` value. Events are skipped.
    pub fn execute(&mut self, command: &str, arguments: Value) -> Result<Value> {
        let msg = json!({ "execute": command, "arguments": arguments });
        writeln!(self.writer, "{msg}")?;
        loop {
            let v = self.read_message()?;
            if v.get("event").is_some() {
                continue;
            }
            if let Some(err) = v.get("error") {
                bail!("QMP {command} failed: {err}");
            }
            return Ok(v.get("return").cloned().unwrap_or(Value::Null));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener};

    /// A fake QEMU: greets, accepts `qmp_capabilities`, sends an event, then
    /// answers one command.
    #[test]
    fn talks_to_qemu_over_an_abstract_socket() {
        let name = format!("relay-qmp-test-{}", std::process::id());
        let addr = SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        let listener = UnixListener::bind_addr(&addr).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut w = stream.try_clone().unwrap();
            let mut r = BufReader::new(stream);
            let mut line = String::new();
            writeln!(w, r#"{{"QMP": {{"version": {{}}, "capabilities": []}}}}"#).unwrap();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            writeln!(w, r#"{{"return": {{}}}}"#).unwrap();
            line.clear();
            r.read_line(&mut line).unwrap();
            assert!(line.contains("query-status"));
            writeln!(w, r#"{{"event": "RESUME"}}"#).unwrap();
            writeln!(w, r#"{{"return": {{"status": "running"}}}}"#).unwrap();
        });
        let mut q = Qmp::connect(&name, Duration::from_secs(5)).unwrap();
        let status = q.execute("query-status", serde_json::json!({})).unwrap();
        assert_eq!(status["status"], "running");
        server.join().unwrap();
    }
}
