//! The console's input (user-space gate §6.3): one queue for every source,
//! the USB keyboards and COM1, filled by `poll`. `poll` never waits, so it
//! can run wherever the kernel holds nothing: in the console's idle loop
//! and whenever a command asks whether Ctrl-C was pressed.

use crate::input::InputQueue;
use crate::{serial, usb};
use spin::Mutex;

/// Bytes read from COM1 per poll at most, so a flood cannot starve the rest.
const SERIAL_BURST: usize = 256;

static INPUT: Mutex<InputQueue> = Mutex::new(InputQueue::new());

/// Moves whatever the input devices have into the queue. Never waits.
pub fn poll() {
    let mut input = INPUT.lock();
    usb::poll(&mut input);
    for _ in 0..SERIAL_BURST {
        match serial::read_byte() {
            Some(b) => input.push_serial(b),
            None => break,
        }
    }
}

/// The oldest byte typed.
pub fn pop() -> Option<u8> {
    INPUT.lock().pop()
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
