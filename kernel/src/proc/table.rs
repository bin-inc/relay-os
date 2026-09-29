//! The process table and the run queue (user-space gate §5.4, §6.1),
//! architecture-neutral: who exists, who is whose child and in which group,
//! who is ready, running, blocked or a zombie, and whose turn is next.
//! What a process owns besides that (its memory, kernel stack, fds) is the
//! payload `R`, so the rules are tested here with none.
//!
//! - Pids count from 1 and are not used again while the kernel runs; at
//!   most [`MAX`] processes exist, zombies included (`EAGAIN` beyond).
//! - Process 0 is the idle task: it has no entry, and runs when nothing is
//!   ready.
//! - A process that ends stays as a zombie until its parent `wait`s for it;
//!   its children pass to process 1.
//! - Round-robin: a process made ready joins the end of the queue, and one
//!   that used up its slice of [`SLICE`] ticks goes to the end when another
//!   is ready.
//! - `kill` and Ctrl-C only mark a process and wake it if it is blocked; it
//!   ends itself the next time it runs, before any more of its code runs.

use alloc::string::String;
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use vfs::Errno;

/// The most processes that exist at once (spec §5.4).
pub const MAX: usize = 64;
/// Process 1: the parent of orphans, and the one `kill` refuses.
pub const INIT: u32 = 1;
/// Timer ticks in a time slice (spec §6.1).
pub const SLICE: u32 = 10;

/// What a blocked process waits for (spec §6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// A child to end.
    Wait,
    /// Console input.
    Console,
    /// The tick count to reach this value.
    Sleep(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    Running,
    Blocked(Blocked),
    Zombie(WaitStatus),
}

pub struct Process<R> {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    /// Its path, for the kernel log (and `ps`, milestone 3).
    pub name: String,
    pub state: State,
    /// Why it must end when it runs next: `KILLED_CTRL_C` or `KILLED_KILL`.
    pub killed: Option<u32>,
    /// Timer ticks it has run for.
    pub ticks: u64,
    pub res: R,
}

/// Which child `wait` waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Any,
    Pid(u32),
}

/// The ready processes in order, at most [`MAX`] of them.
struct Queue {
    pids: [u32; MAX],
    head: usize,
    len: usize,
}

impl Queue {
    const fn new() -> Queue {
        Queue {
            pids: [0; MAX],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, pid: u32) {
        // Every entry is a different process, so it always fits.
        assert!(self.len < MAX, "the run queue is full");
        self.pids[(self.head + self.len) % MAX] = pid;
        self.len += 1;
    }

    /// Takes `pid` out of the queue, keeping the others' order.
    fn remove(&mut self, pid: u32) {
        let mut kept = Queue::new();
        while let Some(p) = self.pop() {
            if p != pid {
                kept.push(p);
            }
        }
        *self = kept;
    }

    fn pop(&mut self) -> Option<u32> {
        if self.len == 0 {
            return None;
        }
        let pid = self.pids[self.head];
        self.head = (self.head + 1) % MAX;
        self.len -= 1;
        Some(pid)
    }
}

pub struct Table<R> {
    procs: Vec<Process<R>>,
    next_pid: u32,
    ready: Queue,
    /// The running process; 0 while the idle task runs.
    current: u32,
    /// Ticks left of the running process's slice.
    slice_left: u32,
}

impl<R> Default for Table<R> {
    fn default() -> Self {
        Table::new()
    }
}

impl<R> Table<R> {
    pub const fn new() -> Table<R> {
        Table {
            procs: Vec::new(),
            next_pid: 1,
            ready: Queue::new(),
            current: 0,
            slice_left: SLICE,
        }
    }

    pub fn len(&self) -> usize {
        self.procs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.procs.is_empty()
    }

    /// The running process's pid; 0 for the idle task.
    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn get(&self, pid: u32) -> Option<&Process<R>> {
        self.procs.iter().find(|p| p.pid == pid)
    }

