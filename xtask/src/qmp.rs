use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub fn screendump(socket: &Path, output: &Path) -> Result<String, String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("connect to QMP {}: {error}", socket.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|error| format!("set QMP timeout: {error}"))?;
    let mut response = [0_u8; 1024];
    let greeting_len = stream.read(&mut response).unwrap_or(0);
    let greeting = String::from_utf8_lossy(&response[..greeting_len]).into_owned();
    stream
        .write_all(b"{\"execute\":\"qmp_capabilities\"}\n")
        .map_err(|error| format!("enable QMP capabilities: {error}"))?;
    let capabilities_len = stream.read(&mut response).unwrap_or(0);
    let capabilities = String::from_utf8_lossy(&response[..capabilities_len]).into_owned();
    let command = format!(
        "{{\"execute\":\"screendump\",\"arguments\":{{\"filename\":\"{}\"}}}}\n",
        output.display()
    );
    stream
        .write_all(command.as_bytes())
        .map_err(|error| format!("request QMP screendump: {error}"))?;
    let screendump_len = stream.read(&mut response).unwrap_or(0);
    let screendump = String::from_utf8_lossy(&response[..screendump_len]);
    Ok(format!("{greeting}{capabilities}{screendump}"))
}
