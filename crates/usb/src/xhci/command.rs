//! Commands (xHCI 4.6) and events (xHCI 4.9.4): one command at a time on
//! the command ring, and `poll`, which drains the event ring and hands
//! each event to whoever waits for it.

use super::Xhci;
use super::init::{REGISTER_TIMEOUT, wait_for};
use super::regs::{ALL_ONES, CA, CRCR, EHB, ERDP, HCE, HCH, HSE, RUN, USBCMD, USBSTS};
use super::ring::TRBS;
use super::trb::{
    COMMAND_ABORTED, COMMAND_COMPLETION, COMMAND_RING_STOPPED, HOST_CONTROLLER_EVENT, SUCCESS, Trb,
    command_name, completion_name,
};
use crate::{Hal, UsbError};
use core::sync::atomic::{Ordering, fence};
use core::time::Duration;

/// How long a command may take (spec §6.2).
pub const COMMAND_TIMEOUT: Duration = Duration::from_millis(500);
/// How often a command wait polls.
const COMMAND_POLL: Duration = Duration::from_micros(10);

/// What a Command Completion Event said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub code: u8,
    /// The slot concerned (Enable Slot's new slot).
    pub slot: u8,
}

/// The command in flight: its TRB's address, and its completion once the
/// event has come.
#[derive(Debug)]
pub struct Pending {
    trb: u64,
    done: Option<Completion>,
}

impl<H: Hal> Xhci<H> {
    /// Runs one command and waits up to 500 ms for its completion. On a
    /// timeout the command ring is aborted and the command turned into a
    /// No Op; if even the abort fails, the controller is dead.
    pub(super) fn command(&mut self, trb: Trb) -> Result<Completion, UsbError> {
        if self.dead {
            return Err(UsbError::ControllerDead);
        }
        let kind = trb.trb_type();
        let addr = self.commands.push(trb);
        self.pending = Some(Pending {
            trb: addr,
            done: None,
        });
        // The controller reads the TRB after it sees the doorbell.
        fence(Ordering::SeqCst);
        self.regs.ring_doorbell(&self.hal, 0, 0);
        let start = self.hal.now();
        loop {
            self.poll();
            if self.dead {
                self.pending = None;
                return Err(UsbError::ControllerDead);
            }
            if let Some(done) = self.pending.as_ref().and_then(|p| p.done) {
                self.pending = None;
                if done.code == SUCCESS {
                    return Ok(done);
                }
                xlog!(
                    &self.hal,
                    &self.name,
                    "command {} failed: {}",
                    command_name(kind),
                    completion_name(done.code)
                );
                return Err(UsbError::Command(done.code));
            }
            if self.hal.now() - start >= COMMAND_TIMEOUT {
                break;
            }
            self.hal.sleep(COMMAND_POLL);
        }
        self.pending = None;
        self.abort_command(addr, kind);
        Err(UsbError::Timeout)
    }

    /// Aborts the command ring (xHCI 4.6.1.2) and polls up to 1 s for its
    /// Command Ring Stopped event. CRCR is written whole, CA with the
    /// timed-out TRB's address, as Linux does: some controllers act only
    /// once the high half is written. The controller may resume on the
    /// timed-out TRB or after it; as a No Op with the same cycle bit it does
    /// nothing either way.
    fn abort_command(&mut self, addr: u64, kind: u8) {
        self.ring_stopped = false;
        self.regs
            .op_write64(&self.hal, CRCR, addr & !0x3F | CA as u64);
        let start = self.hal.now();
        while !self.ring_stopped && !self.dead && self.hal.now() - start < REGISTER_TIMEOUT {
            self.poll();
            self.hal.sleep(COMMAND_POLL);
        }
        self.commands.replace(addr, Trb::no_op_command());
        if !self.ring_stopped {
            xlog!(
                &self.hal,
                &self.name,
                "command {} timed out and the command ring did not stop",
                command_name(kind)
            );
            self.controller_dead(true);
            return;
        }
        xlog!(
            &self.hal,
            &self.name,
            "command {} timed out; aborted",
            command_name(kind)
        );
    }

    /// Processes every pending event (command completions, transfer
    /// events, port status changes) and never waits.
    pub fn poll(&mut self) {
        if self.dead {
            return;
        }
        let sts = self.regs.op_read(&self.hal, USBSTS);
        if sts == ALL_ONES || sts & (HSE | HCE) != 0 {
            xlog!(
                &self.hal,
                &self.name,
                "USBSTS {sts:#010x}: host system error, controller error or gone"
            );
            // `poll` never waits: on a host system error the controller
            // clears R/S itself (xHCI 5.4.2), so it is only cleared here.
            self.controller_dead(false);
            return;
        }
        // At most one segment's worth, so a controller that keeps writing
        // cannot hold `poll` forever.
        let mut seen = 0;
        while seen < TRBS {
            let Some(event) = self.events.next() else {
                break;
            };
            seen += 1;
            self.handle_event(event);
        }
        if seen > 0 {
            // EHB is RW1C: writing 1 clears it (xHCI 5.5.2.3.3).
            let erdp = self.events.dequeue_pointer() | EHB;
            self.regs.intr_write64(&self.hal, ERDP, erdp);
        }
    }

