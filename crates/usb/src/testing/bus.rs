//! A [`Bus`] that changes what the one behind it returns, for faults the
//! fake controller cannot produce (a device that disappears at an exact
//! point, a short write).

use crate::{Bus, Setup, UsbError};
use core::time::Duration;

/// A [`Bus`] in front of another that changes what it returns: faults the
/// fake controller cannot produce.
pub struct TamperBus<'a> {
    pub inner: &'a mut dyn Bus,
    /// Every bulk transfer from this one on (counting from 0) fails with
    /// this error without being sent.
    pub fail_bulk: Option<(usize, UsbError)>,
    /// Every control request fails with this error without being sent.
    pub fail_control: Option<UsbError>,
    /// `bulk_out` reports one byte less than it sent.
    pub short_out: bool,
    /// Bulk transfers so far.
    pub bulk: usize,
}

impl<'a> TamperBus<'a> {
    pub fn new(inner: &'a mut dyn Bus) -> TamperBus<'a> {
        TamperBus {
            inner,
            fail_bulk: None,
            fail_control: None,
            short_out: false,
            bulk: 0,
        }
    }

    fn bulk_fault(&mut self) -> Option<UsbError> {
        self.bulk += 1;
        self.fail_bulk
            .and_then(|(from, e)| (self.bulk > from).then_some(e))
    }
}

impl Bus for TamperBus<'_> {
    fn control(&mut self, slot: u8, setup: Setup, data: &mut [u8]) -> Result<usize, UsbError> {
        match self.fail_control {
            Some(e) => Err(e),
            None => self.inner.control(slot, setup, data),
        }
    }

    fn queue_in(&mut self, slot: u8, endpoint: u8, len: usize) -> Result<(), UsbError> {
        self.inner.queue_in(slot, endpoint, len)
    }

    fn take_in(
        &mut self,
        slot: u8,
        endpoint: u8,
        buf: &mut [u8],
    ) -> Option<Result<usize, UsbError>> {
        self.inner.take_in(slot, endpoint, buf)
    }

    fn bulk_in(&mut self, slot: u8, endpoint: u8, buf: &mut [u8]) -> Result<usize, UsbError> {
        match self.bulk_fault() {
            Some(e) => Err(e),
            None => self.inner.bulk_in(slot, endpoint, buf),
        }
    }

    fn bulk_out(&mut self, slot: u8, endpoint: u8, data: &[u8]) -> Result<usize, UsbError> {
        if let Some(e) = self.bulk_fault() {
            return Err(e);
        }
        let n = self.inner.bulk_out(slot, endpoint, data)?;
        Ok(if self.short_out {
            n.saturating_sub(1)
        } else {
            n
        })
    }

    fn clear_halt(&mut self, slot: u8, endpoint: u8) -> Result<(), UsbError> {
        self.inner.clear_halt(slot, endpoint)
    }

    fn now(&self) -> Duration {
        self.inner.now()
    }

    fn sleep(&self, d: Duration) {
        self.inner.sleep(d)
    }

    fn log(&self, args: core::fmt::Arguments) {
        self.inner.log(args)
    }
}
