//! `sleep`, `date`, `df`, `free`, `dmesg`, `ps`, `sync`, `reboot` and
//! `poweroff` (spec §7.3, §7.4, §9.3).

use crate::ctx::{Ctx, getopt, outln, quote};
use crate::time;
use alloc::format;
use alloc::string::String;

/// Options without operands, or the exit status after reporting.
fn no_operands(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &[String],
    flags: &str,
) -> Result<crate::ctx::Opts, i32> {
    let opts = getopt(args, flags, "").map_err(|e| ctx.fail(name, format_args!("{e}")))?;
    match opts.operands.first() {
        Some(extra) => Err(ctx.fail(name, format_args!("extra operand {}", quote(extra)))),
        None => Ok(opts),
    }
}

/// `sleep NUMBER[SUFFIX]...`: waits for the sum of the times given, each in
/// seconds, or with GNU's suffixes `s`, `m`, `h` or `d`; a number may have
/// a fraction, which counts to the millisecond.
pub fn sleep(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("sleep", format_args!("{e}")),
    };
    if opts.operands.is_empty() {
        return ctx.fail("sleep", format_args!("missing operand"));
    }
    let mut total: u64 = 0;
    for op in &opts.operands {
        match millis(op) {
            Some(ms) => total = total.saturating_add(ms),
            None => return ctx.fail("sleep", format_args!("invalid time interval {}", quote(op))),
        }
    }
    ctx.system.sleep(total);
    0
}

/// `NUMBER[SUFFIX]` in milliseconds, at most `u64::MAX`.
fn millis(s: &str) -> Option<u64> {
    let (number, unit): (&str, u128) = match s.as_bytes().last()? {
        b's' => (&s[..s.len() - 1], 1000),
        b'm' => (&s[..s.len() - 1], 60_000),
        b'h' => (&s[..s.len() - 1], 3_600_000),
        b'd' => (&s[..s.len() - 1], 86_400_000),
        _ => (s, 1000),
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return None;
    }
    // Beyond 20 digits a number is more than u64 milliseconds anyway, and
    // beyond 9 a fraction is less than one.
    let value = |t: &str| {
        t.bytes().fold(0u128, |n, b| {
            n.saturating_mul(10).saturating_add(u128::from(b - b'0'))
        })
    };
    let fraction = &fraction[..fraction.len().min(9)];
    let ms = value(&whole[..whole.len().min(30)])
        .saturating_mul(unit)
        .saturating_add(value(fraction) * unit / 10u128.pow(fraction.len() as u32));
    Some(u64::try_from(ms).unwrap_or(u64::MAX))
}

/// `date`: the wall clock in UTC. `-u` is accepted (it is UTC anyway).
pub fn date(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Err(status) = no_operands(ctx, "date", args, "u") {
        return status;
    }
    let now = time::date(ctx.system.now());
    outln!(ctx, "{now}");
    0
}

/// `df`: size, used and available space of `/`, in KiB.
pub fn df(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Err(status) = no_operands(ctx, "df", args, "") {
        return status;
    }
    let fs = match ctx.vfs.statfs(b"/") {
        Ok(fs) => fs,
        Err(e) => return ctx.fail("df", format_args!("/: {e}")),
    };
    let kib = |blocks: u64| blocks * fs.block_size / 1024;
    let (size, used, avail) = (
        kib(fs.blocks),
        kib(fs.blocks.saturating_sub(fs.free_blocks)),
        kib(fs.avail_blocks),
    );
    // GNU rounds the percentage up and leaves the reserved blocks out.
    let percent = match used + avail {
        0 => String::from("-"),
        total => format!("{}%", (used * 100).div_ceil(total)),
    };
    let header = [
        "Filesystem",
        "1K-blocks",
        "Used",
        "Available",
        "Use%",
        "Mounted on",
    ];
    let row = [
        String::from("/dev/root"),
        format!("{size}"),
        format!("{used}"),
        format!("{avail}"),
        percent,
        String::from("/"),
    ];
    let w: [usize; 5] = core::array::from_fn(|i| header[i].len().max(row[i].len()));
    let w0 = w[0].max(14);
    for cells in [header.map(String::from), row] {
        outln!(
            ctx,
            "{:<w0$} {:>w1$} {:>w2$} {:>w3$} {:>w4$} {}",
            cells[0],
            cells[1],
            cells[2],
            cells[3],
            cells[4],
            cells[5],
            w1 = w[1],
            w2 = w[2],
            w3 = w[3],
            w4 = w[4],
        );
    }
    0
}

