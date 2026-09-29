//! The shell's surroundings in the kernel (spec §7.2, §7.3): the console as
//! `shell::Console` (input from the USB keyboards and COM1), the clock,
//! memory figures, kernel log and programs as `shell::System`, and
//! `vfs::Env` for filesystems.

use crate::input::InputQueue;
use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::{arch, console, klog, klogln, power, proc, rtc, serial, usb};
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use shell::{Console, MemInfo, Shell, System};
use vfs::{Env, Errno, MountTable, Vfs};

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

/// The screen and serial for output; the USB keyboards and COM1 for input.
pub struct KernelConsole {
    input: InputQueue,
}

impl KernelConsole {
    pub fn new() -> KernelConsole {
        KernelConsole {
            input: InputQueue::new(),
        }
    }

    /// Moves whatever the input devices have into the queue. Never waits.
    fn poll(&mut self) {
        usb::poll(&mut self.input);
        for _ in 0..SERIAL_BURST {
            match serial::read_byte() {
                Some(b) => self.input.push_serial(b),
                None => break,
            }
        }
    }
}

impl Default for KernelConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl Console for KernelConsole {
    fn read_byte(&mut self) -> Option<u8> {
        loop {
            self.poll();
            if let Some(b) = self.input.pop() {
                return Some(b);
            }
            // Nothing typed: time for the work that may wait (a keyboard
            // plugged in, the Caps Lock LED), then sleep until the next
            // tick.
            usb::service();
            arch::wait_for_interrupt();
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        console::write_output(bytes);
    }

    fn columns(&self) -> usize {
        console::size().map_or(80, |(cols, _)| cols)
    }

    fn interrupted(&mut self) -> bool {
        self.poll();
        self.input.take_interrupt()
    }
}

/// The frame allocator's and the heap's figures, in bytes, for `free`.
pub fn mem_info(s: MemStats) -> MemInfo {
    MemInfo {
        ram_total: s.total_frames * FRAME_SIZE,
        ram_free: s.free_frames * FRAME_SIZE,
        heap_total: s.heap.total as u64,
        heap_used: s.heap.used as u64,
    }
}

pub struct KernelSystem {
    /// `test=1`: `poweroff` makes QEMU exit (spec §7.4).
    pub test_mode: bool,
}

impl System for KernelSystem {
    fn now(&self) -> u64 {
        rtc::now_unix().unwrap_or(0)
    }

    fn memory(&self) -> Option<MemInfo> {
        Some(mem_info(mm::stats()))
    }

    fn kernel_log(&self) -> Vec<u8> {
        klog::KLOG.lock().to_vec()
    }

    // The shell has shut the filesystems down before these.
    fn reboot(&mut self) {
        power::reboot()
    }

    fn poweroff(&mut self) {
        power::poweroff(self.test_mode)
    }

    fn spawn(
        &mut self,
        vfs: &mut dyn Vfs,
        path: &[u8],
        args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        Some(proc::spawn(vfs, path, args))
    }

    fn wait(&mut self, pid: u32, out: &mut dyn FnMut(u32, &[u8])) -> Result<WaitStatus, Errno> {
        proc::wait(pid, out)
    }
}

/// The RTC and the kernel log, for filesystems.
pub struct KernelEnv;

impl Env for KernelEnv {
    fn now(&self) -> u64 {
        rtc::now_unix().unwrap_or(0)
    }

    fn log(&self, line: &str) {
        klogln!("{line}");
    }
}

/// Runs the shell over `vfs`, the root at `/` and the programs at `/bin`
/// (spec §4.4 step 11). Never returns.
pub fn run_shell(mut vfs: MountTable, test_mode: bool) -> ! {
    let mut console = KernelConsole::new();
    let mut system = KernelSystem { test_mode };
    Shell::new(&mut vfs, &mut console, &mut system).run();
    // `run` returns only if `reboot` or `poweroff` do, which they do not.
    arch::halt_forever()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::heap::HeapStats;

    #[test]
    fn memory_figures_are_in_bytes() {
        let s = MemStats {
            free_frames: 3,
            total_frames: 5,
            heap: HeapStats {
                total: 32 << 20,
                used: 1024,
                free_large: 0,
                largest_free: 0,
            },
        };
        assert_eq!(
            mem_info(s),
            MemInfo {
                ram_total: 5 * 4096,
                ram_free: 3 * 4096,
                heap_total: 32 << 20,
                heap_used: 1024,
            }
        );
    }
}
