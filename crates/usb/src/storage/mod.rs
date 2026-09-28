//! The USB mass-storage class driver (spec §6.4): Bulk-Only Transport
//! (USB Mass Storage Class Bulk-Only Transport 1.0) carrying SCSI commands
//! (SPC-4, SBC-3) to LUN 0 of a USB stick.
//!
//! Every log line starts "storage: slot N: " (spec §13: the NUC is debugged
//! from a photo of `dmesg`): each setup step and each failure, with its
//! sense, but not a read or write that went well.

/// Logs one line starting "storage: slot N: ".
macro_rules! slog {
    ($bus:expr, $slot:expr, $($arg:tt)*) => {
        $bus.log(format_args!("storage: slot {}: {}", $slot, format_args!($($arg)*)))
    };
}

pub mod bot;
mod disk;
mod io;
pub mod scsi;
mod transport;

pub use disk::{MassStorage, is_mass_storage};
pub use scsi::Sense;