/// `free`: memory and heap, in KiB.
pub fn free(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Err(status) = no_operands(ctx, "free", args, "") {
        return status;
    }
    let Some(m) = ctx.system.memory() else {
        return ctx.fail("free", format_args!("memory figures are not available"));
    };
    outln!(ctx, "{:<7}{:>12}{:>12}{:>12}", "", "total", "used", "free");
    let rows = [
        ("Mem:", m.ram_total, m.ram_total.saturating_sub(m.ram_free)),
        ("Heap:", m.heap_total, m.heap_used),
    ];
    for (name, total, used) in rows {
        let (total, used) = (total / 1024, used / 1024);
        outln!(
            ctx,
            "{name:<7}{total:>12}{used:>12}{:>12}",
            total.saturating_sub(used)
        );
    }
    0
}

/// `ps`: every process, by pid (spec §9.3, §16 item 9): its parent, what
/// it does (`run`, `ready`, or what it waits for: `wait` for a child,
/// `read` the console, `sleep`, `pipe`; `zombie` once it has ended), the
/// memory its address space holds in KiB, its CPU time as `m:ss`, and the
/// path it was started from. Process 0, the idle task, is no process.
pub fn ps(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Err(status) = no_operands(ctx, "ps", args, "") {
        return status;
    }
    let Some(list) = ctx.system.processes() else {
        return ctx.fail("ps", format_args!("processes are not available"));
    };
    outln!(
        ctx,
        "{:>5} {:>5} {:<6} {:>7} {:>5} CMD",
        "PID",
        "PPID",
        "STATE",
        "MEM",
        "TIME"
    );
    for p in list {
        let secs = p.ticks / 1000;
        let time = format!("{}:{:02}", secs / 60, secs % 60);
        let kib = p.frames.saturating_mul(4);
        let cmd = String::from_utf8_lossy(p.name());
        outln!(
            ctx,
            "{:>5} {:>5} {:<6} {kib:>7} {time:>5} {cmd}",
            p.pid,
            p.ppid,
            state(p.state)
        );
    }
    0
}

/// A process's state as `ps` says it.
fn state(s: u32) -> &'static str {
    use relay_abi::proc::*;
    match s {
        STATE_RUN => "run",
        STATE_READY => "ready",
        STATE_WAIT => "wait",
        STATE_READ => "read",
        STATE_SLEEP => "sleep",
        STATE_PIPE => "pipe",
        STATE_ZOMBIE => "zombie",
        _ => "?",
    }
}

/// `dmesg`: the kernel log.
pub fn dmesg(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Err(status) = no_operands(ctx, "dmesg", args, "") {
        return status;
    }
    let log = ctx.system.kernel_log();
    ctx.out(&log);
    0
}

/// `sync`: writes every cached change to the disk. (The shell also does
/// this after every command.)
pub fn sync(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Err(status) = no_operands(ctx, "sync", args, "") {
        return status;
    }
    match ctx.vfs.sync() {
        Ok(()) => 0,
        Err(e) => ctx.fail("sync", format_args!("{e}")),
    }
}

pub fn reboot(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    restart(ctx, "reboot", args)
}

pub fn poweroff(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    restart(ctx, "poweroff", args)
}

/// `reboot`/`poweroff [-f]`: shuts the filesystems down cleanly first
/// (spec §7.4). If that fails the machine stays up, so nothing is lost
/// silently; `-f` goes ahead anyway, after saying so. (A program's
/// filesystems are shut down by the kernel's `power`, which returns the
/// error instead: it is asked without `-f` first, so that `-f` says the
/// same.)
fn restart(ctx: &mut Ctx<'_>, name: &str, args: &[String]) -> i32 {
    let opts = match no_operands(ctx, name, args, "f") {
        Ok(o) => o,
        Err(status) => return status,
    };
    let force = opts.has('f');
    let unclean = |ctx: &mut Ctx<'_>, e| {
        ctx.fail(
            name,
            format_args!("cannot shut the filesystems down cleanly: {e}"),
        );
        if force {
            1
        } else {
            ctx.fail(name, format_args!("use '{name} -f' to go ahead anyway"))
        }
    };
    if let Err(e) = ctx.vfs.shutdown() {
        let status = unclean(ctx, e);
        if !force {
            return status;
        }
    }
    let go = |ctx: &mut Ctx<'_>, force| {
        if name == "reboot" {
            ctx.system.reboot(force)
        } else {
            ctx.system.poweroff(force)
        }
    };
    match go(ctx, false) {
        Ok(()) => {}
        Err(e) if force => {
            unclean(ctx, e);
            if go(ctx, true).is_err() {
                return 1;
            }
        }
        Err(e) => return unclean(ctx, e),
    }
    ctx.exit = true;
    0
}

