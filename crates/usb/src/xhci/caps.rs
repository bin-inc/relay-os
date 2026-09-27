//! The extended capability list (xHCI 7): USB Legacy Support for the BIOS
//! handoff and Supported Protocol, which says which root ports are USB 2
//! and which USB 3. The list comes from the hardware, so the walk is
//! bounded: it cannot loop or read outside the BAR.

use super::regs::Regs;
use crate::Hal;
use alloc::vec;
use alloc::vec::Vec;

pub const LEGACY_SUPPORT: u8 = 1;
pub const SUPPORTED_PROTOCOL: u8 = 2;
/// More entries than any controller has; a longer list is broken.
const MAX_CAPS: usize = 64;
/// "USB " in Supported Protocol dword 1 (xHCI 7.2.2.1.2).
const USB_NAME: u32 = u32::from_le_bytes(*b"USB ");

/// One entry: its offset from the start of the BAR and its ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cap {
    pub offset: usize,
    pub id: u8,
}

/// The walked list, and why it ended early if it did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapList {
    pub caps: Vec<Cap>,
    pub problem: Option<&'static str>,
}

impl CapList {
    /// Walks the list starting `xecp` dwords into the BAR (HCCPARAMS1
    /// bits 31:16); each entry's bits 15:8 give the next one's distance in
    /// dwords, 0 ending the list.
    pub fn walk<H: Hal>(hal: &H, regs: &Regs, xecp: u16) -> CapList {
        let mut list = CapList {
            caps: Vec::new(),
            problem: None,
        };
        if xecp == 0 {
            return list;
        }
        let mut offset = xecp as usize * 4;
        loop {
            if list.caps.len() == MAX_CAPS {
                list.problem = Some("more than 64 capabilities");
                break;
            }
            let Some(header) = regs.try_read(hal, offset) else {
                list.problem = Some("capability outside the registers");
                break;
            };
            list.caps.push(Cap {
                offset,
                id: header as u8,
            });
            let next = (header >> 8) as u8 as usize;
            if next == 0 {
                break;
            }
            offset += next * 4;
        }
        list
    }

    /// The first capability with `id`.
    pub fn find(&self, id: u8) -> Option<usize> {
        self.caps.iter().find(|c| c.id == id).map(|c| c.offset)
    }
}

/// A Supported Protocol capability: USB `major.minor` (BCD) on `count`
/// ports from `first`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protocol {
    pub major: u8,
    pub minor: u8,
    pub first: u8,
    pub count: u8,
}

/// The Supported Protocol capabilities named "USB " in the list: those
/// that name ports, and those that name none (count 0 or first port 0),
/// which the caller logs and ignores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Protocols {
    pub usable: Vec<Protocol>,
    pub empty: Vec<Protocol>,
}