    /// The controller stopped working: nothing more is sent to it, and it
    /// is halted (R/S = 0; with `wait`, up to 1 s for HCH) so it stops using
    /// memory. Its DMA memory is never freed after this.
    fn controller_dead(&mut self, wait: bool) {
        self.dead = true;
        let cmd = self.regs.op_read(&self.hal, USBCMD);
        if cmd == ALL_ONES {
            xlog!(&self.hal, &self.name, "controller dead and gone");
            return;
        }
        self.regs.op_write(&self.hal, USBCMD, cmd & !RUN);
        if !wait {
            xlog!(&self.hal, &self.name, "controller dead; R/S cleared");
            return;
        }
        let (hal, regs) = (&self.hal, &self.regs);
        match wait_for(hal, REGISTER_TIMEOUT, || {
            regs.op_read(hal, USBSTS) & HCH != 0
        }) {
            Some(t) => xlog!(
                &self.hal,
                &self.name,
                "controller dead; halted after {} ms",
                t.as_millis()
            ),
            None => xlog!(
                &self.hal,
                &self.name,
                "controller dead; USBSTS.HCH still 0 {} ms after R/S = 0",
                REGISTER_TIMEOUT.as_millis()
            ),
        }
    }

    fn handle_event(&mut self, event: Trb) {
        match event.trb_type() {
            COMMAND_COMPLETION => self.command_completed(event),
            HOST_CONTROLLER_EVENT => xlog!(
                &self.hal,
                &self.name,
                "host controller event: {}",
                completion_name(event.completion_code())
            ),
            _ => {}
        }
    }

    fn command_completed(&mut self, event: Trb) {
        let done = Completion {
            code: event.completion_code(),
            slot: event.slot_id(),
        };
        // An abort's own events never complete a command, whatever they
        // point at: Command Ring Stopped names the TRB after the aborted
        // one, where the next command goes (xHCI 4.6.1.2).
        match done.code {
            COMMAND_RING_STOPPED => {
                self.ring_stopped = true;
                return;
            }
            COMMAND_ABORTED => return,
            _ => {}
        }
        match &mut self.pending {
            Some(p) if p.trb == event.pointer() && p.done.is_none() => p.done = Some(done),
            _ => xlog!(
                &self.hal,
                &self.name,
                "completion for command {:#x} matches no command ({})",
                event.pointer(),
                completion_name(done.code)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::trb::{ENABLE_SLOT, NO_OP_COMMAND};
    use super::*;
    use crate::testing::{FakeConfig, FakeHal, start};

    fn no_op(xhci: &mut Xhci<FakeHal>) -> Result<Completion, UsbError> {
        xhci.command(Trb::no_op_command())
    }

    /// An event software does not wait for (MFINDEX Wrap, type 39).
    fn inject(hal: &FakeHal, n: usize) {
        hal.act(|x, dma| {
            for _ in 0..n {
                x.post([0, 0, 1 << 24, 39 << 10], dma);
            }
        });
    }

    #[test]
    fn a_no_op_command_completes() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(no_op(&mut xhci).map(|c| c.code), Ok(SUCCESS));
        assert_eq!(hal.fake().executed().len(), 1);
        assert_eq!(hal.fake().executed()[0].kind, NO_OP_COMMAND as u32);
    }

    #[test]
    fn six_hundred_commands_in_a_row_all_complete() {
        // The command ring (255 TRBs a lap) and the event ring (256) wrap
        // more than twice.
        let (hal, mut xhci) = start(FakeConfig::intel());
        for i in 0..600 {
            assert_eq!(
                no_op(&mut xhci),
                Ok(Completion {
                    code: SUCCESS,
                    slot: 0
                }),
                "command {i}"
            );
        }
        assert_eq!(hal.fake().executed().len(), 600);
        assert_eq!(hal.fake().unconsumed_events(), 0);
    }

    #[test]
    fn enable_slot_names_the_slot_and_failures_carry_their_code() {
        let (_hal, mut xhci) = start(FakeConfig::basic());
        assert_eq!(xhci.command(Trb::enable_slot()).map(|c| c.slot), Ok(1));
        assert_eq!(xhci.command(Trb::enable_slot()).map(|c| c.slot), Ok(2));
        assert_eq!(
            xhci.command(Trb::disable_slot(9)),
            Err(UsbError::Command(super::super::trb::SLOT_NOT_ENABLED))
        );
        assert!(
            xhci.hal()
                .log_text()
                .contains("command Disable Slot failed: slot not enabled")
        );
    }

    #[test]
    fn more_than_256_events_are_all_seen_and_erdp_is_written_with_ehb() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        let base = hal.fake().event_ring().unwrap().base;
        inject(&hal, 3);
        assert!(hal.fake().ehb());
        let writes = hal.fake().erdp_writes();
        xhci.poll();
        assert_eq!(hal.fake().erdp(), base + 3 * 16);
        assert!(!hal.fake().ehb(), "EHB cleared");
        assert_eq!(
            hal.fake().erdp_writes(),
            writes + 1,
            "one ERDP write per poll"
        );
        for _ in 0..3 {
            inject(&hal, 200);
            xhci.poll();
            assert_eq!(hal.fake().unconsumed_events(), 0);
            assert!(!hal.fake().ehb());
        }
        assert_eq!(hal.fake().erdp(), base + (603 % 256) * 16);
    }

    #[test]
    fn a_command_that_never_completes_is_aborted_and_the_next_gets_its_own_completion() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(ENABLE_SLOT as u32);
        let (hal, mut xhci) = start(config);
        let before = hal.clock();
        assert_eq!(xhci.command(Trb::enable_slot()), Err(UsbError::Timeout));
        let waited = hal.clock() - before;
        assert!(waited >= COMMAND_TIMEOUT && waited < COMMAND_TIMEOUT * 2);
        assert_eq!(hal.fake().aborts(), 1);
        assert_eq!(hal.fake().hung(), None, "the abort released the ring");
        assert!(
            hal.log_text()
                .contains("xhci 00:14.0: command Enable Slot timed out; aborted")
        );
        // Command Ring Stopped named the TRB after the aborted one, where
        // this command goes: its completion is its own, not that event.
        assert_eq!(
            no_op(&mut xhci),
            Ok(Completion {
                code: SUCCESS,
                slot: 0
            })
        );
        let executed = hal.fake().executed().to_vec();
        assert_eq!(executed.len(), 1);
        assert_eq!(executed[0].kind, NO_OP_COMMAND as u32);
        assert!(!hal.fake().slot_enabled(1), "Enable Slot never ran");
        assert!(!hal.log_text().contains("matches no command"));
    }

