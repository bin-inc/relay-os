//! The shell's background jobs (user-space gate §9.2, §16 item 9): what
//! `cmd &` started, numbered and reported as bash does.
//!
//! - A new job is numbered one past the highest in the table, so numbers
//!   start again from 1 only once the table is empty.
//! - The newest job is the current one, marked `+`, the one before it `-`.
//! - A job has ended when every process it started has; its status is its
//!   last one's: `Done`, `Exit 3`, or the words bash has for the signal
//!   Linux would have sent (`Killed`, `Segmentation fault`, …).
//! - A line is bash's: `[1]+  Done                    sleep 5`, the state
//!   padded to 24 columns, a running job's text followed by ` &`.

use crate::io::Programs;
use crate::killed;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use relay_abi::WaitStatus;
use relay_abi::wait::EXITED;

/// One job: the processes a background line started.
struct Job {
    number: u32,
    /// Its process group, which `kill %n` kills.
    pgid: u32,
    /// The line as typed, without the `&`.
    text: String,
    /// Its processes, in the order they started, and how each ended.
    procs: Vec<(u32, Option<WaitStatus>)>,
}

impl Job {
    fn finished(&self) -> bool {
        self.procs.iter().all(|(_, w)| w.is_some())
    }

    /// How its last process ended, once it has.
    fn last(&self) -> Option<WaitStatus> {
        self.procs.last().and_then(|(_, w)| *w)
    }
}

/// The background jobs, by number.
#[derive(Default)]
pub struct Jobs {
    jobs: Vec<Job>,
    /// How the processes of the jobs that left the table ended, the newest
    /// last, at most `GONE` of them: `wait PID` answers for each once, as
    /// bash's does.
    gone: Vec<(u32, WaitStatus)>,
}

/// The processes of jobs gone from the table whose statuses are kept.
const GONE: usize = relay_abi::proc::PROC_MAX;

