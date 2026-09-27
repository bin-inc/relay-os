//! `touch`, `mkdir`, `rmdir`, `rm`, `cp` and `mv` (spec §7.3).

use crate::ctx::{Ctx, getopt, quote};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use vfs::{Errno, FileType, Node, path};

/// `cp` copies in pieces of this size (on the heap).
const CHUNK: usize = 64 * 1024;

/// Options, or the exit status after reporting a bad one.
fn options(
    ctx: &mut Ctx<'_>,
    name: &str,
    args: &[String],
    flags: &str,
) -> Result<crate::ctx::Opts, i32> {
    getopt(args, flags, "").map_err(|e| ctx.fail(name, format_args!("{e}")))
}

/// `touch file…`: creates missing files, updates the mtime of others.
pub fn touch(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match options(ctx, "touch", args, "") {
        Ok(o) => o,
        Err(status) => return status,
    };
    if opts.operands.is_empty() {
        return ctx.fail("touch", format_args!("missing file operand"));
    }
    let mut status = 0;
    for op in &opts.operands {
        let p = op.as_bytes();
        let result = match ctx.vfs.lookup(p) {
            Ok(node) => ctx.vfs.touch(node),
            Err(Errno::ENOENT) => ctx.vfs.create(p).map(|_| ()),
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            status = ctx.fail("touch", format_args!("cannot touch {}: {e}", quote(op)));
        }
    }
    status
}

/// `mkdir [-p] dir…`: `-p` also makes missing parents and accepts
/// directories that exist.
pub fn mkdir(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match options(ctx, "mkdir", args, "p") {
        Ok(o) => o,
        Err(status) => return status,
    };
    if opts.operands.is_empty() {
        return ctx.fail("mkdir", format_args!("missing operand"));
    }
    let parents = opts.has('p');
    let mut status = 0;
    for op in &opts.operands {
        let result = if parents {
            mkdir_p(ctx, op.as_bytes())
        } else {
            ctx.vfs.mkdir(op.as_bytes())
        };
        if let Err(e) = result {
            status = ctx.fail(
                "mkdir",
                format_args!("cannot create directory {}: {e}", quote(op)),
            );
        }
    }
    status
}

