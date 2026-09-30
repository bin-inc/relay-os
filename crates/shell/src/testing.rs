//! Test doubles for the shell's traits, and a harness that runs command
//! lines against an in-memory filesystem.
#![cfg(test)]

use crate::Shell;
use crate::io::{Console, MemInfo, Programs, Stdout, System};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use relay_abi::WaitStatus;
use vfs::{DirEntry, Env, Errno, FileSystem, Ino, MemFs, MountTable, Node, Stat, StatFs, Vfs};

/// The time every test runs at: Sat Sep 26 12:00:00 UTC 2026.
pub const NOW: u64 = 1_790_424_000;

pub struct TestConsole {
    pub input: VecDeque<u8>,
    pub output: Vec<u8>,
    pub columns: usize,
    /// Ctrl-C was pressed while a command runs.
    pub interrupt: bool,
    /// Ctrl-C is pressed once `interrupted` has answered false this many
    /// times.
    pub interrupt_after: Option<usize>,
}

impl TestConsole {
    pub fn new() -> TestConsole {
        TestConsole {
            input: VecDeque::new(),
            output: Vec::new(),
            columns: 80,
            interrupt: false,
            interrupt_after: None,
        }
    }

    pub fn type_in(&mut self, bytes: &[u8]) {
        self.input.extend(bytes);
    }

    /// Everything written so far.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    /// Everything written since the last call.
    pub fn take(&mut self) -> String {
        let text = self.text();
        self.output.clear();
        text
    }
}

impl Console for TestConsole {
    fn read_byte(&mut self) -> Option<u8> {
        self.input.pop_front()
    }
    fn write(&mut self, bytes: &[u8]) {
        self.output.extend_from_slice(bytes);
    }
    fn columns(&self) -> usize {
        self.columns
    }
    fn interrupted(&mut self) -> bool {
        match &mut self.interrupt_after {
            Some(0) => self.interrupt = true,
            Some(n) => *n -= 1,
            None => {}
        }
        self.interrupt
    }
}

pub struct TestSystem {
    pub now: u64,
    pub log: Vec<u8>,
    pub memory: Option<MemInfo>,
    pub reboots: u32,
    pub poweroffs: u32,
    /// What `reboot` and `poweroff` say, unforced, as a program's `power`
    /// call does when the filesystems cannot be shut down.
    pub power_error: Option<Errno>,
    /// Whether each `reboot` and `poweroff` was forced.
    pub forced: Vec<bool>,
}

impl TestSystem {
    fn power(&mut self, force: bool) -> Result<(), Errno> {
        self.forced.push(force);
        match self.power_error {
            Some(e) if !force => Err(e),
            _ => Ok(()),
        }
    }
}

impl TestSystem {
    pub fn new() -> TestSystem {
        TestSystem {
            now: NOW,
            log: Vec::new(),
            memory: None,
            reboots: 0,
            poweroffs: 0,
            power_error: None,
            forced: Vec::new(),
        }
    }
}

impl System for TestSystem {
    fn now(&self) -> u64 {
        self.now
    }
    fn memory(&self) -> Option<MemInfo> {
        self.memory
    }
    fn kernel_log(&self) -> Vec<u8> {
        self.log.clone()
    }
    fn reboot(&mut self, force: bool) -> Result<(), Errno> {
        self.power(force)?;
        self.reboots += 1;
        Ok(())
    }
    fn poweroff(&mut self, force: bool) -> Result<(), Errno> {
        self.power(force)?;
        self.poweroffs += 1;
        Ok(())
    }
}

/// What a spawning shell asked `FakePrograms` to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawned {
    pub path: String,
    pub args: Vec<String>,
    /// The redirection's fd, as fd 1.
    pub stdout: Option<u32>,
    pub foreground: bool,
}