impl Jobs {
    pub fn new() -> Jobs {
        Jobs::default()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Adds the job of process group `pgid`, whose processes are `pids`
    /// (at least one; the last is the one `$!` and the status come from);
    /// its number.
    pub fn add(&mut self, pgid: u32, pids: &[u32], text: &str) -> u32 {
        let number = self.jobs.last().map_or(1, |j| j.number + 1);
        self.jobs.push(Job {
            number,
            pgid,
            text: String::from(text),
            procs: pids.iter().map(|&p| (p, None)).collect(),
        });
        number
    }

    /// Collects what has ended of the jobs' processes from `programs`, at
    /// most as many as can exist (`relay_abi::proc::PROC_MAX`).
    pub fn collect(&mut self, programs: &mut dyn Programs) {
        if self.jobs.is_empty() {
            return;
        }
        for _ in 0..relay_abi::proc::PROC_MAX {
            let Some((pid, status)) = programs.collect() else {
                break;
            };
            self.ended(pid, status);
        }
    }

    /// Records how `pid` ended; whether it was a job's.
    pub fn ended(&mut self, pid: u32, status: WaitStatus) -> bool {
        for job in &mut self.jobs {
            if let Some(p) = job.procs.iter_mut().find(|(p, w)| *p == pid && w.is_none()) {
                p.1 = Some(status);
                return true;
            }
        }
        false
    }

    /// The job a job spec names, its `%` taken off (bash's): a number;
    /// nothing, `%` or `+` for the current job, the newest; `-` for the
    /// previous one, the one before it, or the newest when it is alone.
    pub fn spec(&self, spec: &str) -> Option<u32> {
        let newest = |k: usize| self.jobs.len().checked_sub(k).map(|i| self.jobs[i].number);
        match spec {
            "" | "%" | "+" => newest(1),
            "-" => newest(2).or_else(|| newest(1)),
            n => number(n).filter(|&n| self.has(n)),
        }
    }

    /// Whether job `number` is in the table.
    pub fn has(&self, number: u32) -> bool {
        self.find(number).is_some()
    }

    /// The job that has `pid` among its processes.
    pub fn of_pid(&self, pid: u32) -> Option<u32> {
        self.jobs
            .iter()
            .find(|j| j.procs.iter().any(|&(p, _)| p == pid))
            .map(|j| j.number)
    }

    /// How `pid`, a job's process, ended, once it has.
    pub fn status_of_pid(&self, pid: u32) -> Option<i32> {
        self.jobs
            .iter()
            .flat_map(|j| &j.procs)
            .find(|&&(p, _)| p == pid)
            .and_then(|(_, w)| w.as_ref().map(status_of))
    }

    /// Job `number`'s process group.
    pub fn pgid(&self, number: u32) -> Option<u32> {
        self.find(number).map(|j| j.pgid)
    }

    /// Job `number`'s processes that have not ended, in order.
    pub fn running(&self, number: u32) -> Vec<u32> {
        self.find(number).map_or(Vec::new(), |j| {
            j.procs
                .iter()
                .filter(|(_, w)| w.is_none())
                .map(|&(p, _)| p)
                .collect()
        })
    }

    /// Every job's processes that have not ended.
    pub fn all_running(&self) -> Vec<u32> {
        self.jobs
            .iter()
            .flat_map(|j| self.running(j.number))
            .collect()
    }

    /// The exit status job `number` ended with, as bash gives it (its last
    /// process's), once it has ended.
    pub fn status(&self, number: u32) -> Option<i32> {
        let job = self.find(number).filter(|j| j.finished())?;
        job.last().map(|w| status_of(&w))
    }

    /// The lines `jobs` prints: of every job by number, or of those named
    /// in the order named, a job named twice listed twice, as bash's;
    /// the jobs listed that have ended are reported there, and leave the
    /// table.
    pub fn list(&mut self, named: Option<&[u32]>) -> Vec<String> {
        let order: Vec<usize> = match named {
            None => (0..self.jobs.len()).collect(),
            Some(n) => n
                .iter()
                .filter_map(|&k| self.jobs.iter().position(|j| j.number == k))
                .collect(),
        };
        let lines = order.iter().map(|&i| self.line(i)).collect();
        let listed = |j: &Job| named.is_none_or(|n| n.contains(&j.number));
        self.drop_jobs(|j| listed(j) && j.finished());
        lines
    }

    /// The lines for the jobs that have ended, which leave the table (what
    /// the shell says before a prompt).
    pub fn report(&mut self) -> Vec<String> {
        let lines = (0..self.jobs.len())
            .filter(|&i| self.jobs[i].finished())
            .map(|i| self.line(i))
            .collect();
        self.drop_jobs(Job::finished);
        lines
    }

    /// Job `number`'s line if it has ended, as `report` gives it; it
    /// leaves the table (`wait %n` reports it at once).
    pub fn take(&mut self, number: u32) -> Option<String> {
        let i = self
            .jobs
            .iter()
            .position(|j| j.number == number && j.finished())?;
        let line = self.line(i);
        self.drop_jobs(|j| j.number == number);
        Some(line)
    }

    /// The jobs that have ended leave the table, and the lines of those a
    /// signal ended are given: `wait` without operands says nothing of a
    /// job that exited, as bash's does, but tells of one that was killed.
    pub fn report_killed(&mut self) -> Vec<String> {
        let killed = |j: &Job| j.finished() && j.last().is_some_and(|w| w.how != EXITED);
        let lines = (0..self.jobs.len())
            .filter(|&i| killed(&self.jobs[i]))
            .map(|i| self.line(i))
            .collect();
        self.drop_jobs(Job::finished);
        lines
    }

    /// The status `pid`, a process of a job gone from the table, ended
    /// with; asked once (bash's `wait $!` after the job was reported).
    pub fn take_gone(&mut self, pid: u32) -> Option<i32> {
        let i = self.gone.iter().position(|&(p, _)| p == pid)?;
        Some(status_of(&self.gone.remove(i).1))
    }

    /// The jobs `which` names leave the table; their processes' statuses
    /// are kept, the oldest forgotten beyond `GONE`.
    fn drop_jobs(&mut self, which: impl Fn(&Job) -> bool) {
        for job in self.jobs.iter().filter(|j| which(j)) {
            for &(pid, w) in &job.procs {
                if let Some(w) = w {
                    self.gone.push((pid, w));
                }
            }
        }
        let over = self.gone.len().saturating_sub(GONE);
        self.gone.drain(..over);
        self.jobs.retain(|j| !which(j));
    }

    fn find(&self, number: u32) -> Option<&Job> {
        self.jobs.iter().find(|j| j.number == number)
    }

    /// The line of the job at `i`: `+` for the newest, `-` for the one
    /// before.
    fn line(&self, i: usize) -> String {
        let job = &self.jobs[i];
        let mark = match self.jobs.len() - i {
            1 => '+',
            2 => '-',
            _ => ' ',
        };
        let (state, amp) = match job.last() {
            _ if !job.finished() => (String::from("Running"), " &"),
            Some(w) if w.how == EXITED && w.code == 0 => (String::from("Done"), ""),
            Some(w) if w.how == EXITED => (format!("Exit {}", w.code), ""),
            Some(w) => (String::from(signal_words(status_of(&w))), ""),
            None => (String::from("Killed"), ""),
        };
        // bash pads to 24 and no further, so its `Floating point
        // exception` meets the text; here a blank stays between.
        format!("[{}]{mark}  {state:<23} {}{amp}\n", job.number, job.text)
    }
}

/// bash's words for a job its signal ended, by the status the signal
/// gives (`killed::killed`'s): SIGSEGV for a page fault or a protection
/// fault, SIGILL, SIGFPE, SIGINT, and SIGKILL for `kill` or anything else.
fn signal_words(status: i32) -> &'static str {
    match status {
        139 => "Segmentation fault",
        132 => "Illegal instruction",
        136 => "Floating point exception",
        130 => "Interrupt",
        _ => "Killed",
    }
}