/// Makes every directory along `p`; ones that exist are fine.
fn mkdir_p(ctx: &mut Ctx<'_>, p: &[u8]) -> Result<(), Errno> {
    if p.is_empty() {
        return Err(Errno::ENOENT);
    }
    let ends: Vec<usize> = (1..=p.len())
        .filter(|&i| i == p.len() || (p[i] == b'/' && p[i - 1] != b'/'))
        .collect();
    for &end in &ends {
        match ctx.vfs.mkdir(&p[..end]) {
            Ok(()) => {}
            Err(Errno::EEXIST) if end < p.len() => {}
            Err(Errno::EEXIST) => {
                let node = ctx.vfs.lookup(p)?;
                if ctx.vfs.stat(node)?.kind != FileType::Directory {
                    return Err(Errno::EEXIST);
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// `rmdir dir…`: removes empty directories.
pub fn rmdir(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match options(ctx, "rmdir", args, "") {
        Ok(o) => o,
        Err(status) => return status,
    };
    if opts.operands.is_empty() {
        return ctx.fail("rmdir", format_args!("missing operand"));
    }
    let mut status = 0;
    for op in &opts.operands {
        if let Err(e) = ctx.vfs.rmdir(op.as_bytes()) {
            status = ctx.fail("rmdir", format_args!("failed to remove {}: {e}", quote(op)));
        }
    }
    status
}

/// `rm [-r] [-f] path…`: `-r` removes directories with everything in them,
/// `-f` ignores missing files. `/` is always refused.
pub fn rm(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match options(ctx, "rm", args, "rRf") {
        Ok(o) => o,
        Err(status) => return status,
    };
    let recursive = opts.has('r') || opts.has('R');
    let force = opts.has('f');
    if opts.operands.is_empty() && !force {
        return ctx.fail("rm", format_args!("missing operand"));
    }
    let root = ctx.vfs.lookup(b"/").ok();
    let mut status = 0;
    for op in &opts.operands {
        let p = op.as_bytes();
        if matches!(path::basename(p), b"." | b"..") {
            status = ctx.fail(
                "rm",
                format_args!(
                    "refusing to remove '.' or '..' directory: skipping {}",
                    quote(op)
                ),
            );
            continue;
        }
        let stat = ctx.vfs.lookup(p).and_then(|n| Ok((n, ctx.vfs.stat(n)?)));
        let (node, st) = match stat {
            Ok(x) => x,
            // GNU `rm -f` is quiet about anything that is not there.
            Err(Errno::ENOENT | Errno::ENOTDIR) if force => continue,
            Err(e) => {
                status = ctx.fail("rm", format_args!("cannot remove {}: {e}", quote(op)));
                continue;
            }
        };
        if st.kind != FileType::Directory {
            if let Err(e) = ctx.vfs.unlink(p) {
                status = ctx.fail("rm", format_args!("cannot remove {}: {e}", quote(op)));
            }
        } else if !recursive {
            status = ctx.fail(
                "rm",
                format_args!("cannot remove {}: Is a directory", quote(op)),
            );
        } else if Some(node) == root {
            status = ctx.fail(
                "rm",
                format_args!("it is dangerous to operate recursively on '/'"),
            );
        } else if !remove_tree(ctx, p) {
            status = 1;
        }
    }
    status
}

/// Removes a directory and everything below it, reporting each failure.
/// Depth-first with an explicit stack: a deep tree must not use up the
/// kernel's small stack. Returns whether everything went.
fn remove_tree(ctx: &mut Ctx<'_>, top: &[u8]) -> bool {
    let mut ok = true;
    let above = ctx.vfs.lookup(&path::join(top, b".."));
    let mut stack = match above.and_then(|parent| children(ctx, top, parent)) {
        Ok((node, names)) => vec![(top.to_vec(), node, names)],
        Err(e) => {
            ctx.fail(
                "rm",
                format_args!("cannot remove {}: {e}", quote(&path::display(top))),
            );
            return false;
        }
    };
    while let Some((dir, node, pending)) = stack.last_mut() {
        let Some(name) = pending.pop() else {
            let (dir, _, _) = stack.pop().expect("not empty");
            if let Err(e) = ctx.vfs.rmdir(&dir) {
                ctx.fail(
                    "rm",
                    format_args!("cannot remove {}: {e}", quote(&path::display(&dir))),
                );
                ok = false;
            }
            continue;
        };
        let parent = *node;
        let child = path::join(dir, &name);
        let is_dir = ctx
            .vfs
            .lookup(&child)
            .and_then(|n| ctx.vfs.stat(n))
            .map(|st| st.kind == FileType::Directory);
        let result = match is_dir {
            Ok(true) => children(ctx, &child, parent)
                .map(|(node, names)| stack.push((child.clone(), node, names))),
            Ok(false) => ctx.vfs.unlink(&child),
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            ctx.fail(
                "rm",
                format_args!("cannot remove {}: {e}", quote(&path::display(&child))),
            );
            ok = false;
        }
    }
    ok
}

/// Directory `dir`, reached from `parent`, and its names without `.` and
/// `..`. The directory must be where its path says: its `.` is itself and
/// its `..` is `parent` (unless a mount lies between). A corrupt disk can
/// hold an entry naming a directory that lives elsewhere; following it
/// would take the walk out of its target, so that is `EIO`. A name that
/// could not be created (empty, or with a `/`) is left out rather than
/// joined into a path that leads somewhere else; the directory then
/// stays, not empty.
fn children(ctx: &mut Ctx<'_>, dir: &[u8], parent: Node) -> Result<(Node, Vec<Vec<u8>>), Errno> {
    let node = ctx.vfs.lookup(dir)?;
    let entries = ctx.vfs.read_dir(node)?;
    let ino_of = |name: &[u8]| entries.iter().find(|e| e.name == name).map(|e| e.ino);
    if ino_of(b".") != Some(node.ino)
        || (parent.mount == node.mount && ino_of(b"..") != Some(parent.ino))
    {
        return Err(Errno::EIO);
    }
    let mut names: Vec<Vec<u8>> = entries
        .into_iter()
        .map(|e| e.name)
        .filter(|n| path::check_name(n).is_ok())
        .collect();
    names.sort();
    Ok((node, names))
}

/// The sources and where each goes, for `cp` and `mv`: `src dst`, or
/// `src… dir` (each source into `dir` under its own name).
fn targets(
    ctx: &mut Ctx<'_>,
    name: &str,
    operands: &[String],
) -> Result<Vec<(String, Vec<u8>)>, i32> {
    let (dest, sources) = match operands {
        [] => return Err(ctx.fail(name, format_args!("missing file operand"))),
        [only] => {
            return Err(ctx.fail(
                name,
                format_args!("missing destination file operand after {}", quote(only)),
            ));
        }
        [sources @ .., dest] => (dest, sources),
    };
    let into_dir = ctx
        .vfs
        .lookup(dest.as_bytes())
        .and_then(|n| ctx.vfs.stat(n))
        .is_ok_and(|st| st.kind == FileType::Directory);
    if !into_dir && sources.len() > 1 {
        return Err(ctx.fail(
            name,
            format_args!("target {} is not a directory", quote(dest)),
        ));
    }
    Ok(sources
        .iter()
        .map(|src| {
            let to = if into_dir {
                path::join(dest.as_bytes(), path::basename(src.as_bytes()))
            } else {
                dest.as_bytes().to_vec()
            };
            (src.clone(), to)
        })
        .collect())
}

/// `cp src dst`, `cp src… dir`: regular files only.
pub fn cp(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match options(ctx, "cp", args, "") {
        Ok(o) => o,
        Err(status) => return status,
    };
    let targets = match targets(ctx, "cp", &opts.operands) {
        Ok(t) => t,
        Err(status) => return status,
    };
    let mut status = 0;
    for (src, dst) in targets {
        if copy(ctx, &src, &dst).is_err() {
            status = 1;
        }
    }
    status
}

/// Copies one file, reporting what went wrong.
fn copy(ctx: &mut Ctx<'_>, src: &str, dst: &[u8]) -> Result<(), ()> {
    let shown = path::display(dst);
    let from = match ctx
        .vfs
        .lookup(src.as_bytes())
        .and_then(|n| Ok((n, ctx.vfs.stat(n)?)))
    {
        Ok((_, st)) if st.kind == FileType::Directory => {
            ctx.fail(
                "cp",
                format_args!("-r not specified; omitting directory {}", quote(src)),
            );
            return Err(());
        }
        Ok((node, _)) => node,
        Err(e) => {
            ctx.fail("cp", format_args!("cannot stat {}: {e}", quote(src)));
            return Err(());
        }
    };
    let to = match open_target(ctx, from, dst) {
        Ok(node) => node,
        Err(Errno::EEXIST) => {
            ctx.fail(
                "cp",
                format_args!("{} and {} are the same file", quote(src), quote(&shown)),
            );
            return Err(());
        }
        Err(Errno::EISDIR) => {
            ctx.fail(
                "cp",
                format_args!(
                    "cannot overwrite directory {} with non-directory",
                    quote(&shown)
                ),
            );
            return Err(());
        }
        Err(e) => {
            ctx.fail(
                "cp",
                format_args!("cannot create regular file {}: {e}", quote(&shown)),
            );
            return Err(());
        }
    };
    let mut buf = vec![0; CHUNK];
    let mut offset = 0;
    loop {
        let n = match ctx.vfs.read_at(from, offset, &mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) => {
                ctx.fail("cp", format_args!("error reading {}: {e}", quote(src)));
                return Err(());
            }
        };
        let mut done = 0;
        while done < n {
            // Nothing written would loop forever; the contract says ENOSPC.
            let written = match ctx.vfs.write_at(to, offset + done as u64, &buf[done..n]) {
                Ok(0) => Err(Errno::ENOSPC),
                other => other,
            };
            match written {
                Ok(written) => done += written,
                Err(e) => {
                    ctx.fail("cp", format_args!("error writing {}: {e}", quote(&shown)));
                    return Err(());
                }
            }
        }
        offset += n as u64;
    }
}

/// Opens `dst` for a copy of `from`: created if missing, emptied if it is
/// a file. `EEXIST` if it is `from` itself, `EISDIR` if it is a directory.
fn open_target(ctx: &mut Ctx<'_>, from: Node, dst: &[u8]) -> Result<Node, Errno> {
    match ctx.vfs.lookup(dst) {
        Ok(node) if node == from => Err(Errno::EEXIST),
        Ok(node) => {
            if ctx.vfs.stat(node)?.kind == FileType::Directory {
                return Err(Errno::EISDIR);
            }
            ctx.vfs.truncate(node, 0)?;
            Ok(node)
        }
        Err(Errno::ENOENT) => ctx.vfs.create(dst),
        Err(e) => Err(e),
    }
}

/// `mv src dst`, `mv src… dir`: renames within the one filesystem.
pub fn mv(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match options(ctx, "mv", args, "") {
        Ok(o) => o,
        Err(status) => return status,
    };
    let targets = match targets(ctx, "mv", &opts.operands) {
        Ok(t) => t,
        Err(status) => return status,
    };
    let mut status = 0;
    for (src, dst) in targets {
        let shown = path::display(&dst);
        let (s, d) = (quote(&src), quote(&shown));
        let from = match ctx.vfs.lookup(src.as_bytes()) {
            Ok(node) => node,
            Err(e) => {
                status = ctx.fail("mv", format_args!("cannot stat {s}: {e}"));
                continue;
            }
        };
        if ctx.vfs.lookup(&dst) == Ok(from) {
            status = ctx.fail("mv", format_args!("{s} and {d} are the same file"));
            continue;
        }
        let result = ctx.vfs.rename(src.as_bytes(), &dst);
        status = match result {
            Ok(()) => continue,
            Err(Errno::EINVAL) => ctx.fail(
                "mv",
                format_args!("cannot move {s} to a subdirectory of itself, {d}"),
            ),
            Err(Errno::EISDIR) => ctx.fail(
                "mv",
                format_args!("cannot overwrite directory {d} with non-directory"),
            ),
            Err(Errno::ENOTDIR) if ctx.vfs.lookup(&dst).is_ok() => ctx.fail(
                "mv",
                format_args!("cannot overwrite non-directory {d} with directory {s}"),
            ),
            Err(e) => ctx.fail("mv", format_args!("cannot move {s} to {d}: {e}")),
        };
    }
    status
}

#[cfg(test)]
mod tests {
    use crate::testing::{Harness, memfs};
    use alloc::string::String;

    #[test]
    fn touch_creates_files_and_updates_times() {
        let mut h = Harness::new();
        assert_eq!(h.run("touch /tmp/a /tmp/b"), (0, "".into()));
        assert_eq!(h.get("/tmp/a"), b"");
        h.put("/tmp/c", b"keep");
        h.run("touch /tmp/c");
        assert_eq!(h.get("/tmp/c"), b"keep");
        assert_eq!(
            h.run("touch /nope/x"),
            (
                1,
                "touch: cannot touch '/nope/x': No such file or directory\n".into()
            )
        );
        assert_eq!(h.run("touch"), (1, "touch: missing file operand\n".into()));
    }

    #[test]
    fn mkdir_makes_directories() {
        let mut h = Harness::new();
        assert_eq!(h.run("mkdir /tmp/a /tmp/b"), (0, "".into()));
        assert!(h.exists("/tmp/a") && h.exists("/tmp/b"));
        assert_eq!(
            h.run("mkdir /tmp/a"),
            (
                1,
                "mkdir: cannot create directory '/tmp/a': File exists\n".into()
            )
        );
        assert_eq!(
            h.run("mkdir /tmp/x/y"),
            (
                1,
                "mkdir: cannot create directory '/tmp/x/y': No such file or directory\n".into()
            )
        );
        assert_eq!(h.run("mkdir"), (1, "mkdir: missing operand\n".into()));
    }

    #[test]
    fn mkdir_p_makes_parents_and_accepts_existing_directories() {
        let mut h = Harness::new();
        assert_eq!(h.run("mkdir -p /tmp/x/y//z/"), (0, "".into()));
        assert!(h.exists("/tmp/x/y/z"));
        assert_eq!(h.run("mkdir -p /tmp/x/y"), (0, "".into()));
        h.run("cd /tmp");
        assert_eq!(h.run("mkdir -p rel/./sub/../other"), (0, "".into()));
        assert!(h.exists("/tmp/rel/sub") && h.exists("/tmp/rel/other"));
        assert_eq!(
            h.run("mkdir -p ''"),
            (
                1,
                "mkdir: cannot create directory '': No such file or directory\n".into()
            )
        );
        h.put("/tmp/file", b"");
        assert_eq!(
            h.run("mkdir -p /tmp/file"),
            (
                1,
                "mkdir: cannot create directory '/tmp/file': File exists\n".into()
            )
        );
        assert_eq!(
            h.run("mkdir -p /tmp/file/sub"),
            (
                1,
                "mkdir: cannot create directory '/tmp/file/sub': Not a directory\n".into()
            )
        );
    }

    #[test]
    fn rmdir_removes_only_empty_directories() {
        let mut h = Harness::new();
        h.dir("/tmp/d");
        h.put("/tmp/d/f", b"");
        assert_eq!(
            h.run("rmdir /tmp/d"),
            (
                1,
                "rmdir: failed to remove '/tmp/d': Directory not empty\n".into()
            )
        );
        assert_eq!(
            h.run("rmdir /tmp/d/f"),
            (
                1,
                "rmdir: failed to remove '/tmp/d/f': Not a directory\n".into()
            )
        );
        h.run("rm /tmp/d/f");
        assert_eq!(h.run("rmdir /tmp/d"), (0, "".into()));
        assert!(!h.exists("/tmp/d"));
    }

    #[test]
    fn rm_removes_files_and_with_r_whole_trees() {
        let mut h = Harness::new();
        h.put("/tmp/f", b"");
        assert_eq!(h.run("rm /tmp/f"), (0, "".into()));
        assert!(!h.exists("/tmp/f"));
        h.run("mkdir -p /tmp/t/a/b");
        h.put("/tmp/t/a/b/deep", b"x");
        h.put("/tmp/t/top", b"y");
        assert_eq!(
            h.run("rm /tmp/t"),
            (1, "rm: cannot remove '/tmp/t': Is a directory\n".into())
        );
        assert_eq!(h.run("rm -r /tmp/t"), (0, "".into()));
        assert!(!h.exists("/tmp/t"));
    }

    #[test]
    fn rm_f_ignores_missing_files() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("rm /tmp/nope"),
            (
                1,
                "rm: cannot remove '/tmp/nope': No such file or directory\n".into()
            )
        );
        assert_eq!(h.run("rm -f /tmp/nope"), (0, "".into()));
        assert_eq!(h.run("rm -f"), (0, "".into()));
        assert_eq!(h.run("rm"), (1, "rm: missing operand\n".into()));
        h.dir("/tmp/d");
        assert_eq!(h.run("rm -rf /tmp/d /tmp/gone"), (0, "".into()));
        assert_eq!(h.run("rm -f /etc/motd/x"), (0, "".into()));
    }

    #[test]
    fn rm_refuses_the_root_and_dot_directories() {
        let mut h = Harness::new();
        assert_eq!(
            h.run("rm -rf /"),
            (
                1,
                "rm: it is dangerous to operate recursively on '/'\n".into()
            )
        );
        assert_eq!(
            h.run("rm -r //"),
            (
                1,
                "rm: it is dangerous to operate recursively on '/'\n".into()
            )
        );
        h.run("cd /tmp");
        assert_eq!(
            h.run("rm -r ../.."),
            (
                1,
                "rm: refusing to remove '.' or '..' directory: skipping '../..'\n".into()
            )
        );
        assert_eq!(
            h.run("rm -r ."),
            (
                1,
                "rm: refusing to remove '.' or '..' directory: skipping '.'\n".into()
            )
        );
        assert_eq!(
            h.run("rm /"),
            (1, "rm: cannot remove '/': Is a directory\n".into())
        );
        assert!(h.exists("/etc/motd"));
    }

    /// A corrupt disk can hold a directory entry naming a directory that
    /// lives elsewhere; `rm -r` must not follow it out of its target.
    #[test]
    fn rm_r_does_not_follow_an_alias_out_of_its_target() {
        let mut fs = memfs();
        let root = vfs::FileSystem::root(&fs);
        let keep = vfs::FileSystem::mkdir(&mut fs, root, b"keep").unwrap();
        vfs::FileSystem::create(&mut fs, keep, b"important").unwrap();
        let junk = vfs::FileSystem::mkdir(&mut fs, root, b"junk").unwrap();
        fs.link(junk, b"evil", keep).unwrap();
        let mut h = Harness::on(fs);
        assert_eq!(
            h.run("rm -r /junk"),
            (
                1,
                "rm: cannot remove '/junk/evil': Input/output error\n\
                 rm: cannot remove '/junk': Directory not empty\n"
                    .into()
            )
        );
        assert_eq!(
            h.run("rm -r /junk/evil"),
            (
                1,
                "rm: cannot remove '/junk/evil': Input/output error\n".into()
            )
        );
        assert!(h.exists("/keep/important"));
    }

    #[test]
    fn rm_r_handles_a_very_deep_tree() {
        let mut h = Harness::new();
        h.run("cd /tmp");
        let mut path = String::from("/tmp");
        for _ in 0..500 {
            path.push_str("/d");
            h.dir(&path);
        }
        assert_eq!(h.run("rm -r /tmp/d"), (0, "".into()));
        assert!(!h.exists("/tmp/d"));
    }

    #[test]
    fn cp_copies_files() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"alpha");
        assert_eq!(h.run("cp /tmp/a /tmp/b"), (0, "".into()));
        assert_eq!(h.get("/tmp/b"), b"alpha");
        h.put("/tmp/long", &[7u8; 150_000]);
        h.put("/tmp/b", b"a longer old content");
        h.run("cp /tmp/a /tmp/b");
        assert_eq!(h.get("/tmp/b"), b"alpha", "the target is emptied first");
        h.run("cp /tmp/long /tmp/copy");
        assert_eq!(h.get("/tmp/copy"), [7u8; 150_000]);
    }

    #[test]
    fn cp_into_a_directory_keeps_the_names() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"1");
        h.put("/tmp/b", b"2");
        h.dir("/root/in");
        assert_eq!(h.run("cp /tmp/a /tmp/b /root/in"), (0, "".into()));
        assert_eq!(h.get("/root/in/a"), b"1");
        assert_eq!(h.get("/root/in/b"), b"2");
        h.run("cd /root");
        assert_eq!(h.run("cp /etc/hostname in/"), (0, "".into()));
        assert_eq!(h.get("/root/in/hostname"), b"relay\n");
    }

    #[test]
    fn cp_errors() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"1");
        assert_eq!(h.run("cp"), (1, "cp: missing file operand\n".into()));
        assert_eq!(
            h.run("cp /tmp/a"),
            (
                1,
                "cp: missing destination file operand after '/tmp/a'\n".into()
            )
        );
        assert_eq!(
            h.run("cp /tmp/nope /tmp/b"),
            (
                1,
                "cp: cannot stat '/tmp/nope': No such file or directory\n".into()
            )
        );
        assert_eq!(
            h.run("cp /etc /tmp/b"),
            (
                1,
                "cp: -r not specified; omitting directory '/etc'\n".into()
            )
        );
        assert_eq!(
            h.run("cp /tmp/a /tmp/a"),
            (1, "cp: '/tmp/a' and '/tmp/a' are the same file\n".into())
        );
        assert_eq!(
            h.run("cp /tmp/a /etc/motd /tmp/a"),
            (1, "cp: target '/tmp/a' is not a directory\n".into())
        );
        h.run("mkdir -p /tmp/x/a");
        assert_eq!(
            h.run("cp /tmp/a /tmp/x"),
            (
                1,
                "cp: cannot overwrite directory '/tmp/x/a' with non-directory\n".into()
            )
        );
        assert_eq!(
            h.run("cp /tmp/a /nope/b"),
            (
                1,
                "cp: cannot create regular file '/nope/b': No such file or directory\n".into()
            )
        );
    }

    #[test]
    fn cp_reports_a_full_disk() {
        let mut h = Harness::with_capacity(4 * 4096);
        h.put("/tmp/big", &[1u8; 8192]);
        assert_eq!(
            h.run("cp /tmp/big /tmp/copy"),
            (
                1,
                "cp: error writing '/tmp/copy': No space left on device\n".into()
            )
        );
    }

    #[test]
    fn cp_on_a_filesystem_that_writes_nothing_is_an_error_not_a_hang() {
        let mut h = Harness::new();
        h.spy.zero_writes.set(true);
        assert_eq!(
            h.run("cp /etc/motd /tmp/x"),
            (
                1,
                "cp: error writing '/tmp/x': No space left on device\n".into()
            )
        );
    }

    #[test]
    fn mv_renames_and_moves_into_directories() {
        let mut h = Harness::new();
        h.put("/tmp/a", b"1");
        assert_eq!(h.run("mv /tmp/a /tmp/b"), (0, "".into()));
        assert!(!h.exists("/tmp/a"));
        assert_eq!(h.get("/tmp/b"), b"1");
        h.dir("/root/in");
        h.put("/tmp/c", b"3");
        assert_eq!(h.run("mv /tmp/b /tmp/c /root/in"), (0, "".into()));
        assert_eq!(h.get("/root/in/c"), b"3");
        h.run("mkdir -p /tmp/tree/sub");
        assert_eq!(h.run("mv /tmp/tree /root/moved"), (0, "".into()));
        assert!(h.exists("/root/moved/sub"));
        h.put("/tmp/over", b"new");
        h.put("/tmp/old", b"old");
        assert_eq!(h.run("mv /tmp/over /tmp/old"), (0, "".into()));
        assert_eq!(h.get("/tmp/old"), b"new");
    }

    #[test]
    fn mv_errors() {
        let mut h = Harness::new();
        h.put("/tmp/f", b"");
        h.run("mkdir -p /tmp/d/sub /tmp/e/d/full /tmp/h/f");
        let cases = [
            (
                "mv /tmp/nope /tmp/x",
                "mv: cannot stat '/tmp/nope': No such file or directory\n",
            ),
            (
                "mv /tmp/f /tmp/f",
                "mv: '/tmp/f' and '/tmp/f' are the same file\n",
            ),
            (
                "mv /tmp/d /tmp/d/sub",
                "mv: cannot move '/tmp/d' to a subdirectory of itself, '/tmp/d/sub/d'\n",
            ),
            (
                "mv /tmp/d /tmp/e",
                "mv: cannot move '/tmp/d' to '/tmp/e/d': Directory not empty\n",
            ),
            (
                "mv /tmp/e /tmp/f",
                "mv: cannot overwrite non-directory '/tmp/f' with directory '/tmp/e'\n",
            ),
            (
                "mv /tmp/f /tmp/h",
                "mv: cannot overwrite directory '/tmp/h/f' with non-directory\n",
            ),
            ("mv x", "mv: missing destination file operand after 'x'\n"),
        ];
        for (line, message) in cases {
            assert_eq!(h.run(line), (1, message.into()), "{line}");
        }
    }
}
