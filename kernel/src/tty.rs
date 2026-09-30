//! The console's input (user-space gate §6.3–§6.5): one queue for every
//! source, the USB keyboards and COM1, filled by `poll`. `poll` never waits,
//! so it can run wherever the kernel holds nothing: in the idle task, on
//! every tick that interrupts a program, and whenever the in-kernel shell
//! reads or asks whether Ctrl-C was pressed.
//!
//! The console has a foreground process group and a mode (spec §6.4). In
//! raw mode a Ctrl-C is input like any other byte (the shell's line editor
//! cancels its line); in line mode it is for the foreground group, which
//! the process table kills (`ctrl_c`). Plan 3b adds reading in line mode.

use crate::input::InputQueue;
use crate::{serial, timer, usb};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

/// Line mode (true) or raw mode.
static LINE_MODE: AtomicBool = AtomicBool::new(false);
/// The foreground process group; process 1's at first.
static FOREGROUND: AtomicU32 = AtomicU32::new(1);

/// The console's mode: raw (`false`) or line (`true`); the previous one.
pub fn set_line_mode(line: bool) -> bool {
    LINE_MODE.swap(line, Ordering::Relaxed)
}

/// Makes `pgid` the console's foreground group.
pub fn set_foreground(pgid: u32) {
    FOREGROUND.store(pgid, Ordering::Relaxed);
}

/// The foreground group a Ctrl-C typed in line mode is for, if one is
/// waiting: it and what was typed before it are dropped (spec §6.4).
pub fn ctrl_c() -> Option<u32> {
    if !LINE_MODE.load(Ordering::Relaxed) {
        return None;
    }
    take_interrupt().then(|| FOREGROUND.load(Ordering::Relaxed))
}

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

static INPUT: Mutex<InputQueue> = Mutex::new(InputQueue::new());

/// Moves whatever the input devices have into the queue. Never waits.
pub fn poll() {
    let mut input = INPUT.lock();
    usb::poll(&mut input);
    let now = now_ms();
    for _ in 0..SERIAL_BURST {
        match serial::read_byte() {
            Some(b) => input.push_serial(b, now),
            None => break,
        }
    }
    input.expire(now);
}

/// Milliseconds since the machine started, for serial escape sequences:
/// from the TSC, which runs even when the timer could not start.
fn now_ms() -> u64 {
    let t = timer::tsc_time().unwrap_or_else(timer::uptime);
    u64::try_from(t.as_millis()).unwrap_or(u64::MAX)
}

/// The oldest byte typed.
pub fn pop() -> Option<u8> {
    INPUT.lock().pop()
}

/// Whether anything typed waits to be read.
pub fn has_input() -> bool {
    !INPUT.lock().is_empty()
}

/// Whether a Ctrl-C is waiting; if so, it and what was typed before it are
/// dropped (`InputQueue::take_interrupt`).
pub fn take_interrupt() -> bool {
    INPUT.lock().take_interrupt()
}

/// Whether the input queue is locked now (for the kernel's checks that no
/// lock is held across a switch).
pub fn is_locked() -> bool {
    INPUT.is_locked()
}
