//! The USB mass-storage class driver (spec §6.4): Bulk-Only Transport
//! (USB Mass Storage Class Bulk-Only Transport 1.0) carrying SCSI commands
//! (SPC-4, SBC-3) to LUN 0 of a USB stick.

pub mod bot;
pub mod scsi;
