use relay_core::console::TextOutput;

pub struct SerialOutput;

impl TextOutput for SerialOutput {
    fn write_bytes(&mut self, bytes: &[u8]) {
        crate::serial::write(bytes);
    }
}
