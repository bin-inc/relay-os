//! The call numbers (spec §7.3).

/// The system calls, numbered from 1 in the order of spec §7.3. A number is
/// never reused.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Exit = 1,
    Spawn,
    Wait,
    Kill,
    Getpid,
    ProcList,
    MemMap,
    MemUnmap,
    Open,
    Close,
    Read,
    Write,
    Seek,
    Fstat,
    Stat,
    ReadDir,
    Mkdir,
    Rmdir,
    Unlink,
    Truncate,
    Touch,
    Readlink,
    Rename,
    Statfs,
    Sync,
    Chdir,
    Getcwd,
    ConsoleMode,
    ConsoleSize,
    ConsoleForeground,
    ConsoleTeePush,
    ConsoleTeePop,
    Time,
    Sleep,
    SysInfo,
    Power,
    Pipe,
}

impl Call {
    /// Every call, in number order.
    pub const ALL: [Call; 37] = [
        Call::Exit,
        Call::Spawn,
        Call::Wait,
        Call::Kill,
        Call::Getpid,
        Call::ProcList,
        Call::MemMap,
        Call::MemUnmap,
        Call::Open,
        Call::Close,
        Call::Read,
        Call::Write,
        Call::Seek,
        Call::Fstat,
        Call::Stat,
        Call::ReadDir,
        Call::Mkdir,
        Call::Rmdir,
        Call::Unlink,
        Call::Truncate,
        Call::Touch,
        Call::Readlink,
        Call::Rename,
        Call::Statfs,
        Call::Sync,
        Call::Chdir,
        Call::Getcwd,
        Call::ConsoleMode,
        Call::ConsoleSize,
        Call::ConsoleForeground,
        Call::ConsoleTeePush,
        Call::ConsoleTeePop,
        Call::Time,
        Call::Sleep,
        Call::SysInfo,
        Call::Power,
        Call::Pipe,
    ];

    pub const fn number(self) -> u64 {
        self as u64
    }

    /// The call with number `n`; `None` for a number no call has (the
    /// kernel answers those with `ENOSYS`).
    pub fn from_number(n: u64) -> Option<Call> {
        let i = usize::try_from(n.checked_sub(1)?).ok()?;
        Call::ALL.get(i).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_are_numbered_from_1_in_spec_order() {
        for (i, c) in Call::ALL.iter().enumerate() {
            assert_eq!(c.number(), i as u64 + 1, "{c:?}");
        }
        assert_eq!(Call::Exit.number(), 1);
        assert_eq!(Call::Write.number(), 12);
        assert_eq!(Call::Power.number(), 36);
        assert_eq!(Call::Pipe.number(), 37);
    }

    #[test]
    fn numbers_map_back_to_their_calls() {
        for c in Call::ALL {
            assert_eq!(Call::from_number(c.number()), Some(c));
        }
        for n in [0, 38, 1000, u64::MAX] {
            assert_eq!(Call::from_number(n), None, "{n}");
        }
    }
}
