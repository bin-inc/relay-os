//! `date`, `df`, `free`, `dmesg`, `sync`, `reboot` and `poweroff`
//! (spec §7.3, §7.4).

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
/// silently; `-f` goes ahead anyway.
fn restart(ctx: &mut Ctx<'_>, name: &str, args: &[String]) -> i32 {
    let opts = match no_operands(ctx, name, args, "f") {
        Ok(o) => o,
        Err(status) => return status,
    };
    if let Err(e) = ctx.vfs.shutdown() {
        ctx.fail(
            name,
            format_args!("cannot shut the filesystems down cleanly: {e}"),
        );
        if !opts.has('f') {
            return ctx.fail(name, format_args!("use '{name} -f' to go ahead anyway"));
        }
    }
    if name == "reboot" {
        ctx.system.reboot();
    } else {
        ctx.system.poweroff();
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
}
