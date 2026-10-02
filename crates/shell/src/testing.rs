//! Test doubles for the shell's traits, and a harness that runs command
//! lines against an in-memory filesystem.
#![cfg(test)]

use crate::Shell;
use crate::io::{Bytes, Console, Group, MemInfo, Programs, Stdout, System};
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

/// The most a test's shell may ask for a Ctrl-C, which it does before each
/// command: past it a loop that never ends fails the test instead of
/// hanging it.
const ASKED_MAX: usize = 100_000;

pub struct TestConsole {
    pub input: VecDeque<u8>,
    pub output: Vec<u8>,
    pub columns: usize,
    /// Ctrl-C was pressed while a command runs.
    pub interrupt: bool,
    /// Ctrl-C is pressed once `interrupted` has answered false this many
    /// times.
    pub interrupt_after: Option<usize>,
    /// How many times `interrupted` was asked.
    asked: usize,
    /// How many times the shell took the console back.
    pub taken_back: usize,
}

impl TestConsole {
    pub fn new() -> TestConsole {
        TestConsole {
            input: VecDeque::new(),
            output: Vec::new(),
            columns: 80,
            interrupt: false,
            interrupt_after: None,
            asked: 0,
            taken_back: 0,
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
    fn take_back(&mut self) {
        self.taken_back += 1;
    }

    /// As the real console's, a Ctrl-C is taken by the call that sees it.
    fn interrupted(&mut self) -> bool {
        self.asked += 1;
        assert!(
            self.asked <= ASKED_MAX,
            "asked for a Ctrl-C {ASKED_MAX} times: a loop that does not end"
        );
        match &mut self.interrupt_after {
            Some(0) => {
                self.interrupt_after = None;
                self.interrupt = true;
            }
            Some(n) => *n -= 1,
            None => {}
        }
        core::mem::take(&mut self.interrupt)
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
    /// Every `sleep`, in milliseconds.
    pub slept: Vec<u64>,
    /// What `processes` says.
    pub processes: Option<Vec<relay_abi::ProcInfo>>,
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
            slept: Vec::new(),
            processes: None,
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
    fn processes(&self) -> Option<Vec<relay_abi::ProcInfo>> {
        self.processes.clone()
    }
    fn sleep(&mut self, ms: u64) {
        self.slept.push(ms);
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
    /// What it got as fd 0 (a pipe's read end) and fd 1 (a pipe's write
    /// end or a redirection), instead of the shell's.
    pub stdin: Option<u32>,
    pub stdout: Option<u32>,
    pub group: Group,
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
    /// Every pipe made, (read end, write end); what `pipe` fails with
    /// after this many, if anything.
    pub pipes: Vec<(u32, u32)>,
    pub pipe_error: Option<(usize, Errno)>,
    pub closed: Vec<u32>,
    pub spawned: Vec<Spawned>,
    /// The programs that run on through this many rounds of `collect` (a
    /// round ends when it finds nothing), by path; the others end at once.
    pub lives: Vec<(&'static str, u32)>,
    /// The children started and not yet collected.
    children: Vec<FakeChild>,
    /// The rounds of `collect` so far.
    round: u32,
    /// How many children were not yet collected at each `spawn`.
    pub alive_at_spawn: Vec<usize>,
    /// A Ctrl-C ends the `wait_or_ctrl_c` after this many more.
    pub ctrl_c_after: Option<usize>,
    /// Every pid `wait_or_ctrl_c` waited for.
    pub waited: Vec<u32>,
    /// Every `kill`'s target.
    pub kills: Vec<i64>,
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
            pipes: Vec::new(),
            pipe_error: None,
            closed: Vec::new(),
            spawned: Vec::new(),
            lives: Vec::new(),
            children: Vec::new(),
            round: 0,
            alive_at_spawn: Vec::new(),
            ctrl_c_after: None,
            waited: Vec::new(),
            kills: Vec::new(),
            tees: Vec::new(),
            pushed: Vec::new(),
            push_error: None,
            pop_error: None,
            next_fd: 3,
            next_pid: 100,
        }
    }
}

/// A child of `FakePrograms`: how it ends, its group, and the round of
/// `collect` from which it has ended.
struct FakeChild {
    pid: u32,
    status: WaitStatus,
    group: u32,
    ends_at: u32,
}

impl FakePrograms {
    /// The children not yet collected, by pid, with their groups.
    pub fn children(&self) -> Vec<(u32, u32)> {
        self.children.iter().map(|c| (c.pid, c.group)).collect()
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
    fn pipe(&mut self) -> Result<(u32, u32), Errno> {
        if let Some((after, e)) = self.pipe_error
            && self.pipes.len() >= after
        {
            return Err(e);
        }
        let ends = (self.next_fd + 1, self.next_fd + 2);
        self.next_fd += 2;
        self.pipes.push(ends);
        Ok(ends)
    }
    fn spawn(
        &mut self,
        path: &[u8],
        args: &[&[u8]],
        stdin: Option<u32>,
        stdout: Option<u32>,
        group: Group,
    ) -> Result<u32, Errno> {
        let path = String::from_utf8_lossy(path).into_owned();
        self.alive_at_spawn.push(self.children.len());
        let life = self
            .lives
            .iter()
            .find(|(p, _)| *p == path)
            .map_or(0, |l| l.1);
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
            stdin,
            stdout,
            group,
        });
        self.next_pid += 1;
        let group = match group {
            Group::Shell => 0,
            Group::New | Group::Background => self.next_pid,
            Group::Join(g) => g,
        };
        self.children.push(FakeChild {
            pid: self.next_pid,
            status,
            group,
            ends_at: self.round + life,
        });
        Ok(self.next_pid)
    }
    /// Waits as long as the child runs on.
    fn wait(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        let i = self
            .children
            .iter()
            .position(|c| c.pid == pid)
            .ok_or(Errno::ECHILD)?;
        Ok(self.children.remove(i).status)
    }
    /// Its children end at once, killed, as the kernel's `kill` ends them;
    /// one that has ended already and is not yet collected counts, but
    /// keeps how it ended, as a zombie does. Process 1 is refused, and any
    /// other pid or group is none.
    fn kill(&mut self, target: i64) -> Result<(), Errno> {
        self.kills.push(target);
        if target == 1 || target == -1 {
            return Err(Errno::EPERM);
        }
        let hit = |c: &FakeChild| match target {
            t if t < 0 => i64::from(c.group) == -t,
            t => i64::from(c.pid) == t,
        };
        let mut found = false;
        let round = self.round;
        for c in self.children.iter_mut().filter(|c| hit(c)) {
            if c.ends_at > round {
                c.status = WaitStatus::killed(relay_abi::wait::KILLED_KILL);
                c.ends_at = round;
            }
            found = true;
        }
        if found { Ok(()) } else { Err(Errno::ESRCH) }
    }
    fn wait_or_ctrl_c(&mut self, pid: u32) -> Result<WaitStatus, Errno> {
        if let Some(n) = &mut self.ctrl_c_after {
            if *n == 0 {
                self.ctrl_c_after = None;
                return Err(Errno::EINTR);
            }
            *n -= 1;
        }
        self.waited.push(pid);
        self.wait(pid)
    }
    fn collect(&mut self) -> Option<(u32, WaitStatus)> {
        match self.children.iter().position(|c| c.ends_at <= self.round) {
            Some(i) => {
                let c = self.children.remove(i);
                Some((c.pid, c.status))
            }
            None => {
                self.round += 1;
                None
            }
        }
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
    /// The next command's standard input (in-process and as a program).
    pub stdin: Vec<u8>,
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
            stdin: Vec::new(),
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
            stdin: Vec::new(),
        }
    }

    /// Runs one command line; its exit status and everything it printed.
    pub fn run(&mut self, line: &str) -> (i32, String) {
        let mut input = Bytes::new(core::mem::take(&mut self.stdin));
        let status = Shell::new(&mut self.vfs, &mut self.console, &mut self.system)
            .with_input(&mut input)
            .execute(line);
        (status, self.console.take())
    }

    /// Runs `lines` one after another in one shell, as if typed (no
    /// prompt); the last one's status and everything they printed.
    pub fn lines(&mut self, lines: &[&str]) -> (i32, String) {
        let mut input = Bytes::new(core::mem::take(&mut self.stdin));
        let mut shell =
            Shell::new(&mut self.vfs, &mut self.console, &mut self.system).with_input(&mut input);
        let mut status = 0;
        for line in lines {
            status = shell.execute(line);
        }
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
        let words = crate::parser::parse(line).unwrap().remove(0).words;
        let status = crate::run_command(
            &words[0],
            crate::commands::find(&words[0]).unwrap().run,
            &words[1..],
            crate::CommandIo {
                vfs: &mut self.vfs,
                console: &mut self.console,
                system: &mut self.system,
                stdin: &mut Bytes::new(core::mem::take(&mut self.stdin)),
                stdout,
            },
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

    /// Runs `args` (the command's name first) as its program does, in a
    /// fresh directory holding `files` (a name ending in `/` is a
    /// directory), with `stdin` as its input: its exit
    /// status, standard output and standard error, to compare with
    /// [`host_tool`]'s.
    pub fn like_host(
        &mut self,
        args: &[&str],
        files: &[(&str, &[u8])],
        stdin: &[u8],
    ) -> (i32, String, String) {
        let dir = std::format!("/tmp/host{}", next_dir());
        self.dir(&dir);
        for (name, data) in files {
            match name.strip_suffix('/') {
                Some(d) => self.dir(&std::format!("{dir}/{d}")),
                None => self.put(&std::format!("{dir}/{name}"), data),
            }
        }
        self.vfs.chdir(dir.as_bytes()).unwrap();
        self.stdin = stdin.to_vec();
        let mut out = FakeStdout::file(None);
        let (status, errors) = self.program_args(args, &mut out);
        self.vfs.chdir(b"/").unwrap();
        (status, out.text(), errors)
    }

    /// As [`Harness::program`], with the words given (the command's name
    /// first), so that any byte can be in one.
    pub fn program_args(&mut self, args: &[&str], stdout: &mut FakeStdout) -> (i32, String) {
        let words: Vec<String> = args.iter().map(|a| String::from(*a)).collect();
        let status = crate::run_command(
            &words[0],
            crate::commands::find(&words[0]).unwrap().run,
            &words[1..],
            crate::CommandIo {
                vfs: &mut self.vfs,
                console: &mut self.console,
                system: &mut self.system,
                stdin: &mut Bytes::new(core::mem::take(&mut self.stdin)),
                stdout,
            },
        );
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

/// A file a test makes on both sides, in a fresh directory: on the host
/// for its own tool ([`host_files`]) and in a `MemFs` for ours
/// ([`like_host_files`]).
#[derive(Clone, Copy, Debug)]
pub struct TestFile<'a> {
    pub name: &'a str,
    pub made: Made<'a>,
    /// Access and modification times, in seconds since 1970.
    pub times: Option<(u64, u64)>,
}

/// What a [`TestFile`] is.
#[derive(Clone, Copy, Debug)]
pub enum Made<'a> {
    File(&'a [u8]),
    Dir,
    /// Another name for the file of that name, made before it.
    HardLink(&'a str),
}

impl<'a> TestFile<'a> {
    pub fn file(name: &'a str, data: &'a [u8]) -> TestFile<'a> {
        TestFile::made(name, Made::File(data))
    }

    pub fn dir(name: &'a str) -> TestFile<'a> {
        TestFile::made(name, Made::Dir)
    }

    pub fn hard_link(name: &'a str, to: &'a str) -> TestFile<'a> {
        TestFile::made(name, Made::HardLink(to))
    }

    fn made(name: &'a str, made: Made<'a>) -> TestFile<'a> {
        TestFile {
            name,
            made,
            times: None,
        }
    }

    pub fn times(self, atime: u64, mtime: u64) -> TestFile<'a> {
        TestFile {
            times: Some((atime, mtime)),
            ..self
        }
    }
}

/// Runs `args` (the command's name first) as its program, in `/w` of a
/// fresh standard tree holding `files`: its status, standard output and
/// standard error, to compare with [`host_files`]'.
pub fn like_host_files(args: &[&str], files: &[TestFile<'_>]) -> (i32, String, String) {
    let mut fs = memfs();
    let w = fs.mkdir(fs.root(), b"w").unwrap();
    let mut made: Vec<(&str, Ino)> = Vec::new();
    for f in files {
        let (dir, name) = match f.name.rsplit_once('/') {
            Some((d, n)) => (lookup_in(&mut fs, w, d), n),
            None => (w, f.name),
        };
        let ino = match f.made {
            Made::File(data) => {
                let ino = fs.create(dir, name.as_bytes()).unwrap();
                fs.write_at(ino, 0, data).unwrap();
                ino
            }
            Made::Dir => fs.mkdir(dir, name.as_bytes()).unwrap(),
            Made::HardLink(to) => {
                let ino = made.iter().find(|m| m.0 == to).unwrap().1;
                fs.link(dir, name.as_bytes(), ino).unwrap();
                ino
            }
        };
        made.push((f.name, ino));
    }
    // Last, as on the host: making a file changes its directory's times.
    for (f, &(_, ino)) in files.iter().zip(&made) {
        if let Some((atime, mtime)) = f.times {
            fs.set_times(ino, atime, mtime).unwrap();
        }
    }
    let mut h = Harness::on(fs);
    h.vfs.chdir(b"/w").unwrap();
    let mut out = FakeStdout::file(None);
    let (status, errors) = h.program_args(args, &mut out);
    (status, out.text(), errors)
}

/// The directory `path` under `dir`.
fn lookup_in(fs: &mut MemFs, dir: Ino, path: &str) -> Ino {
    path.split('/')
        .fold(dir, |d, name| fs.lookup(d, name.as_bytes()).unwrap())
}

/// What the host's own tool prints for `args` (its name first), as
/// [`host_tool`] runs it, in a fresh directory holding `files`; with
/// `root`, through `unshare -r`, so that it answers as root (Relay OS runs
/// every program as root). A missing tool, or a namespace refused, fails
/// the test.
pub fn host_files(args: &[&str], files: &[TestFile<'_>], root: bool) -> (i32, String, String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/like-host")
        .join(std::format!("{}-{}", std::process::id(), next_dir()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in files {
        let path = dir.join(f.name);
        match f.made {
            Made::File(data) => std::fs::write(&path, data).unwrap(),
            Made::Dir => std::fs::create_dir(&path).unwrap(),
            Made::HardLink(to) => std::fs::hard_link(dir.join(to), &path).unwrap(),
        }
    }
    for f in files {
        if let Some((atime, mtime)) = f.times {
            for (flag, t) in [("-a", atime), ("-m", mtime)] {
                let ok = std::process::Command::new("touch")
                    .args(["-h", flag, "-d", &std::format!("@{t}")])
                    .arg(dir.join(f.name))
                    .status()
                    .expect("the host's touch is needed")
                    .success();
                assert!(ok, "touch {flag} {}", f.name);
            }
        }
    }
    let mut cmd = if root {
        let mut c = std::process::Command::new("unshare");
        c.arg("-r").args(args);
        c
    } else {
        let mut c = std::process::Command::new(args[0]);
        c.args(&args[1..]);
        c
    };
    let out = cmd
        .current_dir(&dir)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    let _ = std::fs::remove_dir_all(&dir);
    let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
    let stderr = text(out.stderr);
    if root {
        assert!(
            !stderr.starts_with("unshare:"),
            "unshare -r is needed (on Ubuntu 24.04, sysctl \
             kernel.apparmor_restrict_unprivileged_userns=0): {stderr}"
        );
    }
    (out.status.code().unwrap_or(-1), text(out.stdout), stderr)
}

/// A number for each fresh directory a test asks for.
fn next_dir() -> usize {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// What the host's own tool prints for `args` (its name first), run in a
/// fresh directory under the workspace's `target/` holding `files` (a name
/// ending in `/` is a directory), with
/// `stdin` as its input and `LC_ALL=C`: its exit status, standard output
/// and standard error, to compare with [`Harness::like_host`]'s. A tool
/// that is missing fails the test.
pub fn host_tool(args: &[&str], files: &[(&str, &[u8])], stdin: &[u8]) -> (i32, String, String) {
    use std::io::Write;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/like-host")
        .join(std::format!("{}-{}", std::process::id(), next_dir()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, data) in files {
        match name.strip_suffix('/') {
            Some(d) => std::fs::create_dir(dir.join(d)).unwrap(),
            None => std::fs::write(dir.join(name), data).unwrap(),
        }
    }
    let mut child = std::process::Command::new(args[0])
        .args(&args[1..])
        .current_dir(&dir)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("the host's {} is needed: {e}", args[0]));
    // From a thread: a tool that writes much before it has read all would
    // otherwise wait on its full output while this waits on its input. A
    // tool that stops reading early (head) closes the pipe: not an error.
    let (mut pipe, data) = (child.stdin.take().unwrap(), stdin.to_vec());
    let writer = std::thread::spawn(move || {
        let _ = pipe.write_all(&data);
    });
    let out = child.wait_with_output().unwrap();
    let _ = writer.join();
    let _ = std::fs::remove_dir_all(&dir);
    let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
    (
        out.status.code().unwrap_or(-1),
        text(out.stdout),
        text(out.stderr),
    )
}
