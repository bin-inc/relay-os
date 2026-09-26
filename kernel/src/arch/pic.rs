//! The legacy 8259 PIC pair. Relay OS uses the LAPIC timer only, so the PIC
//! is remapped away from the CPU exception vectors (32-47) and every line
//! is masked. A spurious IRQ 7 or 15 can still arrive; see `irq`.

use x86_64::instructions::port::Port;

pub const MASTER_CMD: u16 = 0x20;
pub const MASTER_DATA: u16 = 0x21;
pub const SLAVE_CMD: u16 = 0xA0;
pub const SLAVE_DATA: u16 = 0xA1;
/// Writing here gives the PIC time between initialisation words.
const IO_WAIT: u16 = 0x80;
const EOI: u8 = 0x20;

/// Port output, so the sequence can be checked on the host.
pub trait PortOut {
    fn outb(&mut self, port: u16, value: u8);
}

pub struct RealPorts;

impl PortOut for RealPorts {
    fn outb(&mut self, port: u16, value: u8) {
        // SAFETY: only used with the PIC and POST ports.
        unsafe { Port::new(port).write(value) }
    }
}

/// Remaps IRQ 0-7 to `base`..`base + 8` and IRQ 8-15 to the next eight
/// vectors, then masks every line.
pub fn remap_and_mask(io: &mut impl PortOut, base: u8) {
    let words = [
        (MASTER_CMD, 0x11), // ICW1: initialise, ICW4 follows
        (SLAVE_CMD, 0x11),
        (MASTER_DATA, base), // ICW2: vector offset
        (SLAVE_DATA, base + 8),
        (MASTER_DATA, 4), // ICW3: slave on IRQ 2
        (SLAVE_DATA, 2),
        (MASTER_DATA, 1), // ICW4: 8086 mode
        (SLAVE_DATA, 1),
        (MASTER_DATA, 0xFF), // mask everything
        (SLAVE_DATA, 0xFF),
    ];
    for (port, value) in words {
        io.outb(port, value);
        io.outb(IO_WAIT, 0);
    }
}

/// End of interrupt to the master PIC (needed after a spurious IRQ 15).
pub fn eoi_master(io: &mut impl PortOut) {
    io.outb(MASTER_CMD, EOI);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder(Vec<(u16, u8)>);

    impl PortOut for Recorder {
        fn outb(&mut self, port: u16, value: u8) {
            if port != IO_WAIT {
                self.0.push((port, value));
            }
        }
    }

    #[test]
    fn remaps_to_32_and_masks_every_line() {
        let mut r = Recorder::default();
        remap_and_mask(&mut r, 32);
        assert_eq!(
            r.0,
            [
                (0x20, 0x11),
                (0xA0, 0x11),
                (0x21, 32),
                (0xA1, 40),
                (0x21, 4),
                (0xA1, 2),
                (0x21, 1),
                (0xA1, 1),
                (0x21, 0xFF),
                (0xA1, 0xFF),
            ]
        );
    }
}