    pub fn get_mut(&mut self, pid: u32) -> Option<&mut Process<R>> {
        self.procs.iter_mut().find(|p| p.pid == pid)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Process<R>> {
        self.procs.iter()
    }

    /// Adds a ready process, a child of `ppid`, in its parent's group or,
    /// with `new_group` (or no parent), in a new group numbered with its
    /// own pid; its pid. `EAGAIN` when [`MAX`] processes exist.
    pub fn insert(
        &mut self,
        ppid: u32,
        new_group: bool,
        name: String,
        res: R,
    ) -> Result<u32, Errno> {
        if self.procs.len() >= MAX {
            return Err(Errno::EAGAIN);
        }
        let pid = self.next_pid;
        self.next_pid += 1;
        let pgid = match self.get(ppid) {
            Some(parent) if !new_group => parent.pgid,
            _ => pid,
        };
        self.procs.push(Process {
            pid,
            ppid,
            pgid,
            name,
            state: State::Ready,
            killed: None,
            ticks: 0,
            res,
        });
        self.ready.push(pid);
        Ok(pid)
    }

    /// Gives up the CPU for the running process (which stays ready unless
    /// it has blocked or ended) and picks the next ready one, which is then
    /// running; 0 when none is and the idle task runs.
    pub fn schedule(&mut self) -> u32 {
        let me = self.current;
        if let Some(p) = self.get_mut(me)
            && p.state == State::Running
        {
            p.state = State::Ready;
            self.ready.push(me);
        }
        let next = self.ready.pop().unwrap_or(0);
        if let Some(p) = self.get_mut(next) {
            p.state = State::Running;
        }
        self.current = next;
        self.slice_left = SLICE;
        next
    }

    /// The running process blocks on `why`; the caller then calls
    /// `schedule`.
    pub fn block(&mut self, why: Blocked) {
        let me = self.current;
        if let Some(p) = self.get_mut(me) {
            p.state = State::Blocked(why);
        }
    }

    /// Makes `pid` ready if it is blocked; whether it was.
    pub fn wake(&mut self, pid: u32) -> bool {
        match self.get_mut(pid) {
            Some(p) if matches!(p.state, State::Blocked(_)) => {
                p.state = State::Ready;
                self.ready.push(pid);
                true
            }
            _ => false,
        }
    }

    /// Wakes every process blocked on `why`.
    pub fn wake_all(&mut self, why: Blocked) {
        let pids: Vec<u32> = self
            .procs
            .iter()
            .filter(|p| p.state == State::Blocked(why))
            .map(|p| p.pid)
            .collect();
        for pid in pids {
            self.wake(pid);
        }
    }

    /// Wakes the sleepers whose time has come at tick `now`.
    pub fn wake_sleepers(&mut self, now: u64) {
        let mut due = [0u32; MAX];
        let mut n = 0;
        for p in &self.procs {
            if let State::Blocked(Blocked::Sleep(until)) = p.state
                && until <= now
            {
                due[n] = p.pid;
                n += 1;
            }
        }
        for &pid in &due[..n] {
            self.wake(pid);
        }
    }

    /// Whether anything but the running process is ready.
    pub fn others_ready(&self) -> bool {
        self.ready.len > 0
    }

    /// A timer tick while the running process ran: counts it, and whether
    /// its slice is used up while another process is ready (it should
    /// then give up the CPU). A slice nobody else wants starts again.
    pub fn tick(&mut self) -> bool {
        let me = self.current;
        if let Some(p) = self.get_mut(me) {
            p.ticks += 1;
        }
        self.slice_left = self.slice_left.saturating_sub(1);
        if self.slice_left > 0 {
            return false;
        }
        self.slice_left = SLICE;
        self.others_ready()
    }

    /// `pid` has ended with `status`: it is a zombie until its parent
    /// waits for it, its children pass to process 1, and a parent blocked
    /// in `wait` is woken.
    pub fn end(&mut self, pid: u32, status: WaitStatus) {
        let Some(p) = self.get_mut(pid) else {
            return;
        };
        let was_ready = p.state == State::Ready;
        p.state = State::Zombie(status);
        let ppid = p.ppid;
        if was_ready {
            self.ready.remove(pid);
        }
        let mut orphaned_zombie = false;
        for child in self.procs.iter_mut().filter(|c| c.ppid == pid) {
            child.ppid = INIT;
            orphaned_zombie |= matches!(child.state, State::Zombie(_));
        }
        for waiter in [Some(ppid), orphaned_zombie.then_some(INIT)]
            .into_iter()
            .flatten()
        {
            if self.get(waiter).map(|p| p.state) == Some(State::Blocked(Blocked::Wait)) {
                self.wake(waiter);
            }
        }
    }

    /// Takes a zombie child of `parent` out of the table: `Ok(None)` if
    /// there are such children but none has ended, `ECHILD` if there are
    /// none.
    pub fn reap(&mut self, parent: u32, want: Want) -> Result<Option<Process<R>>, Errno> {
        let mine =
            |p: &Process<R>| p.ppid == parent && (want == Want::Any || want == Want::Pid(p.pid));
        if !self.procs.iter().any(mine) {
            return Err(Errno::ECHILD);
        }
        match self
            .procs
            .iter()
            .position(|p| mine(p) && matches!(p.state, State::Zombie(_)))
        {
            Some(i) => Ok(Some(self.procs.remove(i))),
            None => Ok(None),
        }
    }

    /// Marks the process `target` (a pid, or a negated group number) to end
    /// for `reason`, waking whichever of them is blocked. `ESRCH` if none
    /// exists; `EPERM` (and nothing marked) if process 1 is among them. A
    /// zombie counts as existing, and a process marked earlier keeps its
    /// first reason.
    pub fn kill(&mut self, target: i64, reason: u32) -> Result<(), Errno> {
        let hit = |p: &Process<R>| match target {
            t if t > 0 => i64::from(p.pid) == t,
            t if t < 0 => t.checked_neg() == Some(i64::from(p.pgid)),
            _ => false,
        };
        if !self.procs.iter().any(hit) {
            return Err(Errno::ESRCH);
        }
        if self.procs.iter().any(|p| hit(p) && p.pid == INIT) {
            return Err(Errno::EPERM);
        }
        let mut woken = [0u32; MAX];
        let mut n = 0;
        for p in self.procs.iter_mut().filter(|p| hit(p)) {
            if matches!(p.state, State::Zombie(_)) {
                continue;
            }
            p.killed.get_or_insert(reason);
            if matches!(p.state, State::Blocked(_)) {
                woken[n] = p.pid;
                n += 1;
            }
        }
        for &pid in &woken[..n] {
            self.wake(pid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::wait::{KILLED_CTRL_C, KILLED_KILL};

    fn table() -> Table<()> {
        Table::new()
    }

    fn add(t: &mut Table<()>, ppid: u32, new_group: bool) -> u32 {
        t.insert(ppid, new_group, String::from("p"), ()).unwrap()
    }

    fn state(t: &Table<()>, pid: u32) -> State {
        t.get(pid).unwrap().state
    }

    /// The order the processes get the CPU in when each gives it up at
    /// once.
    fn turns(t: &mut Table<()>, n: usize) -> Vec<u32> {
        (0..n).map(|_| t.schedule()).collect()
    }

    #[test]
    fn pids_count_from_1_and_are_not_used_again() {
        let mut t = table();
        assert_eq!(add(&mut t, 0, true), 1);
        assert_eq!(add(&mut t, 1, true), 2);
        t.schedule();
        t.schedule();
        t.end(2, WaitStatus::exited(0));
        assert!(t.reap(1, Want::Pid(2)).unwrap().is_some());
        assert_eq!(add(&mut t, 1, true), 3, "2 is not used again");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn at_most_64_processes_zombies_included() {
        let mut t = table();
        for _ in 0..MAX {
            add(&mut t, 0, true);
        }
        assert_eq!(
            t.insert(1, false, String::from("x"), ()),
            Err(Errno::EAGAIN)
        );
        t.end(64, WaitStatus::exited(0));
        assert_eq!(
            t.insert(1, false, String::from("x"), ()),
            Err(Errno::EAGAIN),
            "a zombie keeps its entry"
        );
        assert!(t.reap(0, Want::Pid(64)).unwrap().is_some());
        assert_eq!(add(&mut t, 1, false), 65);
    }

    #[test]
    fn a_child_joins_its_parent_s_group_or_starts_its_own() {
        let mut t = table();
        let shell = add(&mut t, 0, false);
        assert_eq!(t.get(shell).unwrap().pgid, shell, "no parent: its own");
        let cmd = add(&mut t, shell, true);
        assert_eq!(t.get(cmd).unwrap().pgid, cmd);
        let child = add(&mut t, cmd, false);
        assert_eq!(t.get(child).unwrap().pgid, cmd);
        assert_eq!(t.get(child).unwrap().ppid, cmd);
    }

    #[test]
    fn ready_processes_take_turns_in_order() {
        let mut t = table();
        for _ in 0..3 {
            add(&mut t, 0, true);
        }
        assert_eq!(t.current(), 0, "the idle task runs first");
        assert_eq!(turns(&mut t, 7), [1, 2, 3, 1, 2, 3, 1]);
        assert_eq!(state(&t, 1), State::Running);
        assert_eq!(state(&t, 2), State::Ready);
        // A new process joins the end of the queue.
        add(&mut t, 1, false);
        assert_eq!(turns(&mut t, 4), [2, 3, 4, 1]);
    }

    #[test]
    fn a_blocked_process_waits_until_it_is_woken() {
        let mut t = table();
        add(&mut t, 0, true);
        add(&mut t, 0, true);
        assert_eq!(t.schedule(), 1);
        t.block(Blocked::Console);
        assert_eq!(t.schedule(), 2);
        t.block(Blocked::Wait);
        assert_eq!(t.schedule(), 0, "nothing ready: the idle task");
        assert_eq!(t.schedule(), 0);
        assert!(!t.wake(0), "the idle task is not in the table");
        t.wake_all(Blocked::Console);
        assert_eq!(state(&t, 1), State::Ready);
        assert_eq!(state(&t, 2), State::Blocked(Blocked::Wait));
        assert!(!t.wake(1), "already ready");
        assert_eq!(turns(&mut t, 2), [1, 1], "queued once");
    }

    #[test]
    fn sleepers_wake_when_their_tick_comes() {
        let mut t = table();
        add(&mut t, 0, true);
        add(&mut t, 0, true);
        t.schedule();
        t.block(Blocked::Sleep(100));
        t.schedule();
        t.block(Blocked::Sleep(50));
        t.schedule();
        t.wake_sleepers(49);
        assert_eq!(t.schedule(), 0);
        t.wake_sleepers(100);
        assert_eq!(turns(&mut t, 3), [1, 2, 1]);
    }

    #[test]
    fn a_slice_is_10_ticks_and_ends_only_for_another_process() {
        let mut t = table();
        add(&mut t, 0, true);
        t.schedule();
        for _ in 0..25 {
            assert!(!t.tick(), "nobody else is ready");
        }
        assert_eq!(t.get(1).unwrap().ticks, 25);
        add(&mut t, 0, true);
        // 5 ticks of the slice are gone.
        for _ in 0..4 {
            assert!(!t.tick());
        }
        assert!(t.tick(), "used up");
        assert_eq!(t.schedule(), 2);
        for _ in 0..9 {
            assert!(!t.tick());
        }
        assert!(t.tick());
        assert_eq!(t.get(2).unwrap().ticks, 10);
        // A process that gives up the CPU early leaves the next a whole
        // slice.
        assert_eq!(t.schedule(), 1);
        t.tick();
        assert_eq!(t.schedule(), 2);
        for _ in 0..9 {
            assert!(!t.tick());
        }
        assert!(t.tick());
    }

    #[test]
    fn an_ended_child_is_a_zombie_until_its_parent_waits() {
        let mut t = table();
        let parent = add(&mut t, 0, true);
        let child = add(&mut t, parent, true);
        assert_eq!(t.reap(parent, Want::Any).unwrap().map(|p| p.pid), None);
        assert_eq!(
            t.reap(parent, Want::Pid(child)).unwrap().map(|p| p.pid),
            None
        );
        t.schedule();
        t.block(Blocked::Wait);
        t.schedule();
        t.end(child, WaitStatus::exited(3));
        assert_eq!(state(&t, parent), State::Ready, "woken");
        assert_eq!(state(&t, child), State::Zombie(WaitStatus::exited(3)));
        let reaped = t.reap(parent, Want::Pid(child)).unwrap().unwrap();
        assert_eq!(reaped.state, State::Zombie(WaitStatus::exited(3)));
        assert!(t.get(child).is_none());
        assert_eq!(t.reap(parent, Want::Any).err(), Some(Errno::ECHILD));
    }

    #[test]
    fn wait_is_only_for_one_s_own_children() {
        let mut t = table();
        let a = add(&mut t, 0, true);
        let b = add(&mut t, 0, true);
        let child = add(&mut t, a, true);
        t.end(child, WaitStatus::exited(0));
        assert_eq!(t.reap(b, Want::Pid(child)).err(), Some(Errno::ECHILD));
        assert_eq!(t.reap(b, Want::Any).err(), Some(Errno::ECHILD));
        assert_eq!(t.reap(a, Want::Pid(99)).err(), Some(Errno::ECHILD));
        assert_eq!(t.reap(a, Want::Pid(a)).err(), Some(Errno::ECHILD), "itself");
        assert!(t.reap(a, Want::Any).unwrap().is_some());
    }

    #[test]
    fn a_parent_is_woken_only_while_it_waits() {
        let mut t = table();
        let parent = add(&mut t, 0, true);
        let child = add(&mut t, parent, true);
        t.schedule();
        t.block(Blocked::Console);
        t.schedule();
        t.end(child, WaitStatus::exited(0));
        assert_eq!(state(&t, parent), State::Blocked(Blocked::Console));
    }

    #[test]
    fn orphans_pass_to_process_1_which_is_woken_to_collect_them() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let cmd = add(&mut t, init, true);
        let a = add(&mut t, cmd, false);
        let b = add(&mut t, cmd, false);
        t.end(a, WaitStatus::exited(1));
        t.schedule();
        t.block(Blocked::Wait);
        t.end(cmd, WaitStatus::exited(0));
        assert_eq!(t.get(a).unwrap().ppid, INIT);
        assert_eq!(t.get(b).unwrap().ppid, INIT);
        assert_eq!(state(&t, init), State::Ready);
        let mut got: Vec<u32> = core::iter::from_fn(|| t.reap(INIT, Want::Any).unwrap())
            .map(|p| p.pid)
            .collect();
        got.sort();
        assert_eq!(got, [cmd, a], "b still runs");
        // An orphan ending later wakes process 1 too.
        t.schedule();
        t.block(Blocked::Wait);
        t.end(b, WaitStatus::exited(0));
        assert_eq!(state(&t, init), State::Ready);
        assert!(t.reap(INIT, Want::Pid(b)).unwrap().is_some());
    }

    #[test]
    fn process_1_is_woken_for_a_grandchild_s_zombie() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let x = add(&mut t, init, true);
        let y = add(&mut t, x, false);
        let z = add(&mut t, y, false);
        t.end(z, WaitStatus::exited(0));
        assert_eq!(t.schedule(), init);
        t.block(Blocked::Wait);
        t.end(y, WaitStatus::exited(0));
        assert_eq!(state(&t, init), State::Ready, "z is its orphan now");
        assert_eq!(t.reap(INIT, Want::Any).unwrap().map(|p| p.pid), Some(z));
        assert_eq!(t.reap(x, Want::Any).unwrap().map(|p| p.pid), Some(y));
    }

    #[test]
    fn kill_marks_a_process_or_a_group_and_wakes_the_blocked() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let cmd = add(&mut t, init, true);
        let child = add(&mut t, cmd, false);
        let other = add(&mut t, init, true);
        assert_eq!(turns(&mut t, 2), [init, cmd]);
        t.block(Blocked::Wait);
        t.schedule();
        assert_eq!(t.kill(-i64::from(cmd), KILLED_CTRL_C), Ok(()));
        assert_eq!(t.get(cmd).unwrap().killed, Some(KILLED_CTRL_C));
        assert_eq!(t.get(child).unwrap().killed, Some(KILLED_CTRL_C));
        assert_eq!(t.get(other).unwrap().killed, None);
        assert_eq!(state(&t, cmd), State::Ready, "woken to end");
        // The first reason stays.
        assert_eq!(t.kill(i64::from(child), KILLED_KILL), Ok(()));
        assert_eq!(t.get(child).unwrap().killed, Some(KILLED_CTRL_C));
        assert_eq!(t.kill(i64::from(other), KILLED_KILL), Ok(()));
        assert_eq!(t.get(other).unwrap().killed, Some(KILLED_KILL));
    }

    #[test]
    fn kill_refuses_process_1_and_what_does_not_exist() {
        let mut t = table();
        let init = add(&mut t, 0, false);
        let child = add(&mut t, init, false);
        assert_eq!(t.kill(1, KILLED_KILL), Err(Errno::EPERM));
        assert_eq!(
            t.kill(-1, KILLED_KILL),
            Err(Errno::EPERM),
            "the group of process 1"
        );
        assert_eq!(t.get(child).unwrap().killed, None, "nothing marked");
        for target in [0, 99, -99, i64::MIN, i64::MAX] {
            assert_eq!(t.kill(target, KILLED_KILL), Err(Errno::ESRCH), "{target}");
        }
        let zombie = add(&mut t, init, true);
        t.end(zombie, WaitStatus::exited(0));
        assert_eq!(t.kill(i64::from(zombie), KILLED_KILL), Ok(()), "exists");
        assert_eq!(
            state(&t, zombie),
            State::Zombie(WaitStatus::exited(0)),
            "and keeps how it ended"
        );
        assert_eq!(t.get(zombie).unwrap().killed, None);
    }
}
