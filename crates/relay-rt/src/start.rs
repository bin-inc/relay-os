//! From `_start` to the program's `main`, and the panic handler.

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// The program's name for messages, set before `main` runs.
static NAME_PTR: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static NAME_LEN: AtomicUsize = AtomicUsize::new(0);

fn set_name(name: &'static [u8]) {
    NAME_PTR.store(name.as_ptr().cast_mut(), Ordering::Relaxed);
    NAME_LEN.store(name.len(), Ordering::Relaxed);
}

/// The program's name, as `Args::name` gave it; empty before `start`.
pub fn name() -> &'static [u8] {
    let p = NAME_PTR.load(Ordering::Relaxed);
    if p.is_null() {
        return b"";
    }
    // SAFETY: set from a `&'static [u8]` in `set_name`.
    unsafe { core::slice::from_raw_parts(p, NAME_LEN.load(Ordering::Relaxed)) }
}

/// `<name>: panicked at <file>:<line>:<column>: <message>`, as one line.
pub fn panic_message(
    w: &mut impl Write,
    name: &[u8],
    location: Option<(&str, u32, u32)>,
    message: impl fmt::Display,
) -> fmt::Result {
    for chunk in name.utf8_chunks() {
        w.write_str(chunk.valid())?;
        if !chunk.invalid().is_empty() {
            w.write_char(char::REPLACEMENT_CHARACTER)?;
        }
    }
    w.write_str(": panicked")?;
    if let Some((file, line, column)) = location {
        write!(w, " at {file}:{line}:{column}")?;
    }
    writeln!(w, ": {message}")
}

/// What only exists in a program on Relay OS: the start and the panic
/// handler.
#[cfg(target_os = "none")]
mod on_relay {
    use super::{name, panic_message, set_name};
    use crate::args::Args;
    use crate::sys;

    /// Runs the program: the arguments the kernel laid out at `ptr` (`len`
    /// bytes, `count` arguments), then `main`, whose result is the exit status.
    ///
    /// # Safety
    /// `ptr` and `len` must describe memory that stays readable for the whole
    /// run (the kernel puts the arguments above the stack).
    pub unsafe extern "sysv64" fn start(ptr: *const u8, len: usize, count: usize) -> ! {
        unsafe extern "Rust" {
            /// Defined by `relay_rt::main!`.
            fn __relay_main(args: Args) -> u8;
        }
        let bytes: &'static [u8] = if ptr.is_null() {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(ptr, len) }
        };
        let args = Args::new(bytes, count);
        set_name(args.name());
        let code = unsafe { __relay_main(args) };
        sys::exit(code)
    }

    /// A panic ends the program with status 101, as Rust programs do.
    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        let location = info.location().map(|l| (l.file(), l.line(), l.column()));
        let _ = panic_message(&mut sys::Fd(2), name(), location, info.message());
        sys::exit(101)
    }
}

#[cfg(target_os = "none")]
pub(crate) use on_relay::start;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_reads_as_one_line_naming_the_program() {
        let mut s = String::new();
        panic_message(&mut s, b"t-args", Some(("src/main.rs", 7, 5)), "boom").unwrap();
        assert_eq!(s, "t-args: panicked at src/main.rs:7:5: boom\n");
        let mut s = String::new();
        panic_message(&mut s, b"a\xffb", None, 42).unwrap();
        assert_eq!(s, "a\u{fffd}b: panicked: 42\n");
    }

    #[test]
    fn the_name_is_empty_until_set() {
        assert_eq!(name(), b"");
        set_name(b"prog");
        assert_eq!(name(), b"prog");
    }
}