/// `/bin/sh`'s system calls, for the spawning runner: redirection files
/// are opened by path (and recorded, not created), and a program is known
/// by its path.
pub struct FakePrograms {
    /// The programs `spawn` starts, by path, and how each ends.
    pub known: Vec<(&'static str, WaitStatus)>,
    /// What `spawn` says of a path it does not know (default `ENOENT`).
    pub refusals: Vec<(&'static str, Errno)>,
    /// Every redirection opened: its path, whether it appends, its fd.
    pub opened: Vec<(String, bool, u32)>,
    /// What `open_output` fails with, if anything.
    pub open_error: Option<Errno>,
    pub closed: Vec<u32>,
    pub spawned: Vec<Spawned>,
    /// The children started and not yet waited for, by pid.
    children: Vec<(u32, WaitStatus)>,
    /// The tees pushed and not popped, by path.
    pub tees: Vec<String>,
    /// Every tee pushed.
    pub pushed: Vec<String>,
    /// What `tee_push` fails with, if anything.
    pub push_error: Option<Errno>,
    /// What the next `tee_pop` returns, as a failed write would.
    pub pop_error: Option<Errno>,
    next_fd: u32,
    next_pid: u32,
}

impl FakePrograms {
    pub fn new() -> FakePrograms {
        FakePrograms {
            known: Vec::new(),
            refusals: Vec::new(),
            opened: Vec::new(),
            open_error: None,
            closed: Vec::new(),
            spawned: Vec::new(),
            children: Vec::new(),
            tees: Vec::new(),
            pushed: Vec::new(),
            push_error: None,
            pop_error: None,
            next_fd: 3,
            next_pid: 100,
        }
    }
}

impl Programs for FakePrograms {
    fn open_output(&mut self, path: &[u8], append: bool) -> Result<u32, Errno> {
        if let Some(e) = self.open_error {
            return Err(e);
        }
        self.next_fd += 1;
        let path = String::from_utf8_lossy(path).into_owned();
        self.opened.push((path, append, self.next_fd));
        Ok(self.next_fd)
    }
    fn close(&mut self, fd: u32) {
        self.closed.push(fd);
    }
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdout: Option<u32>,
        foreground: bool,
    ) -> Result<u32, Errno> {
        let path = String::from_utf8_lossy(path).into_owned();
        let Some(&(_, status)) = self.known.iter().find(|(p, _)| *p == path) else {
            let refusal = self.refusals.iter().find(|(p, _)| *p == path);
            return Err(refusal.map_or(Errno::ENOENT, |r| r.1));
        };
        self.spawned.push(Spawned {
            path,
            args: args
                .iter()
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect(),
            stdout,
            foreground,
        });
        self.next_pid += 1;
        self.children.push((self.next_pid, status));
        Ok(self.next_pid)
    }
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        let i = self
            .children
            .iter()
            .position(|c| c.0 == pid)
            .ok_or(Errno::ECHILD)?;
        Ok(self.children.remove(i).1)
    }
    fn tee_push(&mut self, path: &[u8]) -> Result<(), Errno> {
        if let Some(e) = self.push_error {
            return Err(e);
        }
        let path = String::from_utf8_lossy(path).into_owned();
        self.pushed.push(path.clone());
        self.tees.push(path);
        Ok(())
    }
    fn tee_pop(&mut self) -> Result<(), Errno> {
        self.tees.pop().ok_or(Errno::EINVAL)?;
        self.pop_error.take().map_or(Ok(()), Err)
    }
}

/// A program's fd 1: the console or a file, keeping every write whole.
pub struct FakeStdout {
    pub tty: bool,
    pub node: Option<Node>,
    pub writes: Vec<Vec<u8>>,
    /// Writes fail with this once this many bytes were written.
    pub fail_after: Option<(usize, Errno)>,
}

impl FakeStdout {
    pub fn console() -> FakeStdout {
        FakeStdout {
            tty: true,
            node: None,
            writes: Vec::new(),
            fail_after: None,
        }
    }

    pub fn file(node: Option<Node>) -> FakeStdout {
        FakeStdout {
            tty: false,
            ..FakeStdout::console()
        }
        .with_node(node)
    }

    fn with_node(mut self, node: Option<Node>) -> FakeStdout {
        self.node = node;
        self
    }

    /// Everything written.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.writes.concat()).into_owned()
    }
}

impl Stdout for FakeStdout {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Errno> {
        if let Some((limit, e)) = self.fail_after
            && self.writes.iter().map(Vec::len).sum::<usize>() + bytes.len() > limit
        {
            return Err(e);
        }
        self.writes.push(bytes.to_vec());
        Ok(())
    }
    fn is_tty(&self) -> bool {
        self.tty
    }
    fn node(&self) -> Option<Node> {
        self.node
    }
}

