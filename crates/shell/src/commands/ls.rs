//! `ls [-l] [-a] [path…]` (spec §7.3), plus the mode and owner formatting
//! `stat` shares.

use crate::ctx::{Ctx, getopt, outln, quote};
use crate::time;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use vfs::{FileType, Node, Stat, path};

/// `ls` exits with 2 when an operand cannot be listed at all, 1 for a
/// problem with something inside a directory (as GNU `ls`).
const SERIOUS: i32 = 2;

struct Entry {
    name: Vec<u8>,
    stat: Stat,
    /// A symbolic link's target, for `-l`.
    target: Option<Vec<u8>>,
}

pub fn ls(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "la", "") {
        Ok(o) => o,
        Err(e) => {
            ctx.fail("ls", format_args!("{e}"));
            return SERIOUS;
        }
    };
    let (long, all) = (opts.has('l'), opts.has('a'));
    let mut operands = opts.operands;
    if operands.is_empty() {
        operands.push(String::from("."));
    }
    let headers = operands.len() > 1;
    let mut status = 0;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for op in &operands {
        match entry(ctx, op.as_bytes(), op.as_bytes()) {
            Ok((node, e)) if e.stat.kind == FileType::Directory => dirs.push((op.clone(), node)),
            Ok((_, e)) => files.push(e),
            Err(e) => {
                ctx.fail("ls", format_args!("cannot access {}: {e}", quote(op)));
                status = SERIOUS;
            }
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    dirs.sort_by(|a: &(String, Node), b| a.0.cmp(&b.0));
    let mut first = files.is_empty();
    if !files.is_empty() {
        print(ctx, &files, long, false);
    }
    for (dir, node) in dirs {
        if !first {
            outln!(ctx);
        }
        first = false;
        if headers {
            outln!(ctx, "{}:", path::display(dir.as_bytes()));
        }
        match list(ctx, &dir, node, all) {
            Ok((entries, problems)) => {
                print(ctx, &entries, long, true);
                status = status.max(problems);
            }
            Err(e) => {
                ctx.fail(
                    "ls",
                    format_args!("cannot open directory {}: {e}", quote(&dir)),
                );
                status = SERIOUS;
            }
        }
    }
    status
}

/// Looks up `path` (shown as `name`) with everything `-l` needs.
fn entry(ctx: &mut Ctx<'_>, path: &[u8], name: &[u8]) -> Result<(Node, Entry), vfs::Errno> {
    let node = ctx.vfs.lookup(path)?;
    let stat = ctx.vfs.stat(node)?;
    let target = match stat.kind {
        FileType::Symlink => ctx.vfs.read_link(node).ok(),
        _ => None,
    };
    let entry = Entry {
        name: name.to_vec(),
        stat,
        target,
    };
    Ok((node, entry))
}

/// A directory's entries, sorted by name, hidden ones only with `-a`; and
/// the exit status its problems deserve.
fn list(
    ctx: &mut Ctx<'_>,
    dir: &str,
    node: Node,
    all: bool,
) -> Result<(Vec<Entry>, i32), vfs::Errno> {
    let mut names: Vec<Vec<u8>> = ctx
        .vfs
        .read_dir(node)?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| all || !n.starts_with(b"."))
        .collect();
    names.sort();
    let mut entries = Vec::with_capacity(names.len());
    let mut problems = 0;
    for name in names {
        let full = path::join(dir.as_bytes(), &name);
        match entry(ctx, &full, &name) {
            Ok((_, e)) => entries.push(e),
            Err(e) => {
                let shown = path::display(&full);
                ctx.fail("ls", format_args!("cannot access {}: {e}", quote(&shown)));
                problems = 1;
            }
        }
    }
    Ok((entries, problems))
}

fn print(ctx: &mut Ctx<'_>, entries: &[Entry], long: bool, total: bool) {
    if long {
        if total {
            let blocks: u64 = entries.iter().map(|e| e.stat.blocks.div_ceil(2)).sum();
            outln!(ctx, "total {blocks}");
        }
        print_long(ctx, entries);
        return;
    }
    let names: Vec<String> = entries.iter().map(|e| path::display(&e.name)).collect();
    if ctx.is_tty() {
        let width = ctx.columns();
        for line in columns(&names, width) {
            outln!(ctx, "{line}");
        }
    } else {
        for name in names {
            outln!(ctx, "{name}");
        }
    }
}