/// The job number `n` names (`1`, `27`), as `%n` does: decimal digits only
/// (no job is 0).
pub fn number(n: &str) -> Option<u32> {
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}

/// The status bash gives for a process that ended so.
pub fn status_of(w: &WaitStatus) -> i32 {
    if w.how == EXITED {
        w.code as i32
    } else {
        killed::killed(w).1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_abi::wait::{ACCESS_READ, FAULT_PAGE, KILLED_KILL};

    fn killed() -> WaitStatus {
        WaitStatus::killed(KILLED_KILL)
    }

    #[test]
    fn jobs_are_numbered_one_past_the_highest_as_bash_numbers_them() {
        let mut j = Jobs::new();
        assert_eq!(j.add(10, &[10], "sleep 30"), 1);
        assert_eq!(j.add(11, &[11], "sleep 30"), 2);
        assert_eq!(j.add(12, &[12], "sleep 30"), 3);
        // bash 5.2: with 1, 2 and 3 started, 1 and 2 killed and reported,
        // the next two are 4 and 5, not 1 and 2.
        for pid in [10, 11] {
            assert!(j.ended(pid, killed()));
        }
        assert_eq!(j.report().len(), 2);
        assert_eq!(j.add(13, &[13], "sleep 30"), 4);
        assert_eq!(j.add(14, &[14], "sleep 30"), 5);
        for pid in [12, 13, 14] {
            j.ended(pid, WaitStatus::exited(0));
        }
        j.report();
        assert!(j.is_empty());
        assert_eq!(j.add(15, &[15], "true"), 1, "an empty table starts again");
    }

    #[test]
    fn the_lines_are_bash_s() {
        // What bash 5.2 printed for the same jobs, in a terminal.
        let mut j = Jobs::new();
        j.add(20, &[20], "sleep 0.5");
        j.add(21, &[21, 22], "sleep 30 | cat");
        j.add(23, &[23], "true");
        j.ended(23, WaitStatus::exited(0));
        assert_eq!(
            j.list(None),
            [
                "[1]   Running                 sleep 0.5 &\n",
                "[2]-  Running                 sleep 30 | cat &\n",
                "[3]+  Done                    true\n",
            ]
        );
        assert_eq!(j.list(None).len(), 2, "a finished job is listed once");
        j.ended(20, WaitStatus::exited(0));
        assert_eq!(j.report(), ["[1]-  Done                    sleep 0.5\n"]);
        j.add(24, &[24], "false");
        j.ended(24, WaitStatus::exited(1));
        assert_eq!(j.report(), ["[3]+  Exit 1                  false\n"]);
        j.ended(21, killed());
        assert_eq!(j.report(), [] as [String; 0], "cat still runs");
        j.ended(22, killed());
        assert_eq!(
            j.report(),
            ["[2]+  Killed                  sleep 30 | cat\n"]
        );
        assert!(j.report().is_empty());
    }

    #[test]
    fn a_pipeline_s_status_is_its_last_process_s() {
        let mut j = Jobs::new();
        let n = j.add(30, &[30, 31], "false | true");
        j.ended(30, WaitStatus::exited(1));
        assert_eq!(j.status(n), None, "not ended yet");
        assert_eq!(j.running(n), [31]);
        j.ended(31, WaitStatus::exited(0));
        assert_eq!(j.status(n), Some(0));
        assert_eq!(
            j.take(n).unwrap(),
            "[1]+  Done                    false | true\n"
        );
        assert!(!j.has(n));
        let n = j.add(32, &[32, 33], "true | false");
        j.ended(33, WaitStatus::exited(1));
        j.ended(32, WaitStatus::exited(0));
        assert_eq!(j.status(n), Some(1));
        assert_eq!(j.report(), ["[1]+  Exit 1                  true | false\n"]);
        // A fault's or a kill's status is bash's for its signal.
        let n = j.add(34, &[34], "t-fault null-read");
        j.ended(34, WaitStatus::fault(FAULT_PAGE, ACCESS_READ, 0, 0x40_1000));
        assert_eq!(j.status(n), Some(139));
        assert_eq!(
            j.take(n).unwrap(),
            "[1]+  Segmentation fault      t-fault null-read\n"
        );
        let n = j.add(35, &[35], "t-spin");
        j.ended(35, killed());
        assert_eq!(j.status(n), Some(137));
    }

    #[test]
    fn a_job_is_found_by_its_processes() {
        let mut j = Jobs::new();
        let a = j.add(40, &[40, 41], "a | b");
        let b = j.add(42, &[42], "c");
        assert_eq!(
            (j.of_pid(41), j.of_pid(42), j.of_pid(7)),
            (Some(a), Some(b), None)
        );
        assert_eq!(
            (j.pgid(a), j.pgid(b), j.pgid(9)),
            (Some(40), Some(42), None)
        );
        assert_eq!(j.all_running(), [40, 41, 42]);
        assert!(!j.ended(7, WaitStatus::exited(0)), "no job's");
        assert!(j.ended(41, WaitStatus::exited(0)));
        assert!(!j.ended(41, WaitStatus::exited(0)), "once");
        assert_eq!(j.all_running(), [40, 42]);
        assert_eq!(j.take(a), None, "40 still runs");
        j.ended(40, WaitStatus::exited(0));
        assert!(j.report_killed().is_empty(), "it exited");
        assert!(!j.has(a) && j.has(b));
    }

    #[test]
    fn a_job_spec_names_a_number_or_the_current_or_previous_job() {
        let mut j = Jobs::new();
        assert_eq!(j.spec("%"), None, "no job at all");
        j.add(70, &[70], "a");
        assert_eq!((j.spec(""), j.spec("-")), (Some(1), Some(1)), "alone, both");
        j.add(71, &[71], "b");
        j.add(72, &[72], "c");
        for (spec, number) in [("", 3), ("%", 3), ("+", 3), ("-", 2), ("1", 1), ("3", 3)] {
            assert_eq!(j.spec(spec), Some(number), "{spec:?}");
        }
        for spec in ["4", "0", "x", "-1", "++"] {
            assert_eq!(j.spec(spec), None, "{spec:?}");
        }
    }

    #[test]
    fn a_job_s_processes_answer_once_after_it_left_the_table() {
        let mut j = Jobs::new();
        let a = j.add(60, &[60, 61], "false | true");
        j.ended(60, WaitStatus::exited(1));
        j.ended(61, WaitStatus::exited(0));
        assert_eq!(j.report().len(), 1);
        assert!(!j.has(a));
        assert_eq!((j.take_gone(60), j.take_gone(61)), (Some(1), Some(0)));
        assert_eq!(j.take_gone(60), None, "once");
        // At most the table's 64 are kept, the oldest forgotten.
        for pid in 100..170 {
            let n = j.add(pid, &[pid], "true");
            j.ended(pid, WaitStatus::exited(0));
            assert!(j.take(n).is_some());
        }
        assert_eq!((j.take_gone(105), j.take_gone(106)), (None, Some(0)));
        // A killed job is told of by `wait`, an exited one not.
        j.add(200, &[200], "t-spin");
        j.add(201, &[201], "sleep 5");
        j.ended(200, WaitStatus::killed(relay_abi::wait::KILLED_KILL));
        j.ended(201, WaitStatus::exited(0));
        assert_eq!(
            j.report_killed(),
            ["[1]-  Killed                  t-spin\n"]
        );
        assert!(j.is_empty());
    }

    #[test]
    fn a_job_a_fault_ended_says_bash_s_words_for_its_signal() {
        use relay_abi::wait::{
            FAULT_DIVIDE, FAULT_FPU, FAULT_GENERAL_PROTECTION, FAULT_INVALID_OPCODE,
            FAULT_STACK_OVERFLOW,
        };
        // What bash 5.2 printed for jobs SIGSEGV, SIGILL and SIGFPE ended
        // (without its `(core dumped)`, which nothing here does).
        for (fault, words) in [
            (FAULT_PAGE, "Segmentation fault      "),
            (FAULT_GENERAL_PROTECTION, "Segmentation fault      "),
            (FAULT_STACK_OVERFLOW, "Segmentation fault      "),
            (FAULT_INVALID_OPCODE, "Illegal instruction     "),
            (FAULT_DIVIDE, "Floating point exception "),
            (FAULT_FPU, "Floating point exception "),
        ] {
            let mut j = Jobs::new();
            let n = j.add(50, &[50], "t-fault x");
            j.ended(50, WaitStatus::fault(fault, ACCESS_READ, 0, 0x40_1000));
            assert_eq!(
                j.take(n).unwrap(),
                alloc::format!("[1]+  {words}t-fault x\n"),
                "{fault}"
            );
        }
        let mut j = Jobs::new();
        let n = j.add(51, &[51], "t-spin");
        j.ended(51, WaitStatus::killed(relay_abi::wait::KILLED_CTRL_C));
        assert_eq!(j.take(n).unwrap(), "[1]+  Interrupt               t-spin\n");
    }
}