#[cfg(test)]
mod tests {
    use crate::MemInfo;
    use crate::Shell;
    use crate::testing::Harness;
    use vfs::Errno;

    #[test]
    fn sleep_waits_for_the_sum_of_its_times_to_the_millisecond() {
        let mut h = Harness::new();
        for (line, ms) in [
            ("sleep 2", 2000),
            ("sleep 1.5 2m", 121_500),
            ("sleep .25", 250),
            ("sleep 5.", 5000),
            ("sleep 0.0004", 0),
            ("sleep 1h 1d 0s", 90_000_000),
            ("sleep 0.123456789987", 123),
            ("sleep 0.0001m 0.00001h", 6 + 36),
            ("sleep 99999999999999999999 1", u64::MAX),
            ("sleep -- 1", 1000),
        ] {
            h.system.slept.clear();
            assert_eq!(h.run(line), (0, "".into()), "{line}");
            assert_eq!(h.system.slept, [ms], "{line}");
        }
    }

    #[test]
    fn what_sleep_refuses() {
        let mut h = Harness::new();
        for (line, said) in [
            ("sleep", "sleep: missing operand\n"),
            ("sleep x", "sleep: invalid time interval 'x'\n"),
            ("sleep 1x", "sleep: invalid time interval '1x'\n"),
            ("sleep .", "sleep: invalid time interval '.'\n"),
            ("sleep 1.2.3", "sleep: invalid time interval '1.2.3'\n"),
            ("sleep s", "sleep: invalid time interval 's'\n"),
            ("sleep 1 y", "sleep: invalid time interval 'y'\n"),
            ("sleep -1", "sleep: invalid option -- '1'\n"),
        ] {
            assert_eq!(h.run(line), (1, said.into()), "{line}");
        }
        assert!(h.system.slept.is_empty(), "nothing is waited for");
    }

    #[test]
    fn date_prints_utc() {
        let mut h = Harness::new();
        assert_eq!(h.run("date"), (0, "Sat Sep 26 12:00:00 UTC 2026\n".into()));
        assert_eq!(
            h.run("date -u"),
            (0, "Sat Sep 26 12:00:00 UTC 2026\n".into())
        );
        assert_eq!(h.run("date +%s"), (1, "date: extra operand '+%s'\n".into()));
    }

    #[test]
    fn df_shows_the_root_filesystem() {
        let mut h = Harness::with_capacity(100 * 4096);
        let (status, text) = h.run("df");
        assert_eq!(status, 0);
        assert_eq!(
            text,
            "Filesystem     1K-blocks Used Available Use% Mounted on\n\
             /dev/root            400    8       392   2% /\n"
        );
        assert_eq!(h.run("df /tmp"), (1, "df: extra operand '/tmp'\n".into()));
    }