pub fn protocols<H: Hal>(hal: &H, regs: &Regs, list: &CapList) -> Protocols {
    let mut found = Protocols::default();
    for cap in list.caps.iter().filter(|c| c.id == SUPPORTED_PROTOCOL) {
        let read = |i: usize| regs.try_read(hal, cap.offset + 4 * i);
        let (Some(d0), Some(name), Some(d2)) = (read(0), read(1), read(2)) else {
            continue;
        };
        if name != USB_NAME {
            continue;
        }
        let p = Protocol {
            major: (d0 >> 24) as u8,
            minor: (d0 >> 16) as u8,
            first: d2 as u8,
            count: (d2 >> 8) as u8,
        };
        if p.first == 0 || p.count == 0 {
            found.empty.push(p);
        } else {
            found.usable.push(p);
        }
    }
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortProtocol {
    Usb2,
    Usb3,
}

/// Which protocol each of `ports` root ports speaks (index 0 is port 1).
/// A port no capability covers stays `None`; where two overlap, the first
/// wins.
pub fn port_map(ports: u8, protocols: &[Protocol]) -> Vec<Option<PortProtocol>> {
    let mut map = vec![None; ports as usize];
    for p in protocols {
        let kind = match p.major {
            2 => PortProtocol::Usb2,
            3 => PortProtocol::Usb3,
            _ => continue,
        };
        let first = p.first as usize;
        for port in first..first + p.count as usize {
            if let Some(slot @ None) = port.checked_sub(1).and_then(|i| map.get_mut(i)) {
                *slot = Some(kind);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ExtCap, FAKE_BAR, FAKE_BAR_LEN, FakeConfig, FakeHal};

    fn walk(config: FakeConfig) -> (FakeHal, Regs, CapList) {
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, FAKE_BAR_LEN).unwrap();
        let regs = Regs::new(&hal, base, FAKE_BAR_LEN).unwrap();
        let xecp = super::super::regs::Params::read(&hal, &regs).xecp;
        let list = CapList::walk(&hal, &regs, xecp);
        (hal, regs, list)
    }

    fn map(config: FakeConfig) -> Vec<Option<PortProtocol>> {
        let (hal, regs, list) = walk(config);
        let ports = super::super::regs::Params::read(&hal, &regs).ports;
        port_map(ports, &protocols(&hal, &regs, &list).usable)
    }

    use PortProtocol::{Usb2, Usb3};

    #[test]
    fn basic_has_four_usb2_ports_then_four_usb3_ports() {
        let (hal, regs, list) = walk(FakeConfig::basic());
        assert_eq!(list.problem, None);
        assert_eq!(list.caps.len(), 2);
        let p = protocols(&hal, &regs, &list).usable;
        assert_eq!(
            p,
            [
                Protocol {
                    major: 2,
                    minor: 0,
                    first: 1,
                    count: 4
                },
                Protocol {
                    major: 3,
                    minor: 0,
                    first: 5,
                    count: 4
                },
            ]
        );
        assert_eq!(
            map(FakeConfig::basic()),
            [&[Some(Usb2); 4][..], &[Some(Usb3); 4]].concat()
        );
    }

    #[test]
    fn qemu_lists_usb2_first_but_numbers_usb3_first() {
        assert_eq!(
            map(FakeConfig::qemu()),
            [&[Some(Usb3); 4][..], &[Some(Usb2); 4]].concat()
        );
    }

    #[test]
    fn intel_has_the_legacy_capability_first_and_others_between() {
        let (hal, regs, list) = walk(FakeConfig::intel());
        assert_eq!(list.caps[0].id, LEGACY_SUPPORT);
        assert_eq!(list.find(LEGACY_SUPPORT), Some(list.caps[0].offset));
        assert!(
            list.caps.len() > 4,
            "unrelated capabilities are walked past"
        );
        let p = protocols(&hal, &regs, &list).usable;
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].major, p[1].minor), (3, 0x10));
        assert_eq!(
            map(FakeConfig::intel()),
            [&[Some(Usb2); 12][..], &[Some(Usb3); 4]].concat()
        );
    }

    #[test]
    fn a_port_no_capability_covers_has_no_protocol() {
        let mut config = FakeConfig::basic();
        config
            .caps
            .retain(|c| !matches!(c.cap, ExtCap::Protocol { major: 3, .. }));
        config.caps[0].next = 0;
        let m = map(config);
        assert_eq!(m[3], Some(Usb2));
        assert_eq!(m[4], None);
    }

    #[test]
    fn ports_beyond_the_controller_and_other_protocols_are_ignored() {
        let p = [
            Protocol {
                major: 2,
                minor: 0,
                first: 0,
                count: 2,
            },
            Protocol {
                major: 3,
                minor: 0,
                first: 3,
                count: 200,
            },
            Protocol {
                major: 2,
                minor: 0,
                first: 4,
                count: 1,
            },
            Protocol {
                major: 9,
                minor: 0,
                first: 1,
                count: 1,
            },
        ];
        assert_eq!(port_map(4, &p), [Some(Usb2), None, Some(Usb3), Some(Usb3)]);
    }

    #[test]
    fn a_protocol_capability_naming_no_ports_is_set_aside() {
        for (first, count) in [(0, 0), (5, 0), (0, 4)] {
            let mut config = FakeConfig::basic();
            if let ExtCap::Protocol {
                first: f, count: c, ..
            } = &mut config.caps[1].cap
            {
                (*f, *c) = (first, count);
            }
            let (hal, regs, list) = walk(config);
            let p = protocols(&hal, &regs, &list);
            assert_eq!(p.usable.len(), 1, "first {first} count {count}");
            assert_eq!(
                p.empty,
                [Protocol {
                    major: 3,
                    minor: 0,
                    first,
                    count
                }]
            );
        }
    }

    #[test]
    fn a_protocol_capability_not_named_usb_is_skipped() {
        let mut config = FakeConfig::basic();
        config.protocol_name = u32::from_le_bytes(*b"ABCD");
        let (hal, regs, list) = walk(config);
        assert_eq!(protocols(&hal, &regs, &list), Protocols::default());
    }

    #[test]
    fn a_list_without_end_stops_after_64_entries() {
        let mut config = FakeConfig::intel();
        config.endless_caps = true;
        let (hal, _regs, list) = walk(config);
        assert_eq!(list.caps.len(), 64);
        assert_eq!(list.problem, Some("more than 64 capabilities"));
        assert_eq!(hal.fake().cap_reads(), 64, "one read per entry, no more");
    }

    #[test]
    fn a_list_pointing_outside_the_bar_stops_at_the_edge() {
        let mut config = FakeConfig::intel();
        let last = config.caps.len() - 1;
        let last_offset = config.caps[last].offset;
        config.caps[last].next = 0xFF;
        // The BAR ends 0x100 bytes after the last capability; its next
        // pointer reaches 0x3FC bytes on.
        let end = last_offset + 0x100;
        let hal = FakeHal::with_controller(config);
        let base = hal.map_mmio(FAKE_BAR, end).unwrap();
        let regs = Regs::new(&hal, base, end).unwrap();
        let xecp = super::super::regs::Params::read(&hal, &regs).xecp;
        let list = CapList::walk(&hal, &regs, xecp);
        assert_eq!(list.caps.last().map(|c| c.offset), Some(last_offset));
        assert_eq!(list.problem, Some("capability outside the registers"));
        assert!(hal.fake().highest_read() < end);
    }
}