fn print_long(ctx: &mut Ctx<'_>, entries: &[Entry]) {
    let now = ctx.system.now();
    let width =
        |f: &dyn Fn(&Entry) -> String| entries.iter().map(|e| f(e).len()).max().unwrap_or(0);
    let links = width(&|e| format!("{}", e.stat.nlink));
    let users = width(&|e| owner(e.stat.uid));
    let groups = width(&|e| owner(e.stat.gid));
    let sizes = width(&|e| format!("{}", e.stat.size));
    for e in entries {
        let mut line = format!(
            "{} {:>links$} {:<users$} {:<groups$} {:>sizes$} {} {}",
            mode_string(&e.stat),
            e.stat.nlink,
            owner(e.stat.uid),
            owner(e.stat.gid),
            e.stat.size,
            time::short(e.stat.mtime, now),
            path::display(&e.name),
        );
        if let Some(target) = &e.target {
            line.push_str(" -> ");
            line.push_str(&path::display(target));
        }
        outln!(ctx, "{line}");
    }
}

/// Names in columns across a screen `width` wide, filled top to bottom,
/// as many columns as fit (GNU `ls` on a terminal).
fn columns(names: &[String], width: usize) -> Vec<String> {
    let lens: Vec<usize> = names.iter().map(|n| n.chars().count()).collect();
    let n = names.len();
    if n == 0 {
        return Vec::new();
    }
    let mut rows = n;
    let mut widths = alloc::vec![lens.iter().copied().max().unwrap_or(0)];
    for cols in (2..=n.min(width / 3 + 1)).rev() {
        let r = n.div_ceil(cols);
        let cols = n.div_ceil(r);
        let w: Vec<usize> = (0..cols)
            .map(|c| {
                lens[c * r..((c + 1) * r).min(n)]
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        if w.iter().sum::<usize>() + 2 * (cols - 1) <= width {
            rows = r;
            widths = w;
            break;
        }
    }
    (0..rows)
        .map(|r| {
            let mut line = String::new();
            for c in 0..widths.len() {
                let i = c * rows + r;
                let Some(name) = names.get(i) else {
                    break;
                };
                if c > 0 {
                    let pad = widths[c - 1] + 2 - lens[i - rows];
                    line.extend(core::iter::repeat_n(' ', pad));
                }
                line.push_str(name);
            }
            line
        })
        .collect()
}

/// `drwxr-xr-x`, `-rw-r--r--`, `drwxrwxrwt`, …
pub fn mode_string(st: &Stat) -> String {
    let kind = match st.kind {
        FileType::Regular => '-',
        FileType::Directory => 'd',
        FileType::Symlink => 'l',
        FileType::CharDev => 'c',
        FileType::BlockDev => 'b',
        FileType::Fifo => 'p',
        FileType::Socket => 's',
    };
    let p = st.perm;
    let bit = |mask: u16, c: char| if p & mask != 0 { c } else { '-' };
    // The execute letter, replaced by s/S (set-ID) or t/T (sticky).
    let exec = |x: u16, special: u16, set: char| match (p & x != 0, p & special != 0) {
        (true, true) => set,
        (false, true) => set.to_ascii_uppercase(),
        (true, false) => 'x',
        (false, false) => '-',
    };
    [
        kind,
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        exec(0o100, 0o4000, 's'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        exec(0o010, 0o2000, 's'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        exec(0o001, 0o1000, 't'),
    ]
    .iter()
    .collect()
}

/// User and group names: 0 is `root` (the only user, spec §1.3); others
/// show as numbers.
pub fn owner(id: u32) -> String {
    match id {
        0 => String::from("root"),
        n => format!("{n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Harness, memfs};

    fn stat(kind: FileType, perm: u16) -> Stat {
        Stat {
            ino: 1,
            kind,
            perm,
            nlink: 1,
            uid: 0,
            gid: 0,
            size: 0,
            blocks: 0,
            block_size: 4096,
            atime: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    #[test]
    fn mode_strings() {
        assert_eq!(mode_string(&stat(FileType::Directory, 0o755)), "drwxr-xr-x");
        assert_eq!(mode_string(&stat(FileType::Regular, 0o644)), "-rw-r--r--");
        assert_eq!(
            mode_string(&stat(FileType::Directory, 0o1777)),
            "drwxrwxrwt"
        );
        assert_eq!(
            mode_string(&stat(FileType::Directory, 0o1776)),
            "drwxrwxrwT"
        );
        assert_eq!(mode_string(&stat(FileType::Regular, 0o4755)), "-rwsr-xr-x");
        assert_eq!(mode_string(&stat(FileType::Regular, 0o2644)), "-rw-r-Sr--");
        assert_eq!(mode_string(&stat(FileType::Symlink, 0o777)), "lrwxrwxrwx");
        assert_eq!(mode_string(&stat(FileType::CharDev, 0o600)), "crw-------");
        assert_eq!(mode_string(&stat(FileType::Fifo, 0o644)), "prw-r--r--");
        assert_eq!(owner(0), "root");
        assert_eq!(owner(1000), "1000");
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn columns_fill_downwards_and_fit_the_width() {
        let n = names(&["a", "bb", "ccc", "dddd", "e"]);
        assert_eq!(columns(&n, 80), ["a  bb  ccc  dddd  e"]);
        // Filled top to bottom, with as many columns as fit.
        assert_eq!(columns(&n, 12), ["a   ccc   e", "bb  dddd"]);
        assert_eq!(columns(&n, 10), ["a    dddd", "bb   e", "ccc"]);
        assert_eq!(columns(&n, 3), ["a", "bb", "ccc", "dddd", "e"]);
        assert!(columns(&[], 80).is_empty());
    }

    #[test]
    fn ls_lists_sorted_names_and_hides_dot_files() {
        let mut h = Harness::new();
        h.put("/tmp/b", b"");
        h.put("/tmp/a", b"");
        h.put("/tmp/.hidden", b"");
        h.put("/tmp/C", b"");
        assert_eq!(h.run("ls /tmp"), (0, "C  a  b\n".into()));
        assert_eq!(h.run("ls -a /tmp"), (0, ".  ..  .hidden  C  a  b\n".into()));
        h.run("cd /tmp");
        assert_eq!(h.run("ls"), (0, "C  a  b\n".into()));
    }

    #[test]
    fn ls_into_a_file_writes_one_name_per_line() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"");
        h.put("/tmp/b", b"");
        assert_eq!(h.run("ls /tmp > /root/list"), (0, "".into()));
        assert_eq!(h.get("/root/list"), b"a\nb\n");
    }

    #[test]
    fn ls_long_shows_mode_links_owner_size_and_time() {
        let mut h = Harness::new();
        h.put("/tmp/notes", b"hello\n");
        h.dir("/tmp/sub");
        let (status, text) = h.run("ls -l /tmp");
        assert_eq!(status, 0);
        assert_eq!(
            text,
            "total 8\n\
             -rw-r--r-- 1 root root    6 Sep 26 12:00 notes\n\
             drwxr-xr-x 2 root root 4096 Sep 26 12:00 sub\n"
        );
        assert_eq!(
            h.run("ls -l /tmp/notes").1,
            "-rw-r--r-- 1 root root 6 Sep 26 12:00 /tmp/notes\n"
        );
    }

    #[test]
    fn ls_long_shows_symlink_targets() {
        let mut fs = memfs();
        let root = vfs::FileSystem::root(&fs);
        fs.symlink(root, b"link", b"/etc/motd").unwrap();
        let mut h = Harness::on(fs);
        assert_eq!(
            h.run("ls -l /link").1,
            "lrwxrwxrwx 1 root root 9 Sep 26 12:00 /link -> /etc/motd\n"
        );
    }

    #[test]
    fn several_operands_get_headers_files_first() {
        let mut h = Harness::new();
        h.put("/tmp/f", b"");
        h.dir("/tmp/d");
        h.put("/tmp/d/inner", b"");
        assert_eq!(
            h.run("ls /tmp/d /etc/hostname /tmp"),
            (0, "/etc/hostname\n\n/tmp:\nd  f\n\n/tmp/d:\ninner\n".into())
        );
    }

    #[test]
    fn missing_operands_are_reported_and_the_rest_listed() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("ls /nope /etc"),
            (
                2,
                "ls: cannot access '/nope': No such file or directory\n/etc:\nhostname  motd\n"
                    .into()
            )
        );
        assert_eq!(h.run("ls -z"), (2, "ls: invalid option -- 'z'\n".into()));
    }
}
