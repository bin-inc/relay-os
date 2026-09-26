//! Minimal QMP (QEMU Machine Protocol) client over a unix socket.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

pub struct Qmp {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Qmp {
    /// Connects (retrying while QEMU starts) and negotiates capabilities.
    pub fn connect(socket: &Path, timeout: Duration) -> Result<Qmp> {
        let deadline = Instant::now() + timeout;
        let stream = loop {
            match UnixStream::connect(socket) {
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
