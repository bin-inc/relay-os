//! Minimal QMP (QEMU Machine Protocol) client over a unix socket.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct Qmp {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Qmp {
    /// Connects to the socket at `socket` (retrying while QEMU starts) and
    /// negotiates capabilities.
    pub fn connect(socket: &Path, timeout: Duration) -> Result<Qmp> {
        let (_dir, path) = reachable(socket)?;
        let deadline = Instant::now() + timeout;
        let stream = loop {
            match UnixStream::connect(&path) {
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

/// A path to `socket` short enough for a Unix socket address (108 bytes)
/// however deep the checkout is: through the `/proc/self/fd` entry of its
/// open directory. The path works while the returned `File` is open.
pub fn reachable(socket: &Path) -> Result<(File, PathBuf)> {
    let dir = socket.parent().context("socket path without a directory")?;
    let name = socket.file_name().context("socket path without a name")?;
    let dir = File::open(dir).with_context(|| format!("opening {}", dir.display()))?;
    let path = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd())).join(name);
    Ok((dir, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::out_dir;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    /// A fake QEMU: greets, accepts `qmp_capabilities`, sends an event, then
    /// answers one command. Its socket is deeper than a Unix socket address
    /// can name.
    #[test]
    fn talks_to_qemu_over_a_socket_in_a_deep_directory() {
        let dir = out_dir().join("qmp-selftest").join("d".repeat(150));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("qmp.sock");
        let _ = std::fs::remove_file(&socket);
        assert!(socket.as_os_str().len() > 108);
        let (_dir, path) = reachable(&socket).unwrap();
        let listener = UnixListener::bind(&path).unwrap();
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
        let mut q = Qmp::connect(&socket, Duration::from_secs(5)).unwrap();
        let status = q.execute("query-status", serde_json::json!({})).unwrap();
        assert_eq!(status["status"], "running");
        server.join().unwrap();
    }
}
