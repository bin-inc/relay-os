//! `stat path…` (spec §7.3), in GNU `stat`'s layout.

use super::ls::{mode_string, owner};
use crate::ctx::{Ctx, getopt, outln, quote};
use crate::time;
use alloc::string::String;
use vfs::{FileType, path};

pub fn stat(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let opts = match getopt(args, "", "") {
        Ok(o) => o,
        Err(e) => return ctx.fail("stat", format_args!("{e}")),
    };
    if opts.operands.is_empty() {
        return ctx.fail("stat", format_args!("missing operand"));
    }
    let mut status = 0;
    for op in &opts.operands {
        let found = ctx
            .vfs
            .lookup(op.as_bytes())
            .and_then(|n| Ok((n, ctx.vfs.stat(n)?)));
        let (node, st) = match found {
            Ok(x) => x,
            Err(e) => {
                status = ctx.fail("stat", format_args!("cannot stat {}: {e}", quote(op)));
                continue;
            }
        };
        let mut name = path::display(op.as_bytes());
        if st.kind == FileType::Symlink {
            let target = ctx.vfs.read_link(node).unwrap_or_default();
            name = alloc::format!("{name} -> {}", path::display(&target));
        }
        let kind = match st.kind {
            FileType::Regular if st.size == 0 => "regular empty file",
            FileType::Regular => "regular file",
            FileType::Directory => "directory",
            FileType::Symlink => "symbolic link",
            FileType::CharDev => "character special file",
            FileType::BlockDev => "block special file",
            FileType::Fifo => "fifo",
            FileType::Socket => "socket",
        };
        outln!(ctx, "  File: {name}");
        outln!(
            ctx,
            "  Size: {:<10}\tBlocks: {:<10} IO Block: {:<6} {kind}",
            st.size,
            st.blocks,
            st.block_size
        );
        outln!(ctx, "Inode: {:<11} Links: {}", st.ino, st.nlink);
        outln!(
            ctx,
            "Access: ({:04o}/{})  Uid: ({:>5}/{:>8})   Gid: ({:>5}/{:>8})",
            st.perm,
            mode_string(&st),
            st.uid,
            owner(st.uid),
            st.gid,
            owner(st.gid)
        );
        outln!(ctx, "Access: {}", time::full(st.atime));
        outln!(ctx, "Modify: {}", time::full(st.mtime));
        outln!(ctx, "Change: {}", time::full(st.ctime));
        outln!(ctx, " Birth: -");
    }
    status
}

#[cfg(test)]
mod tests {
    use crate::testing::{Harness, memfs};

    #[test]
    fn stat_shows_everything_about_a_file() {
        let mut h = Harness::new();
        h.put("/tmp/notes", b"hello\n");
        let (status, text) = h.run("stat /tmp/notes");
        assert_eq!(status, 0);
        let lines: alloc::vec::Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "  File: /tmp/notes");
        assert_eq!(
            lines[1],
            "  Size: 6         \tBlocks: 8          IO Block: 4096   regular file"
        );
        assert!(lines[2].starts_with("Inode: ") && lines[2].ends_with(" Links: 1"));
        assert_eq!(
            lines[3],
            "Access: (0644/-rw-r--r--)  Uid: (    0/    root)   Gid: (    0/    root)"
        );
        assert_eq!(lines[4], "Access: 2026-09-26 12:00:00.000000000 +0000");
        assert_eq!(lines[5], "Modify: 2026-09-26 12:00:00.000000000 +0000");
        assert_eq!(lines[6], "Change: 2026-09-26 12:00:00.000000000 +0000");
        assert_eq!(lines[7], " Birth: -");
        assert_eq!(lines.len(), 8);
    }

    #[test]
    fn stat_names_the_file_type() {
        let mut fs = memfs();
        let root = vfs::FileSystem::root(&fs);
        fs.symlink(root, b"link", b"/etc").unwrap();
        let mut h = Harness::on(fs);
        h.put("/tmp/empty", b"");
        assert!(
            h.run("stat /tmp")
                .1
                .contains("IO Block: 4096   directory\n")
        );
        assert!(h.run("stat /tmp/empty").1.contains("regular empty file\n"));
        let link = h.run("stat /link").1;
        assert!(link.starts_with("  File: /link -> /etc\n"));
        assert!(link.contains("symbolic link\n"));
        assert!(link.contains("(0777/lrwxrwxrwx)"));
    }

    #[test]
    fn stat_errors() {
        let mut h = Harness::new();
        let (status, text) = h.run("stat /nope /tmp");
        assert_eq!(status, 1);
        assert!(
            text.starts_with(
                "stat: cannot stat '/nope': No such file or directory\n  File: /tmp\n"
            )
        );
        assert_eq!(h.run("stat"), (1, "stat: missing operand\n".into()));
    }
}
