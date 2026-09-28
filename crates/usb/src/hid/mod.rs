//! The HID boot keyboard (spec §6.3): the driver and the US layout.

pub mod keyboard;
pub mod keymap;

pub use keyboard::{BootKeyboard, KeyEvent, is_boot_keyboard};
pub use keymap::{Key, Modifiers};
