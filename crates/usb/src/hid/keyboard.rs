//! The HID boot-protocol keyboard (spec §6.3). It keeps one interrupt-IN
//! transfer queued, compares each 8-byte report with the last one to find
//! presses and releases, tracks Caps Lock (shown on the keyboard's LED) and
//! repeats a held key in software: 500 ms, then 30 times a second.
//!
//! `poll` never waits; everything that needs a control transfer (the LED,
//! recovering a halted endpoint) waits for `service`, which runs where
//! blocking is allowed (the console's idle loop).

use super::keymap::{self, Key, Modifiers};
use crate::UsbError;
use crate::bus::{Bus, RECIPIENT_INTERFACE, Setup, TYPE_CLASS};
use crate::descriptor::{Endpoint, EndpointKind, Interface};
use alloc::collections::VecDeque;
use core::time::Duration;

pub const REPEAT_DELAY: Duration = Duration::from_millis(500);
/// 30 repeats a second.
pub const REPEAT_INTERVAL: Duration = Duration::from_micros(33_333);
/// Time between attempts to recover a failed interrupt endpoint.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(1);
/// Failed recoveries in a row after which the keyboard is given up.
pub const MAX_RECOVERIES: u32 = 3;

/// HID class requests (HID 1.11 §7.2).
const SET_REPORT: u8 = 0x09;
const SET_IDLE: u8 = 0x0A;
const SET_PROTOCOL: u8 = 0x0B;
const BOOT_PROTOCOL: u16 = 0;
/// `SET_REPORT`'s value: report type Output (2), report ID 0.
const OUTPUT_REPORT: u16 = 0x0200;
/// Bit 1 of the LED output report.
const LED_CAPS_LOCK: u8 = 0x02;
const USAGE_CAPS_LOCK: u8 = 0x39;
/// The largest report read; a boot report is 8 bytes.
const MAX_REPORT: usize = 64;

/// A key going down (also when it repeats) or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    /// The modifiers when the event happened; `key` already has Shift and
    /// Caps Lock applied.
    pub modifiers: Modifiers,
    pub pressed: bool,
}

fn interrupt_in(iface: &Interface) -> Option<&Endpoint> {
    iface
        .endpoints
        .iter()
        .find(|e| e.is_in() && e.kind == EndpointKind::Interrupt && e.packet_size() > 0)
}

/// Class 3 (HID), subclass 1 (boot), protocol 1 (keyboard), with an
/// interrupt-IN endpoint: the K120 and the Unifying receiver's keyboard.
pub fn is_boot_keyboard(iface: &Interface) -> bool {
    (iface.class, iface.subclass, iface.protocol) == (3, 1, 1) && interrupt_in(iface).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Running,
    /// The interrupt endpoint failed; recovery is tried at `retry_at`.
    Failed {
        retry_at: Duration,
        attempts: u32,
    },
    /// Given up, or the device is gone.
    Stopped,
}

pub struct BootKeyboard {
    slot: u8,
    interface: u8,
    endpoint: u8,
    len: usize,
    /// Usages held in the last report accepted (0 is an empty slot).
    keys: [u8; 6],
    modifier_byte: u8,
    caps_lock: bool,
    /// The LED state the keyboard shows.
    leds: u8,
    /// The key being repeated and when it repeats next.
    repeat: Option<(u8, Duration)>,
    state: State,
}