struct Clock;

impl Env for Clock {
    fn now(&self) -> u64 {
        NOW
    }
    fn log(&self, _: &str) {}
}

/// An empty `MemFs` at the tests' time, to prepare for [`Harness::on`].
pub fn memfs() -> MemFs {
    MemFs::new(Box::new(Clock))
}

/// What the spy saw, and the failures it should inject.
#[derive(Default)]
pub struct SpyState {
    pub syncs: Cell<u32>,
    pub shutdowns: Cell<u32>,
    pub fail_sync: Cell<Option<Errno>>,
    pub fail_shutdown: Cell<Option<Errno>>,
    /// Every write reports 0 bytes written (a broken filesystem).
    pub zero_writes: Cell<bool>,
    pub fail_unlink: Cell<Option<Errno>>,
    /// `read_at` calls.
    pub reads: Cell<u32>,
    /// The most bytes one `write_at` was given.
    pub largest_write: Cell<usize>,
}

/// A `MemFs` that counts syncs and shutdowns and can make them fail.
struct Spy {
    fs: MemFs,
    state: Rc<SpyState>,
}

impl FileSystem for Spy {
    fn root(&self) -> Ino {
        self.fs.root()
    }
    fn stat(&mut self, ino: Ino) -> Result<Stat, Errno> {
        self.fs.stat(ino)
    }
    fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.fs.lookup(dir, name)
    }
    fn read_dir(&mut self, dir: Ino) -> Result<Vec<DirEntry>, Errno> {
        self.fs.read_dir(dir)
    }
    fn read_link(&mut self, ino: Ino) -> Result<Vec<u8>, Errno> {
        self.fs.read_link(ino)
    }
    fn read_at(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, Errno> {
        self.state.reads.set(self.state.reads.get() + 1);
        self.fs.read_at(ino, offset, buf)
    }
    fn write_at(&mut self, ino: Ino, offset: u64, buf: &[u8]) -> Result<usize, Errno> {
        let largest = self.state.largest_write.get().max(buf.len());
        self.state.largest_write.set(largest);
        if self.state.zero_writes.get() {
            return Ok(0);
        }
        self.fs.write_at(ino, offset, buf)
    }
    fn truncate(&mut self, ino: Ino, size: u64) -> Result<(), Errno> {
        self.fs.truncate(ino, size)
    }
    fn touch(&mut self, ino: Ino) -> Result<(), Errno> {
        self.fs.touch(ino)
    }
    fn create(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.fs.create(dir, name)
    }
    fn mkdir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, Errno> {
        self.fs.mkdir(dir, name)
    }
    fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        match self.state.fail_unlink.get() {
            Some(e) => Err(e),
            None => self.fs.unlink(dir, name),
        }
    }
    fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), Errno> {
        self.fs.rmdir(dir, name)
    }
    fn rename(&mut self, from_dir: Ino, from: &[u8], to_dir: Ino, to: &[u8]) -> Result<(), Errno> {
        self.fs.rename(from_dir, from, to_dir, to)
    }
    fn statfs(&mut self) -> Result<StatFs, Errno> {
        self.fs.statfs()
    }
    fn sync(&mut self) -> Result<(), Errno> {
        self.state.syncs.set(self.state.syncs.get() + 1);
        match self.state.fail_sync.get() {
            Some(e) => Err(e),
            None => self.fs.sync(),
        }
    }
    fn shutdown(&mut self) -> Result<(), Errno> {
        self.state.shutdowns.set(self.state.shutdowns.get() + 1);
        match self.state.fail_shutdown.get() {
            Some(e) => Err(e),
            None => self.fs.shutdown(),
        }
    }
}

/// `/etc/motd`, `/etc/hostname`, `/root/` and `/tmp/` on a spied `MemFs`.
fn standard(fs: MemFs) -> (MountTable, Rc<SpyState>) {
    let state = Rc::new(SpyState::default());
    let mut vfs = MountTable::new(Box::new(Spy {
        fs,
        state: state.clone(),
    }));
    for dir in ["/etc", "/root", "/tmp"] {
        vfs.mkdir(dir.as_bytes()).unwrap();
    }
    for (path, data) in [
        ("/etc/motd", &b"Welcome to Relay OS.\n"[..]),
        ("/etc/hostname", b"relay\n"),
    ] {
        let node = vfs.create(path.as_bytes()).unwrap();
        vfs.write_at(node, 0, data).unwrap();
    }
    (vfs, state)
}

