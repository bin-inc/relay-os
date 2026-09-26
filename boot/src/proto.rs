//! Opening firmware protocols without taking them over.

use core::mem::ManuallyDrop;
use uefi::Handle;
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol};
use uefi::proto::ProtocolPointer;

/// Opens `P` on `handle` with GET_PROTOCOL, as the Linux EFI stub does.
///
/// Exclusive opens make the firmware stop its own drivers using the device.
/// On the NUC 12 firmware that stops the text console as soon as the GOP is
/// held, and a later ExitBootServices never returns (seen on hardware; the
/// QEMU firmware tolerates it).
///
/// GET_PROTOCOL opens need not be closed, and are never closed here: the
/// firmware can answer the close with NOT_FOUND (OVMF does), which trips the
/// `uefi` crate's drop assertion. Hence the `ManuallyDrop`.
pub fn get<P: ProtocolPointer + ?Sized>(
    handle: Handle,
) -> uefi::Result<ManuallyDrop<ScopedProtocol<P>>> {
    let params = OpenProtocolParams {
        handle,
        agent: boot::image_handle(),
        controller: None,
    };
    // SAFETY: the loader only reads through these protocols while boot
    // services are active; GET_PROTOCOL does not interfere with the drivers
    // that own the handle.
    unsafe { boot::open_protocol::<P>(params, OpenProtocolAttributes::GetProtocol) }
        .map(ManuallyDrop::new)
}