    #[test]
    fn free_shows_memory_in_kib() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("free"),
            (1, "free: memory figures are not available\n".into())
        );
        h.system.memory = Some(MemInfo {
            ram_total: 1 << 30,
            ram_free: 768 << 20,
            heap_total: 32 << 20,
            heap_used: 1 << 20,
        });
        assert_eq!(
            h.run("free").1,
            "              total        used        free\n\
             Mem:        1048576      262144      786432\n\
             Heap:         32768        1024       31744\n"
        );
    }

    #[test]
    fn dmesg_prints_the_kernel_log() {
        let mut h = Harness::new();
        h.system.log = b"[ ok ] memory\n[ ok ] usb\n".to_vec();
        assert_eq!(h.run("dmesg"), (0, "[ ok ] memory\n[ ok ] usb\n".into()));
    }

    #[test]
    fn sync_reports_errors() {
        let mut h = Harness::new();
        assert_eq!(h.run("sync"), (0, "".into()));
        h.spy.fail_sync.set(Some(Errno::EIO));
        assert_eq!(
            h.run("sync"),
            (
                1,
                "sync: Input/output error\nrelay-sh: sync failed: Input/output error\n".into()
            )
        );
    }

    #[test]
    fn reboot_and_poweroff_shut_down_then_stop_the_shell() {
        let mut h = Harness::new();
        h.console.type_in(b"reboot\recho never\r");
        Shell::new(&mut h.vfs, &mut h.console, &mut h.system).run();
        assert_eq!(h.spy.shutdowns.get(), 1);
        assert_eq!(h.system.reboots, 1);
        assert_eq!(h.console.take(), "root@relay:/# reboot\n");
        assert_eq!(h.run("poweroff"), (0, "".into()));
        assert_eq!(h.system.poweroffs, 1);
    }

    #[test]
    fn a_failed_shutdown_keeps_the_machine_up_unless_forced() {
        let mut h = Harness::new();
        h.spy.fail_shutdown.set(Some(Errno::EIO));
        assert_eq!(
            h.run("reboot"),
            (
                1,
                "reboot: cannot shut the filesystems down cleanly: Input/output error\n\
                 reboot: use 'reboot -f' to go ahead anyway\n"
                    .into()
            )
        );
        assert_eq!(h.system.reboots, 0);
        let (status, _) = h.run("poweroff -f");
        assert_eq!((status, h.system.poweroffs), (0, 1));
    }

    #[test]
    fn ps_lists_every_process_with_what_it_does() {
        use relay_abi::ProcInfo;
        use relay_abi::proc::*;
        let mut h = Harness::new();
        h.system.processes = Some(alloc::vec![
            ProcInfo::new(1, 0, 1, STATE_WAIT, 0, 12, b"init"),
            ProcInfo::new(2, 1, 2, STATE_WAIT, 309, 1_234, b"/bin/sh"),
            ProcInfo::new(5, 2, 5, STATE_SLEEP, 41, 3, b"/bin/sleep"),
            ProcInfo::new(6, 2, 6, STATE_READY, 35, 754_000, b"/bin/t-spin"),
            ProcInfo::new(8, 2, 8, STATE_PIPE, 38, 0, b"/bin/cat"),
            ProcInfo::new(9, 2, 9, STATE_READ, 37, 0, b"t-read"),
            ProcInfo::new(10, 2, 8, STATE_ZOMBIE, 0, 61_000, b"/bin/seq"),
            ProcInfo::new(11, 2, 11, STATE_RUN, 43, 0, b"/bin/ps"),
            ProcInfo::new(65_536, 1, 3, 99, 2_500_000, 6_000_000, &[b'x'; 64]),
        ]);
        assert_eq!(
            h.run("ps"),
            (
                0,
                "  PID  PPID STATE      MEM  TIME CMD\n\
                 \x20   1     0 wait         0  0:00 init\n\
                 \x20   2     1 wait      1236  0:01 /bin/sh\n\
                 \x20   5     2 sleep      164  0:00 /bin/sleep\n\
                 \x20   6     2 ready      140 12:34 /bin/t-spin\n\
                 \x20   8     2 pipe       152  0:00 /bin/cat\n\
                 \x20   9     2 read       148  0:00 t-read\n\
                 \x20  10     2 zombie       0  1:01 /bin/seq\n\
                 \x20  11     2 run        172  0:00 /bin/ps\n\
                 65536     1 ?      10000000 100:00 xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n"
                    .into()
            )
        );
    }

    #[test]
    fn ps_takes_no_operand_and_needs_processes() {
        let mut h = Harness::new();
        assert_eq!(h.run("ps"), (1, "ps: processes are not available\n".into()));
        h.system.processes = Some(alloc::vec::Vec::new());
        assert_eq!(h.run("ps x"), (1, "ps: extra operand 'x'\n".into()));
        assert_eq!(h.run("ps -e").0, 1);
        assert_eq!(
            h.run("ps"),
            (0, "  PID  PPID STATE      MEM  TIME CMD\n".into())
        );
    }
}