/// A shell's surroundings, reused across command lines (the current
/// directory carries over).
pub struct Harness {
    pub vfs: MountTable,
    pub console: TestConsole,
    pub system: TestSystem,
    pub programs: FakePrograms,
    pub spy: Rc<SpyState>,
}

impl Harness {
    /// The standard tree, current directory `/`.
    pub fn new() -> Harness {
        Harness::on(memfs())
    }

    /// The standard tree on a filesystem holding at most `bytes` of data.
    pub fn with_capacity(bytes: u64) -> Harness {
        Harness::on(memfs().with_capacity(bytes))
    }

    /// Nothing but an empty root directory.
    pub fn empty() -> Harness {
        let state = Rc::new(SpyState::default());
        let fs = Spy {
            fs: memfs(),
            state: state.clone(),
        };
        Harness {
            vfs: MountTable::new(Box::new(fs)),
            console: TestConsole::new(),
            system: TestSystem::new(),
            programs: FakePrograms::new(),
            spy: state,
        }
    }

    /// The standard tree added to `fs`.
    pub fn on(fs: MemFs) -> Harness {
        let (vfs, spy) = standard(fs);
        Harness {
            vfs,
            console: TestConsole::new(),
            system: TestSystem::new(),
            programs: FakePrograms::new(),
            spy,
        }
    }

    /// Runs one command line; its exit status and everything it printed.
    pub fn run(&mut self, line: &str) -> (i32, String) {
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system).execute(line);
        (status, self.console.take())
    }

    /// Runs one command line in a spawning shell (`/bin/sh`'s); its exit
    /// status and what the shell printed.
    pub fn spawning(&mut self, line: &str) -> (i32, String) {
        let status = Shell::spawning(
            &mut self.vfs,
            &mut self.console,
            &mut self.system,
            &mut self.programs,
        )
        .execute(line);
        (status, self.console.take())
    }

    /// Runs `line` as the program named by its first word does, standard
    /// output going to `stdout`; its status and what it said on the
    /// console.
    pub fn program(&mut self, line: &str, stdout: &mut FakeStdout) -> (i32, String) {
        let words = crate::parser::parse(line).unwrap().words;
        let status = crate::run_command(
            &words[0],
            crate::commands::find(&words[0]).unwrap().run,
            &words[1..],
            &mut self.vfs,
            &mut self.console,
            &mut self.system,
            stdout,
        );
        (status, self.console.take())
    }

    /// Runs `/bin/sh` with `args` (after argument 0), as a spawning shell
    /// does, its fd 1 `stdout`; its status and what it printed.
    pub fn sh(&mut self, args: &[&str], stdout: &mut FakeStdout) -> (i32, String) {
        let args: Vec<String> = args.iter().map(|a| String::from(*a)).collect();
        let status = Shell::spawning(
            &mut self.vfs,
            &mut self.console,
            &mut self.system,
            &mut self.programs,
        )
        .run_file(&args, stdout);
        (status, self.console.take())
    }

    /// Creates (or replaces) a file.
    pub fn put(&mut self, path: &str, data: &[u8]) {
        let node = match self.vfs.lookup(path.as_bytes()) {
            Ok(node) => {
                self.vfs.truncate(node, 0).unwrap();
                node
            }
            Err(_) => self.vfs.create(path.as_bytes()).unwrap(),
        };
        self.vfs.write_at(node, 0, data).unwrap();
    }

    /// A file's contents.
    pub fn get(&mut self, path: &str) -> Vec<u8> {
        let node = self.vfs.lookup(path.as_bytes()).unwrap();
        let size = self.vfs.stat(node).unwrap().size as usize;
        let mut buf = alloc::vec![0; size];
        assert_eq!(self.vfs.read_at(node, 0, &mut buf).unwrap(), size);
        buf
    }

    pub fn dir(&mut self, path: &str) {
        self.vfs.mkdir(path.as_bytes()).unwrap();
    }

    pub fn exists(&mut self, path: &str) -> bool {
        self.vfs.lookup(path.as_bytes()).is_ok()
    }
}
