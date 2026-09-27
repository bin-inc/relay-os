//! COM1 (16550 UART at 0x3F8). Present in QEMU, absent on the NUC; the probe
//! makes every call a no-op when there is no UART. Output mirrors the
//! console; input joins the keyboard's (spec §7.2), polled, without
//! interrupts.

use spin::Mutex;
use x86_64::instructions::port::Port;

const COM1: u16 = 0x3F8;

pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    /// Initialises 115200 8N1 and returns `None` if no UART answers.
    ///
    /// # Safety
    /// Performs port I/O on COM1.
    pub unsafe fn probe(base: u16) -> Option<SerialPort> {
        unsafe {
            let mut scratch: Port<u8> = Port::new(base + 7);
            scratch.write(0xAE);
            if scratch.read() != 0xAE {
                return None;
            }
            Port::<u8>::new(base + 1).write(0x00); // no interrupts
            Port::<u8>::new(base + 3).write(0x80); // DLAB on
            Port::<u8>::new(base).write(0x01); // divisor 1 = 115200 baud
            Port::<u8>::new(base + 1).write(0x00);
            Port::<u8>::new(base + 3).write(0x03); // 8N1, DLAB off
            Port::<u8>::new(base + 2).write(0xC7); // FIFO on, cleared
            Port::<u8>::new(base + 4).write(0x03); // DTR + RTS
        }
        Some(SerialPort { base })
    }

    fn write_byte(&mut self, b: u8) {
        unsafe {
            let mut lsr: Port<u8> = Port::new(self.base + 5);
            // Wait for the transmit holding register, but never forever.
            for _ in 0..100_000 {
                if lsr.read() & 0x20 != 0 {
                    break;
                }
            }
            Port::<u8>::new(self.base).write(b);
        }
    }

    /// A received byte, if one is waiting (Line Status bit 0, Data Ready).
    fn read_byte(&mut self) -> Option<u8> {
        unsafe {
            if Port::<u8>::new(self.base + 5).read() & 0x01 == 0 {
                return None;
            }
            Some(Port::<u8>::new(self.base).read())
        }
    }

    /// Writes bytes, translating `\n` to `\r\n`.
    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
    }
}

pub static SERIAL: Mutex<Option<SerialPort>> = Mutex::new(None);

pub fn init() {
    *SERIAL.lock() = unsafe { SerialPort::probe(COM1) };
}

/// The next byte received on COM1, if any. Never waits; `None` on machines
/// without a UART (the NUC).
pub fn read_byte() -> Option<u8> {
    SERIAL.lock().as_mut().and_then(|s| s.read_byte())
}

pub fn write(bytes: &[u8]) {
    if let Some(s) = SERIAL.lock().as_mut() {
        s.write(bytes);
    }
}