impl BootKeyboard {
    /// Claims `iface` of the configured device in `slot`: `SET_PROTOCOL`
    /// (boot), `SET_IDLE(0)` (reports only on changes; a stall is fine),
    /// then the first interrupt-IN transfer.
    pub fn start(bus: &mut dyn Bus, slot: u8, iface: &Interface) -> Result<BootKeyboard, UsbError> {
        let ep = *interrupt_in(iface).ok_or(UsbError::Unsupported("no interrupt IN endpoint"))?;
        class_request(
            bus,
            slot,
            iface.number,
            SET_PROTOCOL,
            BOOT_PROTOCOL,
            &mut [],
        )?;
        if let Err(e) = class_request(bus, slot, iface.number, SET_IDLE, 0, &mut []) {
            bus.log(format_args!(
                "hid: slot {slot} interface {}: SET_IDLE {e}, continuing",
                iface.number
            ));
        }
        let len = (ep.packet_size() as usize).min(MAX_REPORT);
        bus.queue_in(slot, ep.address, len)?;
        Ok(BootKeyboard {
            slot,
            interface: iface.number,
            endpoint: ep.address,
            len,
            keys: [0; 6],
            modifier_byte: 0,
            caps_lock: false,
            leds: 0,
            repeat: None,
            state: State::Running,
        })
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    pub fn interface(&self) -> u8 {
        self.interface
    }

    /// Whether the keyboard was given up (its device is gone or its
    /// endpoint cannot be recovered).
    pub fn is_stopped(&self) -> bool {
        self.state == State::Stopped
    }

    /// Takes a finished report, queues the next transfer and produces the
    /// events, including repeats that are due. Never waits.
    pub fn poll(&mut self, bus: &mut dyn Bus, out: &mut VecDeque<KeyEvent>) {
        if self.state == State::Running {
            let mut buf = [0u8; MAX_REPORT];
            match bus.take_in(self.slot, self.endpoint, &mut buf[..self.len]) {
                None => {}
                Some(Ok(n)) => {
                    self.report(&buf[..n.min(self.len)], bus.now(), out);
                    if let Err(e) = bus.queue_in(self.slot, self.endpoint, self.len) {
                        self.fail(bus, e, out);
                    }
                }
                Some(Err(e)) => self.fail(bus, e, out),
            }
        }
        self.repeat_due(bus.now(), out);
    }

    /// The work that needs control transfers: the Caps Lock LED and
    /// recovering a failed endpoint (at most once a second).
    pub fn service(&mut self, bus: &mut dyn Bus) {
        if let State::Failed { retry_at, attempts } = self.state {
            if bus.now() < retry_at {
                return;
            }
            let recovered = bus
                .clear_halt(self.slot, self.endpoint)
                .and_then(|()| bus.queue_in(self.slot, self.endpoint, self.len));
            self.state = match recovered {
                Ok(()) => {
                    bus.log(format_args!("hid: slot {}: keyboard recovered", self.slot));
                    State::Running
                }
                Err(e) if attempts + 1 >= MAX_RECOVERIES => {
                    bus.log(format_args!(
                        "hid: slot {}: recovery failed ({e}); keyboard given up",
                        self.slot
                    ));
                    State::Stopped
                }
                Err(e) => {
                    bus.log(format_args!(
                        "hid: slot {}: recovery failed ({e})",
                        self.slot
                    ));
                    State::Failed {
                        retry_at: bus.now() + RETRY_INTERVAL,
                        attempts: attempts + 1,
                    }
                }
            };
        }
        let leds = if self.caps_lock { LED_CAPS_LOCK } else { 0 };
        if self.state == State::Running && leds != self.leds {
            // Tried once: a keyboard without LEDs must not be asked again
            // on every idle loop.
            self.leds = leds;
            let mut data = [leds];
            if let Err(e) = class_request(
                bus,
                self.slot,
                self.interface,
                SET_REPORT,
                OUTPUT_REPORT,
                &mut data,
            ) {
                bus.log(format_args!("hid: slot {}: LED report {e}", self.slot));
            }
        }
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers::from_report(self.modifier_byte, self.caps_lock)
    }

    fn emit(&self, usage: u8, pressed: bool, out: &mut VecDeque<KeyEvent>) {
        if let Some(key) = keymap::key(usage, self.modifiers()) {
            out.push_back(KeyEvent {
                key,
                modifiers: self.modifiers(),
                pressed,
            });
        }
    }

    /// One boot report: modifiers, a reserved byte, up to six usages.
    fn report(&mut self, r: &[u8], now: Duration, out: &mut VecDeque<KeyEvent>) {
        if r.len() < 3 {
            return;
        }
        let mut keys = [0u8; 6];
        for (k, &u) in keys.iter_mut().zip(&r[2..]) {
            *k = u;
        }
        // ErrorRollOver (too many keys), POSTFail and ErrorUndefined: the
        // keyboard does not know which keys are down, so nothing changes.
        if keys.iter().any(|u| (1..=3).contains(u)) {
            return;
        }
        // A usage listed twice is one key.
        for i in 1..keys.len() {
            if keys[..i].contains(&keys[i]) {
                keys[i] = 0;
            }
        }
        self.modifier_byte = r[0];
        for u in self.keys {
            if u != 0 && !keys.contains(&u) {
                self.emit(u, false, out);
                if self.repeat.is_some_and(|(r, _)| r == u) {
                    self.repeat = None;
                }
            }
        }
        for u in keys {
            if u == 0 || self.keys.contains(&u) {
                continue;
            }
            if u == USAGE_CAPS_LOCK {
                self.caps_lock = !self.caps_lock;
            }
            self.emit(u, true, out);
            if repeats(u) {
                self.repeat = Some((u, now + REPEAT_DELAY));
            }
        }
        self.keys = keys;
    }

    /// At most one repeat per call: after a long gap (a busy command) the
    /// key repeats once, not in a burst.
    fn repeat_due(&mut self, now: Duration, out: &mut VecDeque<KeyEvent>) {
        let Some((usage, due)) = self.repeat else {
            return;
        };
        if now < due {
            return;
        }
        self.emit(usage, true, out);
        let next = due + REPEAT_INTERVAL;
        self.repeat = Some((
            usage,
            if next <= now {
                now + REPEAT_INTERVAL
            } else {
                next
            },
        ));
    }

    /// The endpoint failed: every held key is released (nothing may keep
    /// repeating) and recovery waits for `service`.
    fn fail(&mut self, bus: &mut dyn Bus, e: UsbError, out: &mut VecDeque<KeyEvent>) {
        bus.log(format_args!(
            "hid: slot {} endpoint {:#04x}: {e}",
            self.slot, self.endpoint
        ));
        for u in core::mem::take(&mut self.keys) {
            if u != 0 {
                self.emit(u, false, out);
            }
        }
        self.repeat = None;
        self.state = if e == UsbError::Disconnected {
            State::Stopped
        } else {
            State::Failed {
                retry_at: bus.now(),
                attempts: 0,
            }
        };
    }
}

/// Lock keys toggle; they do not repeat.
fn repeats(usage: u8) -> bool {
    !matches!(usage, 0x39 | 0x47 | 0x53)
}

fn class_request(
    bus: &mut dyn Bus,
    slot: u8,
    interface: u8,
    request: u8,
    value: u16,
    data: &mut [u8],
) -> Result<(), UsbError> {
    let setup = Setup {
        request_type: TYPE_CLASS | RECIPIENT_INTERFACE,
        request,
        value,
        index: interface as u16,
        length: data.len() as u16,
    };
    bus.control(slot, setup, data).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::{Cell, RefCell};
    use core::fmt::{self, Write};

    const SLOT: u8 = 1;

    /// A bus with one device: records requests, completes the queued IN
    /// transfer with the next scripted report, and has a clock of its own.
    #[derive(Default)]
    struct FakeBus {
        now: Cell<Duration>,
        requests: Vec<(Setup, Vec<u8>)>,
        /// Request codes the device stalls.
        stall: Vec<u8>,
        queued: Option<(u8, usize)>,
        reports: VecDeque<Result<Vec<u8>, UsbError>>,
        clear_halts: u32,
        /// `clear_halt` fails this many more times.
        failing_clear_halts: u32,
        log: RefCell<String>,
    }

    impl Bus for FakeBus {
        fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError> {
            assert_eq!(slot, SLOT);
            self.requests.push((setup, data.to_vec()));
            if self.stall.contains(&setup.request) {
                return Err(UsbError::Stall);
            }
            Ok(data.len())
        }

        fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError> {
            assert_eq!(slot, SLOT);
            if self.queued.is_some() {
                return Err(UsbError::Unsupported("transfer already queued"));
            }
            self.queued = Some((endpoint, len));
            Ok(())
        }

        fn take_in(
            &mut self,
            _slot: u8,
            endpoint: u8,
            buf: &mut [u8],
        ) -> Option<Result<usize, UsbError>> {
            let (ep, len) = self.queued?;
            assert_eq!(ep, endpoint);
            let r = self.reports.pop_front()?;
            self.queued = None;
            Some(r.map(|data| {
                let n = data.len().min(len).min(buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                n
            }))
        }

        fn clear_halt(&mut self, _slot: u8, _endpoint: u8) -> Result<(), UsbError> {
            self.clear_halts += 1;
            if self.failing_clear_halts > 0 {
                self.failing_clear_halts -= 1;
                return Err(UsbError::Timeout);
            }
            self.queued = None;
            Ok(())
        }

        fn now(&self) -> Duration {
            self.now.get()
        }

        fn log(&self, args: fmt::Arguments) {
            let mut log = self.log.borrow_mut();
            log.write_fmt(args).unwrap();
            log.push('\n');
        }
    }

    impl FakeBus {
        fn advance(&self, ms: u64) {
            self.now.set(self.now.get() + Duration::from_millis(ms));
        }
    }

    /// The K120's first interface: a boot keyboard with an 8-byte
    /// interrupt-IN endpoint polled every 10 ms.
    fn k120_keyboard() -> Interface {
        Interface {
            number: 0,
            class: 3,
            subclass: 1,
            protocol: 1,
            endpoints: vec![Endpoint {
                address: 0x81,
                kind: EndpointKind::Interrupt,
                max_packet: 8,
                interval: 10,
                max_burst: 0,
            }],
        }
    }

    fn started() -> (FakeBus, BootKeyboard) {
        let mut bus = FakeBus::default();
        let kbd = BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).unwrap();
        bus.requests.clear();
        (bus, kbd)
    }

    fn report(modifiers: u8, keys: &[u8]) -> Result<Vec<u8>, UsbError> {
        let mut r = vec![modifiers, 0, 0, 0, 0, 0, 0, 0];
        r[2..2 + keys.len()].copy_from_slice(keys);
        Ok(r)
    }

    /// Delivers reports one poll at a time and returns the events.
    fn feed(
        bus: &mut FakeBus,
        kbd: &mut BootKeyboard,
        reports: &[Result<Vec<u8>, UsbError>],
    ) -> Vec<KeyEvent> {
        let mut out = VecDeque::new();
        for r in reports {
            bus.reports.push_back(r.clone());
            kbd.poll(bus, &mut out);
        }
        out.into_iter().collect()
    }

    /// The characters of the key-down events.
    fn typed(events: &[KeyEvent]) -> String {
        events
            .iter()
            .filter(|e| e.pressed)
            .map(|e| match e.key {
                Key::Char(c) => c as char,
                Key::Enter => '\n',
                _ => '?',
            })
            .collect()
    }

    fn down(c: u8) -> KeyEvent {
        KeyEvent {
            key: Key::Char(c),
            modifiers: Modifiers::default(),
            pressed: true,
        }
    }

    fn up(c: u8) -> KeyEvent {
        KeyEvent {
            pressed: false,
            ..down(c)
        }
    }

    #[test]
    fn boot_keyboards_are_recognised() {
        assert!(is_boot_keyboard(&k120_keyboard()));
        // The K120's second interface: HID, but no boot protocol.
        let mut extra_keys = k120_keyboard();
        (extra_keys.subclass, extra_keys.protocol) = (0, 0);
        assert!(!is_boot_keyboard(&extra_keys));
        let mut mouse = k120_keyboard();
        mouse.protocol = 2;
        assert!(!is_boot_keyboard(&mouse));
        let mut out_only = k120_keyboard();
        out_only.endpoints[0].address = 0x01;
        assert!(!is_boot_keyboard(&out_only));
        let mut no_endpoint = k120_keyboard();
        no_endpoint.endpoints.clear();
        assert!(!is_boot_keyboard(&no_endpoint));
    }

    #[test]
    fn starting_selects_the_boot_protocol_and_queues_a_report() {
        let mut bus = FakeBus::default();
        let kbd = BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).unwrap();
        let setups: Vec<Setup> = bus.requests.iter().map(|r| r.0).collect();
        assert_eq!(
            setups,
            [
                Setup {
                    request_type: 0x21,
                    request: 0x0B,
                    value: 0,
                    index: 0,
                    length: 0
                },
                Setup {
                    request_type: 0x21,
                    request: 0x0A,
                    value: 0,
                    index: 0,
                    length: 0
                },
            ]
        );
        assert_eq!(bus.queued, Some((0x81, 8)));
        assert_eq!((kbd.slot(), kbd.interface()), (SLOT, 0));
    }

