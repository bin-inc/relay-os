//! Who may change the console (spec §6.4, §16 item 9): the chain of
//! process groups that handed it on, from process 1's group at the bottom
//! to the foreground group at the top. A group gives the console to
//! another (`spawn`'s `FOREGROUND`, `console_foreground`) only while it is
//! in the chain, and the chain is then cut back to it before the other is
//! put on top; a group taking the console back is cut back to. So a shell
//! takes the console back after its command's group has ended, nested
//! shells and scripts each take it back from theirs, and a background job,
//! which was never given it, can neither take it nor change its mode.

use super::table::{INIT, MAX};
use vfs::Errno;

/// The groups, each given the console by the one below it.
pub struct Holders {
    groups: [u32; MAX],
    len: usize,
}

impl Default for Holders {
    fn default() -> Self {
        Holders::new()
    }
}

impl Holders {
    /// Process 1's group alone.
    pub const fn new() -> Holders {
        let mut groups = [0; MAX];
        groups[0] = INIT;
        Holders { groups, len: 1 }
    }

    /// The group at the top, which reads the console.
    pub fn foreground(&self) -> u32 {
        self.groups[self.len - 1]
    }

    /// Whether the group `pgid` may change the console.
    pub fn holds(&self, pgid: u32) -> bool {
        self.groups[..self.len].contains(&pgid)
    }

    /// The group `by` gives the console to the group `to`: the chain is
    /// cut back to `by`, then to `to` if it holds the console already (it
    /// takes it back), and otherwise `to` goes on top. First the groups
    /// that no longer exist (`exists`) leave the chain, process 1's never,
    /// so it holds at most one entry per group there is. `EPERM` if `by`
    /// holds none of the console.
    pub fn give(&mut self, by: u32, to: u32, exists: impl Fn(u32) -> bool) -> Result<(), Errno> {
        let mut kept = 1;
        for i in 1..self.len {
            let g = self.groups[i];
            if exists(g) {
                self.groups[kept] = g;
                kept += 1;
            }
        }
        self.len = kept;
        let at = self.position(by).ok_or(Errno::EPERM)?;
        self.len = at + 1;
        match self.position(to) {
            Some(below) => self.len = below + 1,
            // One entry per group there is, and `to` is one: it fits.
            None if self.len < MAX => {
                self.groups[self.len] = to;
                self.len += 1;
            }
            None => return Err(Errno::EAGAIN),
        }
        Ok(())
    }

    /// Process 1 takes the console back from everyone (the error screen).
    pub fn reset(&mut self) {
        self.len = 1;
    }

    fn position(&self, pgid: u32) -> Option<usize> {
        self.groups[..self.len].iter().position(|&g| g == pgid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(h: &Holders) -> &[u32] {
        &h.groups[..h.len]
    }

    #[test]
    fn process_1_holds_the_console_at_first() {
        let h = Holders::new();
        assert_eq!((chain(&h), h.foreground()), (&[1][..], 1));
        assert!(h.holds(1) && !h.holds(2));
    }

    #[test]
    fn a_holder_gives_it_on_and_takes_it_back() {
        let mut h = Holders::new();
        let all = |_| true;
        h.give(1, 2, all).unwrap();
        h.give(2, 5, all).unwrap();
        assert_eq!(chain(&h), [1, 2, 5]);
        assert_eq!(h.foreground(), 5);
        // A nested shell, and its command.
        h.give(5, 8, all).unwrap();
        h.give(8, 9, all).unwrap();
        assert_eq!(chain(&h), [1, 2, 5, 8, 9]);
        // The nested shell takes it back, then the outer one.
        h.give(8, 8, all).unwrap();
        assert_eq!(chain(&h), [1, 2, 5, 8]);
        h.give(2, 2, all).unwrap();
        assert_eq!(chain(&h), [1, 2]);
        // Given to a group below the giver: that one takes it back.
        h.give(2, 6, all).unwrap();
        h.give(6, 2, all).unwrap();
        assert_eq!(chain(&h), [1, 2]);
        h.give(2, 1, all).unwrap();
        assert_eq!(chain(&h), [1]);
    }

    #[test]
    fn a_group_never_given_the_console_cannot_take_it() {
        let mut h = Holders::new();
        let all = |_| true;
        h.give(1, 2, all).unwrap();
        // A background job of the shell's.
        assert_eq!(h.give(7, 7, all), Err(Errno::EPERM));
        assert_eq!(h.give(7, 2, all), Err(Errno::EPERM));
        assert!(!h.holds(7));
        assert_eq!(chain(&h), [1, 2], "nothing changed");
        // A group the console was taken back from is out of the chain.
        h.give(2, 5, all).unwrap();
        h.give(2, 2, all).unwrap();
        assert_eq!(h.give(5, 5, all), Err(Errno::EPERM));
        // Nor one above a giver further down: process 1 starts a new shell
        // while the last one's command still runs.
        h.give(2, 5, all).unwrap();
        h.give(1, 9, all).unwrap();
        assert_eq!(chain(&h), [1, 9]);
        assert_eq!(h.give(5, 5, all), Err(Errno::EPERM));
    }

    #[test]
    fn groups_that_are_gone_leave_the_chain_but_process_1_s() {
        let mut h = Holders::new();
        let all = |_| true;
        h.give(1, 2, all).unwrap();
        h.give(2, 5, all).unwrap();
        h.give(5, 8, all).unwrap();
        // Group 5 is gone (a shell that ended): 8 and 2 stay.
        h.give(8, 9, |g| g != 5).unwrap();
        assert_eq!(chain(&h), [1, 2, 8, 9]);
        h.give(2, 3, |g| g == 2 || g == 3).unwrap();
        assert_eq!(chain(&h), [1, 2, 3]);
        assert_eq!(h.give(2, 4, |g| g == 4), Err(Errno::EPERM), "2 is gone");
        assert_eq!(chain(&h), [1]);
        h.give(1, 4, |_| false).unwrap();
        assert_eq!(chain(&h), [1, 4]);
    }

    #[test]
    fn the_chain_holds_one_entry_per_group_at_most() {
        // Each group gives the console to a new one and ends: the chain
        // never grows past the groups that exist.
        let mut h = Holders::new();
        for g in 2..10_000u32 {
            h.give(g - 1, g, |x| x + 1 >= g).unwrap();
            assert!(h.len <= 3, "{g}: {:?}", chain(&h));
        }
        // And with every one still there, it is full at MAX groups.
        let mut h = Holders::new();
        for g in 2..=MAX as u32 {
            h.give(g - 1, g, |_| true).unwrap();
        }
        assert_eq!(h.len, MAX);
        assert_eq!(h.give(MAX as u32, 1000, |_| true), Err(Errno::EAGAIN));
        assert_eq!(h.foreground(), MAX as u32);
    }

    #[test]
    fn reset_gives_it_back_to_process_1() {
        let mut h = Holders::new();
        h.give(1, 2, |_| true).unwrap();
        h.give(2, 3, |_| true).unwrap();
        h.reset();
        assert_eq!(chain(&h), [1]);
        assert_eq!(h.give(2, 2, |_| true), Err(Errno::EPERM));
    }
}
