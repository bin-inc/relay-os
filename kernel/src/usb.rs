//! The USB stack in the kernel (spec §6): the `Hal` it runs on, startup
//! step 8 (every xHCI controller and the devices on its ports), and the
//! polling the console does for key presses and hot-plug.

use crate::input::InputQueue;
use crate::mm::{self, paging::Cache};
use crate::pci::{self, BarKind, PciDevice};
use crate::{console, klogln, kprintln, timer};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use spin::Mutex;
use usb::host::Host;
use usb::xhci::Xhci;
use usb::{DmaBuf, Hal, UsbError};

/// Every running controller with its drivers. The console polls them; the
/// storage driver will share them.
static HOSTS: Mutex<Vec<Host<KernelHal>>> = Mutex::new(Vec::new());

/// `debug=usb`: the USB log goes to the screen as well.
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Device registers through `mm::map_mmio` (uncached), DMA memory from the
/// frame allocator, the TSC for time and the kernel log.
#[derive(Clone, Copy, Debug, Default)]
pub struct KernelHal;

impl Hal for KernelHal {
    fn map_mmio(&self, phys: u64, len: usize) -> Option<usize> {
        match mm::map_mmio(phys, len as u64, Cache::Uncached) {
            Ok(p) => Some(p as usize),
            Err(e) => {
                klogln!("usb: cannot map {len:#x} bytes at {phys:#x}: {e}");
                None
            }
        }
    }

    unsafe fn read32(&self, addr: usize) -> u32 {
        // SAFETY: the caller keeps `addr` inside a range `map_mmio` mapped.
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }

    unsafe fn write32(&self, addr: usize, value: u32) {
        // SAFETY: as in `read32`.
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }

    fn alloc_dma(&self, size: usize, align: usize) -> Option<DmaBuf> {
        let (virt, phys) = mm::alloc_dma(size, align)?;
        // SAFETY: fresh frames of at least `size` bytes, owned by this
        // buffer until `free_dma`.
        Some(unsafe { DmaBuf::new(virt, phys, size) })
    }

    fn free_dma(&self, buf: DmaBuf) {
        mm::free_dma(buf.phys(), buf.size());
    }

    fn now(&self) -> Duration {
        timer::tsc_time().unwrap_or_else(timer::uptime)
    }

    fn sleep(&self, d: Duration) {
        timer::sleep(d);
    }

    fn log(&self, args: fmt::Arguments) {
        if VERBOSE.load(Ordering::Relaxed) {
            kprintln!("{args}");
        } else {
            klogln!("{args}");
        }
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    alloc::format!("{n} {}", if n == 1 { one } else { many })
}

/// The `[ ok ] usb` step: controllers running and devices set up.
pub fn usb_summary(controllers: usize, devices: usize) -> String {
    alloc::format!(
        "usb: {}, {}",
        plural(controllers, "controller", "controllers"),
        plural(devices, "device", "devices")
    )
}

/// Startup step 8 (spec §4.4): starts every xHCI controller PCI found and
/// sets up the devices on its ports. One line per controller and device
/// goes to the screen (a photo of it is how the NUC is debugged), then the
/// `usb` and `keyboard` status lines. A controller that fails is skipped.
pub fn init(verbose: bool) {
    VERBOSE.store(verbose, Ordering::Relaxed);
    if timer::tsc_hz() == 0 {
        console::fail("usb", format_args!("no timer"));
        console::fail("keyboard", format_args!("no USB"));
        return;
    }
    let mut hosts = Vec::new();
    let mut devices = 0;
    let mut error = None;
    for d in pci::devices().iter().filter(|d| d.is_xhci()) {
        let name = d.address.to_string();
        match start(d, &name) {
            Ok((host, found)) => {
                hosts.push(host);
                devices += found;
            }
            Err(e) => {
                kprintln!("usb: {name}: {e}");
                error = Some(e);
            }
        }
    }
    match (hosts.len(), error) {
        (0, None) => console::fail("usb", format_args!("no xHCI controller")),
        (0, Some(e)) => console::fail("usb", format_args!("{e}")),
        (n, _) => console::ok(format_args!("{}", usb_summary(n, devices))),
    }
    let keyboards: usize = hosts.iter().map(|h| h.keyboards()).sum();
    if keyboards == 0 {
        console::fail("keyboard", format_args!("no USB keyboard found"));
    } else {
        console::ok(format_args!(
            "keyboard: {}",
            plural(keyboards, "keyboard", "keyboards")
        ));
    }
    *HOSTS.lock() = hosts;
}

/// Starts one controller and the devices on its ports; returns it and how
/// many devices were set up.
fn start(d: &PciDevice, name: &str) -> Result<(Host<KernelHal>, usize), UsbError> {
    let bar = d
        .bars
        .iter()
        .find(|b| b.index == 0 && b.kind != BarKind::Io)
        .ok_or(UsbError::Unsupported("no memory BAR"))?;
    if let Some(w) = pci::enable_device(d)
        && w.from != 0
    {
        let reset = if w.reset { ", BARs restored" } else { "" };
        klogln!("usb: {name}: woken from D{}{reset}", w.from);
    }
    let xhci = Xhci::new(KernelHal, bar.address, bar.size as usize, name)?;
    kprintln!("usb: {name} {}", xhci.info());
    let mut host = Host::new(xhci);
    let attached = host.service();
    for a in &attached {
        kprintln!("usb: {name} {a}");
    }
    Ok((host, attached.iter().filter(|a| a.outcome.is_ok()).count()))
}

/// Moves key presses from every keyboard into `input`. Never waits.
pub fn poll(input: &mut InputQueue) {
    for host in HOSTS.lock().iter_mut() {
        host.poll();
        while let Some(e) = host.next_key() {
            input.push_key(&e);
        }
    }
}

/// Hot-plug and the keyboards' LED and recovery work. May wait while a new
/// device is set up, so only the console's idle loop calls it.
pub fn service() {
    for host in HOSTS.lock().iter_mut() {
        for a in host.service() {
            klogln!("usb: {} {a}", host.xhci().name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_counts_controllers_and_devices() {
        assert_eq!(usb_summary(1, 2), "usb: 1 controller, 2 devices");
        assert_eq!(usb_summary(2, 1), "usb: 2 controllers, 1 device");
        assert_eq!(usb_summary(1, 0), "usb: 1 controller, 0 devices");
    }
}
