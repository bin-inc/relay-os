//! The console's input (user-space gate §6.3–§6.5): one queue for every
//! source, the USB keyboards and COM1, filled by `poll`. `poll` never waits,
//! so it can run wherever the kernel holds nothing: in the idle task, on
//! every tick that interrupts a program, and whenever the in-kernel shell
//! reads or asks whether Ctrl-C was pressed.
//!
//! The console has a foreground process group and a mode (spec §6.4). In
//! raw mode a Ctrl-C is input like any other byte (the shell's line editor
//! cancels its line); in line mode it is for the foreground group, which
//! the process table kills (`ctrl_c`), and what is typed goes through the
//! line discipline, which echoes it as it comes (`input`).

use crate::input::InputQueue;
use crate::{console, serial, timer, usb};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

/// The foreground process group; process 1's at first.
static FOREGROUND: AtomicU32 = AtomicU32::new(1);

/// The console's mode: raw (`false`) or line (`true`); the previous one.
/// What the switch hands the line discipline is echoed.
pub fn set_line_mode(line: bool) -> bool {
    let (was, echo) = {
        let mut input = INPUT.lock();
        (input.set_line_mode(line), input.take_echo())
    };
    output(&echo);
    was
}

pub fn is_line_mode() -> bool {
    INPUT.lock().is_line_mode()
}

/// Makes `pgid` the console's foreground group.
pub fn set_foreground(pgid: u32) {
    FOREGROUND.store(pgid, Ordering::Relaxed);
}

/// The group that reads the console (spec §6.4).
pub fn foreground() -> u32 {
    FOREGROUND.load(Ordering::Relaxed)
}

/// The foreground group a Ctrl-C typed in line mode is for, if one was
/// typed: what was typed before it is dropped (spec §6.4).
pub fn ctrl_c() -> Option<u32> {
    INPUT
        .lock()
        .take_line_interrupt()
        .then(|| FOREGROUND.load(Ordering::Relaxed))
}

/// What a program reads (spec §6.5): in line mode the next line, in raw
/// mode what was typed, up to `buf`'s length; `None` if nothing waits.
pub fn read(buf: &mut [u8]) -> Option<usize> {
    INPUT.lock().read(buf)
}

/// The line discipline's echo, to the screen.
fn output(echo: &[u8]) {
    if !echo.is_empty() {
        console::write_output(echo);
    }
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
    let echo = input.take_echo();
    drop(input);
    output(&echo);
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

/// Whether anything typed waits to be read: raw input, or a line.
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