    #[test]
    fn a_keyboard_that_refuses_the_boot_protocol_is_not_used() {
        let mut bus = FakeBus {
            stall: vec![0x0B],
            ..FakeBus::default()
        };
        assert_eq!(
            BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).err(),
            Some(UsbError::Stall)
        );
        assert_eq!(bus.queued, None);
    }

    #[test]
    fn a_stalled_set_idle_is_only_logged() {
        let mut bus = FakeBus {
            stall: vec![0x0A],
            ..FakeBus::default()
        };
        assert!(BootKeyboard::start(&mut bus, SLOT, &k120_keyboard()).is_ok());
        assert!(bus.log.borrow().contains("SET_IDLE stalled, continuing"));
        assert!(bus.queued.is_some());
    }

    #[test]
    fn presses_and_releases_come_from_comparing_reports() {
        let (mut bus, mut kbd) = started();
        // h, h+i (rollover), i, nothing.
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0, &[0x0B]),
                report(0, &[0x0B, 0x0C]),
                report(0, &[0x0C]),
                report(0, &[]),
            ],
        );
        assert_eq!(events, [down(b'h'), down(b'i'), up(b'h'), up(b'i')]);
        // Every report was followed by a new transfer.
        assert!(bus.queued.is_some());
        // A key that moves to another slot of the report is still held.
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0, &[0x04, 0x05]),
                report(0, &[0x05, 0x04]),
                report(0, &[]),
            ],
        );
        assert_eq!(typed(&events), "ab");
        assert_eq!(events.len(), 4);
        // A usage twice in one report is one key.
        let events = feed(
            &mut bus,
            &mut kbd,
            &[report(0, &[0x04, 0x04]), report(0, &[])],
        );
        assert_eq!(events, [down(b'a'), up(b'a')]);
    }

    #[test]
    fn nothing_happens_until_a_report_arrives() {
        let (mut bus, mut kbd) = started();
        let mut out = VecDeque::new();
        for _ in 0..100 {
            kbd.poll(&mut bus, &mut out);
        }
        assert!(out.is_empty());
        assert_eq!(bus.queued, Some((0x81, 8)));
    }

    #[test]
    fn shift_and_ctrl_come_with_the_event() {
        let (mut bus, mut kbd) = started();
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0x02, &[]),
                report(0x02, &[0x04]),
                report(0x01, &[0x06]),
            ],
        );
        assert_eq!(typed(&events), "Ac");
        assert!(events[0].modifiers.shift);
        assert!(events.last().unwrap().modifiers.ctrl);
    }

    #[test]
    fn rollover_error_reports_change_nothing() {
        let (mut bus, mut kbd) = started();
        let phantom = Ok(vec![0, 0, 1, 1, 1, 1, 1, 1]);
        let events = feed(
            &mut bus,
            &mut kbd,
            &[
                report(0, &[0x04]),
                phantom,
                report(0x02, &[0x04, 1]),
                report(0, &[]),
            ],
        );
        // One press, no phantom presses or releases, then the release.
        assert_eq!(events, [down(b'a'), up(b'a')]);
    }

    #[test]
    fn short_reports_are_padded_and_tiny_ones_ignored() {
        let (mut bus, mut kbd) = started();
        let events = feed(
            &mut bus,
            &mut kbd,
            &[Ok(vec![0, 0, 0x04]), Ok(vec![0, 0]), Ok(vec![0, 0, 0])],
        );
        assert_eq!(events, [down(b'a'), up(b'a')]);
    }

    #[test]
    fn caps_lock_toggles_letters_and_the_led() {
        let (mut bus, mut kbd) = started();
        let events = feed(&mut bus, &mut kbd, &[report(0, &[0x39]), report(0, &[])]);
        assert_eq!(events[0].key, Key::CapsLock);
        assert!(events[0].modifiers.caps_lock);
        kbd.service(&mut bus);
        let led = Setup {
            request_type: 0x21,
            request: 0x09,
            value: 0x0200,
            index: 0,
            length: 1,
        };
        assert_eq!(bus.requests, [(led, vec![0x02])]);
        // The LED is only sent when it changes.
        kbd.service(&mut bus);
        assert_eq!(bus.requests.len(), 1);
        let events = feed(
            &mut bus,
            &mut kbd,
            &[report(0, &[0x04]), report(0x02, &[0x05]), report(0, &[])],
        );
        assert_eq!(typed(&events), "Ab");
        feed(&mut bus, &mut kbd, &[report(0, &[0x39]), report(0, &[])]);
        kbd.service(&mut bus);
        assert_eq!(bus.requests[1], (led, vec![0x00]));
        let events = feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        assert_eq!(typed(&events), "a");
    }

    #[test]
    fn a_keyboard_without_leds_is_asked_once() {
        let (mut bus, mut kbd) = started();
        bus.stall = vec![0x09];
        feed(&mut bus, &mut kbd, &[report(0, &[0x39]), report(0, &[])]);
        kbd.service(&mut bus);
        kbd.service(&mut bus);
        assert_eq!(bus.requests.len(), 1);
        assert!(bus.log.borrow().contains("LED report stalled"));
    }

    fn repeats_between(
        bus: &mut FakeBus,
        kbd: &mut BootKeyboard,
        from_ms: u64,
        to_ms: u64,
    ) -> usize {
        let mut out = VecDeque::new();
        for _ in from_ms..to_ms {
            bus.advance(1);
            kbd.poll(bus, &mut out);
        }
        out.iter().filter(|e| e.pressed).count()
    }

    #[test]
    fn a_held_key_repeats_after_500_ms_at_30_per_second() {
        let (mut bus, mut kbd) = started();
        assert_eq!(feed(&mut bus, &mut kbd, &[report(0, &[0x04])]).len(), 1);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 499), 0);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 499, 500), 1);
        // 500 ms more: 15 repeats (one every 33.3 ms).
        assert_eq!(repeats_between(&mut bus, &mut kbd, 500, 1000), 15);
        let events = feed(&mut bus, &mut kbd, &[report(0, &[])]);
        assert_eq!(events, [up(b'a')]);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 1000, 2000), 0);
    }

    #[test]
    fn a_long_pause_gives_one_repeat_not_a_burst() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        bus.advance(5000);
        let mut out = VecDeque::new();
        kbd.poll(&mut bus, &mut out);
        kbd.poll(&mut bus, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 34), 1);
    }

    #[test]
    fn the_newest_key_repeats_and_lock_keys_do_not() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        bus.advance(300);
        feed(&mut bus, &mut kbd, &[report(0, &[0x04, 0x05])]);
        let mut out = VecDeque::new();
        for _ in 0..500 {
            bus.advance(1);
            kbd.poll(&mut bus, &mut out);
        }
        assert_eq!(typed(out.make_contiguous()), "b");
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x39])]);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 2000), 0);
    }

    #[test]
    fn releasing_another_key_keeps_the_repeat() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        feed(&mut bus, &mut kbd, &[report(0, &[0x04, 0x05])]);
        feed(&mut bus, &mut kbd, &[report(0, &[0x05])]);
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 500), 1);
    }

    #[test]
    fn a_failed_endpoint_releases_held_keys_and_recovers_in_service() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        let events = feed(&mut bus, &mut kbd, &[Err(UsbError::Stall)]);
        assert_eq!(events, [up(b'a')]);
        // Nothing repeats while the endpoint is down, and nothing is queued.
        assert_eq!(repeats_between(&mut bus, &mut kbd, 0, 1000), 0);
        assert_eq!(bus.queued, None);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 1);
        assert_eq!(bus.queued, Some((0x81, 8)));
        let events = feed(&mut bus, &mut kbd, &[report(0, &[0x05])]);
        assert_eq!(typed(&events), "b");
        assert!(bus.log.borrow().contains("keyboard recovered"));
    }

    #[test]
    fn recovery_is_tried_once_a_second_then_given_up() {
        let (mut bus, mut kbd) = started();
        bus.failing_clear_halts = 10;
        feed(&mut bus, &mut kbd, &[Err(UsbError::Transfer(4))]);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 1);
        for _ in 0..999 {
            bus.advance(1);
            kbd.service(&mut bus);
        }
        assert_eq!(bus.clear_halts, 1);
        bus.advance(1);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 2);
        assert!(!kbd.is_stopped());
        bus.advance(1000);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, MAX_RECOVERIES);
        assert!(kbd.is_stopped());
        bus.advance(5000);
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, MAX_RECOVERIES);
        assert!(bus.log.borrow().contains("keyboard given up"));
    }

    #[test]
    fn a_disconnected_keyboard_stops_at_once() {
        let (mut bus, mut kbd) = started();
        feed(&mut bus, &mut kbd, &[report(0, &[0x04])]);
        let events = feed(&mut bus, &mut kbd, &[Err(UsbError::Disconnected)]);
        assert_eq!(events, [up(b'a')]);
        assert!(kbd.is_stopped());
        kbd.service(&mut bus);
        assert_eq!(bus.clear_halts, 0);
    }
}
