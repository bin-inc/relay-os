//! The shell's surroundings in the kernel (spec §7.2, §7.3): the console as
//! `shell::Console` (input from the USB keyboards and COM1, `tty`), the
//! clock, memory figures, kernel log and programs as `shell::System`, the
//! kernel's mount table as its `Vfs` (`mounts::KernelVfs`), and `vfs::Env`
//! for filesystems.

use crate::mm::{self, MemStats, frame::FRAME_SIZE};
use crate::mounts::KernelVfs;
use crate::syscall::Spawn;
use crate::{arch, console, exec, klog, klogln, power, proc, rtc, tty};
use alloc::vec::Vec;
use relay_abi::{FdMap, WaitStatus};
use shell::{Console, MemInfo, Output, Shell, System};
use vfs::{Env, Errno, Vfs};

/// The screen and serial for output; the USB keyboards and COM1 for input.
#[derive(Default)]
pub struct KernelConsole;

impl Console for KernelConsole {
    fn read_byte(&mut self) -> Option<u8> {
        loop {
            tty::poll();
            if let Some(b) = tty::pop() {
                return Some(b);
            }
            // Nothing typed: the idle task polls until something is.
            proc::wait_for_input();
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        console::write_output(bytes);
    }

    fn columns(&self) -> usize {
        console::size().map_or(80, |(cols, _)| cols)
    }

    fn interrupted(&mut self) -> bool {
        tty::poll();
        tty::take_interrupt()
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

    /// Through the kernel's mount table from the shell's current directory
    /// (which `vfs` is), in a new process group, with the shell's fds 0-2,
    /// its outputs fresh for this command; the orphans that have ended are
    /// collected first.
    fn spawn(
        &mut self,
        _vfs: &mut dyn Vfs,
        path: &[u8],
        args: &[&[u8]],
    ) -> Option<Result<u32, Errno>> {
        let std = [0, 1, 2].map(|fd| FdMap {
            child: fd,
            parent: fd,
        });
        // Orphans that ended while the shell waited for nobody; and
        // outputs of the command's own.
        proc::collect_orphans();
        proc::renew_outputs();
        Some(exec::arg_bytes(args).and_then(|bytes| {
            proc::spawn(&Spawn {
                path: path.to_vec(),
                args: bytes,
                argc: args.len() as u64,
                cwd: Vec::new(),
                fds: std.to_vec(),
                new_group: true,
            })
        }))
    }

    /// The command has the console, in line mode, while the shell waits
    /// for it (spec §6.4): a Ctrl-C kills it.
    fn wait(&mut self, pid: u32, out: &mut Output<'_>) -> Result<WaitStatus, Errno> {
        proc::give_console(pid);
        let ended = proc::wait(pid, out);
        proc::take_console();
        ended
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

/// Process 1 (spec §4.4 step 11): the shell over the kernel's mount table
/// (the root at `/`, the programs at `/bin`); `test_mode` is 1 for
/// `test=1`. Never returns.
pub extern "C" fn shell(test_mode: u64) -> ! {
    let mut vfs = KernelVfs;
    let mut console = KernelConsole;
    let mut system = KernelSystem {
        test_mode: test_mode != 0,
    };
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
