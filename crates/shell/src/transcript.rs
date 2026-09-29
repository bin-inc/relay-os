//! A running script's transcript (spec §15 item 12): what the screen shows,
//! written into a file next to the script as the script goes.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{Errno, Node, Vfs, path};

/// Screen output waits on the heap until this much has come, or the line
/// ends: a command that prints a big file must not hold all of it.
pub const CHUNK: usize = 4096;

pub(crate) struct Transcript {
    node: Node,
    offset: u64,
    pending: Vec<u8>,
    name: String,
}

impl Transcript {
    /// The transcript file `node`, empty, called `name` in messages.
    pub fn new(node: Node, name: String) -> Transcript {
        Transcript {
            node,
            offset: 0,
            pending: Vec::new(),
            name,
        }
    }

    /// Adds what the screen showed; writes it once `CHUNK` bytes wait.
    pub fn add(&mut self, vfs: &mut dyn Vfs, bytes: &[u8]) -> Result<(), Errno> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() >= CHUNK {
            self.write(vfs)?;
        }
        Ok(())
    }

    /// Writes everything that waits.
    pub fn write(&mut self, vfs: &mut dyn Vfs) -> Result<(), Errno> {
        let mut done = 0;
        while done < self.pending.len() {
            match vfs.write_at(self.node, self.offset, &self.pending[done..]) {
                // Nothing written would loop forever; the contract says
                // that is ENOSPC.
                Ok(0) => return Err(Errno::ENOSPC),
                Ok(n) => {
                    done += n;
                    self.offset += n as u64;
                }
                Err(e) => return Err(e),
            }
        }
        self.pending.clear();
        Ok(())
    }

    /// What the screen says when a write failed; the transcript ends there.
    pub fn ended(&self, e: Errno) -> String {
        let name = path::display(self.name.as_bytes());
        format!("sh: {name}: {e}; the transcript ends here\n")
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::Harness;

    #[test]
    fn a_command_that_prints_much_is_written_as_it_goes() {
        // `cat` of a big file inside a script: its output reaches the
        // transcript in pieces, not all at once when the line ends.
        let mut h = Harness::new();
        let big = alloc::vec![b'x'; 200_000];
        h.put("/tmp/big", &big);
        h.put("/tmp/s.sh", b"cat /tmp/big\n");
        h.spy.largest_write.set(0);
        assert_eq!(h.run("sh /tmp/s.sh").0, 0);
        let log = h.get("/tmp/s.log");
        assert_eq!(&log[..15], b"+ cat /tmp/big\n");
        assert_eq!(&log[15..], &big[..]);
        // cat hands the screen 64 KiB at a time.
        assert!(h.spy.largest_write.get() <= 64 * 1024 + super::CHUNK);
    }
}
