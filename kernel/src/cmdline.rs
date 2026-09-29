//! Kernel command line (`\EFI\RELAY\cmdline`), space-separated words.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanicTest {
    /// A page fault before the console exists.
    Early,
    PageFault,
    InvalidOpcode,
    Panic,
    StackOverflow,
    /// The kernel reads a program's page (SMAP must stop it).
    UserRead,
    /// The kernel runs a program's code (SMEP must stop it).
    UserExec,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cmdline {
    /// `test=1`: running under the automated QEMU tests.
    pub test_mode: bool,
    /// `panic=early|pagefault|ud|panic|stack|user-read|user-exec`:
    /// deliberately crash (exercises the panic screen in tests). `early`
    /// faults before the console starts, the others after boot.
    pub panic_test: Option<PanicTest>,
    /// `tsc=hpet`: measure the TSC against the HPET even when CPUID knows
    /// its frequency (exercises the QEMU path everywhere).
    pub tsc_hpet: bool,
    /// `check=timer`: count timer ticks over three RTC seconds at boot.
    pub check_timer: bool,
    /// `debug=usb`: the USB stack's log also goes to the screen, for a
    /// machine whose keyboard does not work (no serial port, no `dmesg`).
    pub debug_usb: bool,
}

impl Cmdline {
    pub fn parse(s: &str) -> Cmdline {
        let mut c = Cmdline::default();
        for word in s.split_whitespace() {
            match word.split_once('=') {
                Some(("test", "1")) => c.test_mode = true,
                Some(("tsc", "hpet")) => c.tsc_hpet = true,
                Some(("check", "timer")) => c.check_timer = true,
                Some(("debug", "usb")) => c.debug_usb = true,
                Some(("panic", v)) => {
                    c.panic_test = match v {
                        "early" => Some(PanicTest::Early),
                        "pagefault" => Some(PanicTest::PageFault),
                        "ud" => Some(PanicTest::InvalidOpcode),
                        "panic" => Some(PanicTest::Panic),
                        "stack" => Some(PanicTest::StackOverflow),
                        "user-read" => Some(PanicTest::UserRead),
                        "user-exec" => Some(PanicTest::UserExec),
                        _ => None,
                    }
                }
                _ => {} // unknown words are ignored (the loader uses video=)
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(Cmdline::parse(""), Cmdline::default());
    }

    #[test]
    fn a_program_s_page_from_the_kernel() {
        assert_eq!(
            Cmdline::parse("panic=user-read").panic_test,
            Some(PanicTest::UserRead)
        );
        assert_eq!(
            Cmdline::parse("panic=user-exec").panic_test,
            Some(PanicTest::UserExec)
        );
    }

    #[test]
    fn test_mode_and_panic_kinds() {
        let c = Cmdline::parse("video=1280x720 test=1 panic=pagefault");
        assert!(c.test_mode);
        assert_eq!(c.panic_test, Some(PanicTest::PageFault));
        assert_eq!(
            Cmdline::parse("panic=early").panic_test,
            Some(PanicTest::Early)
        );
        assert_eq!(
            Cmdline::parse("panic=ud").panic_test,
            Some(PanicTest::InvalidOpcode)
        );
        assert_eq!(
            Cmdline::parse("panic=panic").panic_test,
            Some(PanicTest::Panic)
        );
        assert_eq!(
            Cmdline::parse("panic=stack").panic_test,
            Some(PanicTest::StackOverflow)
        );
    }

    #[test]
    fn timer_options() {
        let c = Cmdline::parse("test=1 tsc=hpet check=timer");
        assert!(c.tsc_hpet);
        assert!(c.check_timer);
        assert!(!Cmdline::parse("tsc=cpuid check=all").tsc_hpet);
        assert!(!Cmdline::parse("check=all").check_timer);
    }

    #[test]
    fn usb_debugging() {
        assert!(Cmdline::parse("video=1920x1080 debug=usb").debug_usb);
        assert!(!Cmdline::parse("debug=all").debug_usb);
    }

    #[test]
    fn unknown_values_are_ignored() {
        let c = Cmdline::parse("test=0 panic=bogus foo tsc=bogus debug=bogus");
        assert_eq!(c, Cmdline::default());
    }
}