    #[test]
    fn a_controller_that_resumes_on_the_aborted_command_finds_a_no_op() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(ENABLE_SLOT as u32);
        config.abort_keeps_dequeue = true;
        let (hal, mut xhci) = start(config);
        assert_eq!(xhci.command(Trb::enable_slot()), Err(UsbError::Timeout));
        // The ring restarts on the old TRB, now a No Op, then runs the next.
        assert_eq!(no_op(&mut xhci).map(|c| c.code), Ok(SUCCESS));
        let executed = hal.fake().executed().to_vec();
        assert_eq!(executed.len(), 2);
        assert!(executed.iter().all(|e| e.kind == NO_OP_COMMAND as u32));
        assert!(!hal.fake().slot_enabled(1), "Enable Slot never ran");
    }

    #[test]
    fn an_abort_that_never_finishes_kills_and_halts_the_controller() {
        let mut config = FakeConfig::basic();
        config.hang_command = Some(ENABLE_SLOT as u32);
        config.abort_never_completes = true;
        let (hal, mut xhci) = start(config);
        assert_eq!(xhci.command(Trb::enable_slot()), Err(UsbError::Timeout));
        let log = hal.log_text();
        assert!(log.contains("command Enable Slot timed out and the command ring did not stop"));
        assert!(
            log.contains("xhci 00:14.0: controller dead; halted after 0 ms"),
            "{log}"
        );
        assert_eq!(hal.fake().usbcmd() & RUN, 0, "R/S cleared");
        assert!(!hal.fake().running());
        let before = hal.clock();
        assert_eq!(no_op(&mut xhci), Err(UsbError::ControllerDead));
        assert_eq!(hal.clock(), before, "failing at once");
    }

    #[test]
    fn a_completion_for_no_command_is_ignored() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        hal.act(|x, dma| x.post_completion(0x1234_5670, 1, 0, dma));
        xhci.poll();
        assert!(
            hal.log_text()
                .contains("completion for command 0x12345670 matches no command (success)")
        );
        assert_eq!(no_op(&mut xhci).map(|c| c.code), Ok(SUCCESS));
    }

    #[test]
    fn a_host_system_error_marks_the_controller_dead() {
        let (hal, mut xhci) = start(FakeConfig::intel());
        hal.fake().host_system_error();
        let before = hal.clock();
        xhci.poll();
        assert!(
            hal.clock() - before <= Duration::from_micros(5),
            "poll does not wait for the halt"
        );
        xhci.poll();
        assert_eq!(hal.fake().usbcmd() & RUN, 0, "R/S cleared");
        let log = hal.log_text();
        assert_eq!(log.matches("host system error").count(), 1, "logged once");
        assert_eq!(no_op(&mut xhci), Err(UsbError::ControllerDead));
    }

    #[test]
    fn poll_with_nothing_pending_returns_without_sleeping() {
        let (hal, mut xhci) = start(FakeConfig::basic());
        let before = hal.clock();
        let writes = hal.fake().erdp_writes();
        xhci.poll();
        assert!(hal.clock() - before <= Duration::from_micros(5));
        assert_eq!(
            hal.fake().erdp_writes(),
            writes,
            "no ERDP write without events"
        );
    }
}
