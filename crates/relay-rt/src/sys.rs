//! System calls (spec §7.3). Each wrapper passes its arguments as the ABI
//! says and decodes the result into a value or an error number.

use core::fmt;
use relay_abi::{Call, decode};

#[cfg(all(target_arch = "x86_64", target_os = "none"))]
use crate::arch::syscall;

/// Off Relay OS (the host tests) there is no kernel to call.
#[cfg(not(all(target_arch = "x86_64", target_os = "none")))]
unsafe fn syscall(_call: Call, _args: [u64; 6]) -> u64 {
    unimplemented!("system calls exist only on Relay OS")
}

/// Writes some of `bytes` to `fd`; returns how many.
pub fn write(fd: u32, bytes: &[u8]) -> Result<usize, u16> {
    let args = [
        u64::from(fd),
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
        0,
        0,
    ];
    decode(unsafe { syscall(Call::Write, args) }).map(|n| n as usize)
}

/// Writes all of `bytes` to `fd`, however many calls that takes.
pub fn write_all(fd: u32, mut bytes: &[u8]) -> Result<(), u16> {
    while !bytes.is_empty() {
        match write(fd, bytes)? {
            0 => return Err(relay_abi::errno::EIO),
            n => bytes = &bytes[n.min(bytes.len())..],
        }
    }
    Ok(())
}

/// The wall clock and the time since the machine started.
pub fn time() -> Result<relay_abi::Time, u16> {
    let mut t = relay_abi::Time::default();
    let args = [&raw mut t as u64, 0, 0, 0, 0, 0];
    decode(unsafe { syscall(Call::Time, args) })?;
    Ok(t)
}

/// Blocks the program for `ms` milliseconds.
pub fn sleep(ms: u64) {
    unsafe { syscall(Call::Sleep, [ms, 0, 0, 0, 0, 0]) };
}

/// Ends the program with status `code`.
pub fn exit(code: u8) -> ! {
    unsafe { syscall(Call::Exit, [u64::from(code), 0, 0, 0, 0, 0]) };
    // `exit` does not return.
    loop {
        core::hint::spin_loop();
    }
}

/// A file descriptor to `write!` to: `Fd(1)` is standard output, `Fd(2)`
/// standard error.
pub struct Fd(pub u32);

impl fmt::Write for Fd {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_all(self.0, s.as_bytes()).map_err(|_| fmt::Error)
    }
}
