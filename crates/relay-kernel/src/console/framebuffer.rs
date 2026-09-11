use relay_core::console::Framebuffer;

pub struct VolatileFramebuffer {
    bytes: &'static mut [u8],
}

impl VolatileFramebuffer {
    pub fn new(bytes: &'static mut [u8]) -> Self {
        Self { bytes }
    }
}

impl Framebuffer for VolatileFramebuffer {
    fn read_byte(&self, offset: usize) -> u8 {
        // SAFETY: the adapter receives one validated framebuffer slice and core console offsets are bounded.
        unsafe { core::ptr::read_volatile(self.bytes.as_ptr().add(offset)) }
    }

    fn write_byte(&mut self, offset: usize, value: u8) {
        // SAFETY: the adapter receives one validated framebuffer slice and core console offsets are bounded.
        unsafe { core::ptr::write_volatile(self.bytes.as_mut_ptr().add(offset), value) };
    }
}
