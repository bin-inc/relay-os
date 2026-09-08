use uart_16550::{Config, Uart16550};

const COM1: u16 = 0x3f8;

pub fn write(bytes: &[u8]) {
    // SAFETY: COM1 is the PC-compatible serial port selected by the QEMU harness;
    // kernel entry has exclusive hardware ownership after ExitBootServices.
    let Ok(mut serial) = (unsafe { Uart16550::new_port(COM1) }) else {
        return;
    };
    if serial.init(Config::default()).is_ok() {
        serial.send_bytes_exact(bytes);
    }
}
