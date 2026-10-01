//! The shell's job commands (user-space gate §9.2, §9.3): `jobs`, `wait`
//! and `kill`. They work on the shell's jobs, which only the shell has
//! (`Ctx::control`).

use crate::ctx::{Ctx, JobControl};
use crate::jobs::number;
use crate::shell::NAME;
use alloc::string::String;
use alloc::vec::Vec;

/// `jobs [%n | n]...`: the background jobs, by number, or those named; the
/// ones that have ended say how, once, and leave the table.
pub fn jobs(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Some(option) = args.iter().find(|a| a.starts_with('-') && a.len() > 1) {
        ctx.fail(NAME, format_args!("jobs: {option}: invalid option"));
        return 2;
    }
    let Some(control) = ctx.control.as_mut() else {
        return 0;
    };
    control.collect();
    let (named, unknown): (Vec<_>, Vec<_>) = args
        .iter()
        .map(|a| (a, number(a.strip_prefix('%').unwrap_or(a))))
        .partition(|(_, n)| n.is_some_and(|n| control.jobs.has(n)));
    let named: Vec<u32> = named.into_iter().filter_map(|(_, n)| n).collect();
    let lines = if args.is_empty() {
        control.jobs.list(None)
    } else {
        control.jobs.list(Some(&named))
    };
    let status = i32::from(!unknown.is_empty());
    for (a, _) in unknown {
        ctx.fail(NAME, format_args!("jobs: {a}: no such job"));
    }
    for line in lines {
        ctx.out(line.as_bytes());
    }
    status
}

/// `wait` waited for a job: it ended, or a Ctrl-C ended the wait.
enum Waited {
    Ended,
    Interrupted,
}

/// Waits for each of `pids`, a job's processes that have not ended,
/// recording how they end.
fn wait_for(control: &mut JobControl<'_>, pids: &[u32]) -> Waited {
    let Some(programs) = control.programs.as_deref_mut() else {
        return Waited::Ended;
    };
    for &pid in pids {
        match programs.wait_or_ctrl_c(pid) {
            Ok(w) => {
                control.jobs.ended(pid, w);
            }
            Err(vfs::Errno::EINTR) => return Waited::Interrupted,
            // Not the shell's child after all: nothing to wait for.
            Err(_) => {
                let gone = relay_abi::WaitStatus::exited(127);
                control.jobs.ended(pid, gone);
            }
        }
    }
    Waited::Ended
}

/// `wait [%n | PID]...`: waits for every job, or for the jobs and
/// processes named, as bash's does (spec §9.2). With operands the status
/// is the last one's: a job's last process's, or 127 for one that is no
/// job (`%3: no such job`, `pid 9 is not a child of this shell`; a job's
/// process already reported answers once more); without, 0, and the jobs
/// that ended leave the table, those a signal ended saying so. At the
/// prompt a job named that ends says how at once, on the screen. Ctrl-C
/// ends the wait (`^C`, 130); the jobs run on.
pub fn wait(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    if let Some(option) = args.iter().find(|a| a.starts_with('-') && a.len() > 1) {
        ctx.fail(NAME, format_args!("wait: {option}: invalid option"));
        return 2;
    }
    // A process that has ended is collected by the wait for it.
    let Some(control) = ctx.control.as_mut() else {
        return 0;
    };
    if args.is_empty() {
        let pids = control.jobs.all_running();
        let waited = wait_for(control, &pids);
        let report = control.report;
        let killed = control.jobs.report_killed();
        // Notices, as bash's: on the screen, never into a redirection.
        for line in killed.iter().filter(|_| report) {
            ctx.err(line.as_bytes());
        }
        if let Waited::Interrupted = waited {
            ctx.cancelled = true;
        }
        return 0;
    }
    let mut status = 0;
    for a in args {
        let Some(control) = ctx.control.as_mut() else {
            break;
        };
        let job = match (a.strip_prefix('%'), number(a)) {
            (Some(n), _) => match number(n).filter(|&n| control.jobs.has(n)) {
                Some(n) => Some((n, control.jobs.running(n))),
                None => {
                    ctx.fail(NAME, format_args!("wait: {a}: no such job"));
                    status = 127;
                    continue;
                }
            },
            (None, Some(pid)) => match control.jobs.of_pid(pid) {
                Some(n) => {
                    let running = control.jobs.running(n).into_iter().filter(|&p| p == pid);
                    Some((n, running.collect()))
                }
                // A job's process the table let go of: once more.
                None if let Some(gone) = control.jobs.take_gone(pid) => {
                    status = gone;
                    continue;
                }
                None => {
                    ctx.fail(
                        NAME,
                        format_args!("wait: pid {pid} is not a child of this shell"),
                    );
                    status = 127;
                    continue;
                }
            },
            (None, None) => None,
        };
        let Some((n, pids)) = job else {
            ctx.fail(
                NAME,
                format_args!("wait: `{a}': not a pid or valid job spec"),
            );
            status = 1;
            continue;
        };
        if let Waited::Interrupted = wait_for(control, &pids) {
            ctx.cancelled = true;
            return status;
        }
        // A pid's own status, or the job's (its last process's).
        status = match number(a) {
            Some(pid) => control.jobs.status_of_pid(pid),
            None => control.jobs.status(n),
        }
        .unwrap_or(0);
        let report = control.report;
        if let Some(line) = control.jobs.take(n)
            && report
        {
            ctx.err(line.as_bytes());
        }
    }
    status
}

/// Linux's signal names, which `kill` knows but for `KILL` refuses: only
/// killing exists here (spec §15).
const SIGNALS: &[&str] = &[
    "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE", "KILL", "USR1", "SEGV", "USR2",
    "PIPE", "ALRM", "TERM", "STKFLT", "CHLD", "CONT", "STOP", "TSTP", "TTIN", "TTOU", "URG",
    "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "IO", "PWR", "SYS",
];

/// What `kill` makes of a signal it was given: `Ok` for killing, else
/// the message.
fn signal(spec: &str) -> Result<(), &'static str> {
    let upper = spec.to_ascii_uppercase();
    let name = upper.strip_prefix("SIG").unwrap_or(&upper);
    match (name, number(name)) {
        ("KILL", _) | (_, Some(9)) => Ok(()),
        (_, Some(1..=64)) => Err("not supported"),
        (n, None) if SIGNALS.contains(&n) => Err("not supported"),
        _ => Err("invalid signal specification"),
    }
}

/// `kill [-9 | -KILL | -s KILL] PID | %n...` (spec §9.3): kills each
/// process, or each job's whole process group, as bash's does with
/// SIGKILL, its messages bash's (`(1) - Operation not permitted`, `(9) -
/// No such process`, `%3: no such job`); the status is 1 if any was not
/// killed. Killing is the only signal here: another is not supported.
pub fn kill(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    let mut targets = args;
    loop {
        match targets {
            [s, spec, rest @ ..] if s == "-s" => {
                if let Err(why) = signal(spec) {
                    return ctx.fail(NAME, format_args!("kill: {spec}: {why}"));
                }
                targets = rest;
            }
            [s] if s == "-s" => {
                return ctx.fail(NAME, format_args!("kill: -s: option requires an argument"));
            }
            [dashes, rest @ ..] if dashes == "--" => {
                targets = rest;
                break;
            }
            [spec, rest @ ..] if spec.len() > 1 && spec.starts_with('-') => {
                if let Err(why) = signal(&spec[1..]) {
                    return ctx.fail(NAME, format_args!("kill: {}: {why}", &spec[1..]));
                }
                targets = rest;
            }
            _ => break,
        }
    }
    if targets.is_empty() {
        ctx.fail(
            NAME,
            format_args!("kill: usage: kill [-s KILL | -KILL] pid | %job ..."),
        );
        return 2;
    }
    let mut status = 0;
    for t in targets {
        let target = match t.strip_prefix('%') {
            Some(n) => {
                let pgid = ctx
                    .control
                    .as_ref()
                    .and_then(|c| number(n).and_then(|n| c.jobs.pgid(n)));
                match pgid {
                    Some(pgid) => -i64::from(pgid),
                    None => {
                        status = ctx.fail(NAME, format_args!("kill: {t}: no such job"));
                        continue;
                    }
                }
            }
            None => match number(t) {
                Some(pid) => i64::from(pid),
                None => {
                    status = ctx.fail(
                        NAME,
                        format_args!("kill: {t}: arguments must be process or job IDs"),
                    );
                    continue;
                }
            },
        };
        let programs = ctx.control.as_mut().and_then(|c| c.programs.as_deref_mut());
        let killed = programs.map_or(Err(vfs::Errno::ESRCH), |p| p.kill(target));
        if let Err(e) = killed {
            status = ctx.fail(
                NAME,
                format_args!("kill: ({}) - {e}", target.unsigned_abs()),
            );
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use crate::Shell;
    use crate::testing::Harness;
    use alloc::string::String;
    use relay_abi::WaitStatus;

    /// What an interactive `/bin/sh` prints for the lines typed.
    fn typed(h: &mut Harness, lines: &[&str]) -> String {
        typed_status(h, lines).1
    }

    /// The last line's status, and what the shell printed.
    fn typed_status(h: &mut Harness, lines: &[&str]) -> (i32, String) {
        for line in lines {
            h.console.type_in(line.as_bytes());
            h.console.type_in(b"\r");
        }
        let mut shell = Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs);
        shell.run();
        let status = shell.status();
        (status, h.console.take())
    }

    /// `t-spin` runs on through 99 rounds of collecting (a round each
    /// prompt, typed line and `jobs`), `sleep` through 1.
    fn with_jobs() -> Harness {
        let mut h = Harness::new();
        for (p, life) in [("/bin/t-spin", 99), ("/bin/sleep", 1)] {
            h.programs.known.push((p, WaitStatus::exited(0)));
            h.programs.lives.push((p, life));
        }
        h
    }

    #[test]
    fn jobs_lists_them_and_says_once_how_the_ended_ones_ended() {
        let mut h = with_jobs();
        // sleep ends after the prompt before `jobs`, which reports it.
        let out = typed(
            &mut h,
            &["t-spin &", "sleep 5 &", "jobs", "jobs", "jobs > /tmp/j"],
        );
        assert_eq!(
            out,
            "root@relay:/# t-spin &\n[1] 101\n\
             root@relay:/# sleep 5 &\n[2] 102\n\
             root@relay:/# jobs\n\
             [1]-  Running                 t-spin &\n\
             [2]+  Done                    sleep 5\n\
             root@relay:/# jobs\n\
             [1]+  Running                 t-spin &\n\
             root@relay:/# jobs > /tmp/j\n\
             root@relay:/# "
        );
        assert_eq!(h.get("/tmp/j"), b"[1]+  Running                 t-spin &\n");
    }

    #[test]
    fn a_job_jobs_does_not_name_is_reported_at_the_prompt() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["t-spin &", "sleep 5 &", "jobs %1", ""]);
        assert!(
            out.ends_with(
                "# jobs %1\n[1]-  Running                 t-spin &\n\
                 [2]+  Done                    sleep 5\nroot@relay:/# \nroot@relay:/# "
            ),
            "{out}"
        );
    }

    #[test]
    fn jobs_names_jobs_as_bash_s_does() {
        let mut h = with_jobs();
        let out = typed(
            &mut h,
            &[
                "t-spin &",
                "t-spin 2 &",
                "jobs %1 2",
                "jobs 2",
                "jobs %3 x %0",
                "jobs -l",
            ],
        );
        assert!(
            out.contains(
                "# jobs %1 2\n[1]-  Running                 t-spin &\n[2]+  Running                 t-spin 2 &\n"
            ),
            "{out}"
        );
        assert!(
            out.contains("# jobs 2\n[2]+  Running                 t-spin 2 &\nroot"),
            "{out}"
        );
        // In the order named, a job named twice listed twice (bash 5.2).
        let ordered = typed(
            &mut h,
            &["t-spin &", "t-spin 2 &", "jobs 2 %1", "jobs %1 %1"],
        );
        assert!(
            ordered.contains(
                "# jobs 2 %1\n[2]+  Running                 t-spin 2 &\n[1]-  Running                 t-spin &\n"
            ),
            "{ordered}"
        );
        assert!(
            ordered.contains(
                "# jobs %1 %1\n[1]-  Running                 t-spin &\n[1]-  Running                 t-spin &\n"
            ),
            "{ordered}"
        );
        assert!(
            out.contains(
                "# jobs %3 x %0\nrelay-sh: jobs: %3: no such job\nrelay-sh: jobs: x: no such job\nrelay-sh: jobs: %0: no such job\n"
            ),
            "{out}"
        );
        assert!(
            out.contains("# jobs -l\nrelay-sh: jobs: -l: invalid option\n"),
            "{out}"
        );
        assert_eq!(
            h.spawning("jobs %3"),
            (1, "relay-sh: jobs: %3: no such job\n".into())
        );
        assert_eq!(
            h.spawning("jobs -l"),
            (2, "relay-sh: jobs: -l: invalid option\n".into())
        );
        // A shell without programs has no jobs.
        assert_eq!(h.run("jobs"), (0, String::new()));
    }

    #[test]
    fn wait_waits_for_every_job_and_says_nothing_of_them() {
        let mut h = with_jobs();
        let (status, out) = typed_status(&mut h, &["t-spin &", "t-spin 2 &", "wait", "jobs"]);
        assert!(
            out.ends_with("root@relay:/# wait\nroot@relay:/# jobs\nroot@relay:/# "),
            "{out}"
        );
        assert_eq!(status, 0);
        assert_eq!(h.programs.waited, [101, 102]);
        assert!(h.programs.children().is_empty());
    }

    #[test]
    fn wait_for_a_job_or_a_pid_says_how_it_ended_and_takes_its_status() {
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        h.programs.lives.push(("/bin/false", 9));
        let (status, out) = typed_status(
            &mut h,
            &[
                "false | t-spin &",
                "false &",
                "wait %2",
                "wait 101",
                "wait 102",
                "jobs",
            ],
        );
        assert!(
            out.contains("# wait %2\n[2]+  Exit 1                  false\nroot@relay:/# "),
            "{out}"
        );
        // A pid of a job: its own status; the job says how it ended once
        // its last process has.
        assert!(
            out.contains(
                "# wait 101\nroot@relay:/# wait 102\n[1]+  Done                    false | t-spin\n"
            ),
            "{out}"
        );
        assert!(out.ends_with("# jobs\nroot@relay:/# "), "{out}");
        assert_eq!(status, 0, "jobs's");
        assert_eq!(
            typed_status(&mut h, &["false &", "wait %1"]).0,
            1,
            "the job's"
        );
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        assert_eq!(
            typed_status(&mut h, &["false | t-spin &", "wait 101"]).0,
            1,
            "the pid's"
        );
    }

    #[test]
    fn a_script_s_wait_says_nothing_of_the_job() {
        let mut h = with_jobs();
        h.put("/tmp/s.sh", b"t-spin &\nwait %1\njobs\n");
        let mut out = crate::testing::FakeStdout::console();
        assert_eq!(
            h.sh(&["/tmp/s.sh"], &mut out),
            (0, "+ t-spin &\n+ wait %1\n+ jobs\n".into())
        );
        assert_eq!(h.programs.waited, [101]);
    }

    #[test]
    fn ctrl_c_ends_a_wait_and_the_jobs_run_on() {
        let mut h = with_jobs();
        h.programs.ctrl_c_after = Some(1);
        let (status, out) = typed_status(&mut h, &["t-spin &", "t-spin 2 &", "wait", "jobs"]);
        assert!(
            out.contains(
                "# wait\n^C\nroot@relay:/# jobs\n[2]+  Running                 t-spin 2 &\n"
            ),
            "{out}"
        );
        assert_eq!(h.programs.waited, [101], "the first, then the Ctrl-C");
        assert_eq!(status, 0, "jobs's");
        h.programs.ctrl_c_after = Some(0);
        assert_eq!(typed_status(&mut h, &["t-spin &", "wait %1"]).0, 130);
        h.programs.ctrl_c_after = Some(0);
        assert_eq!(typed_status(&mut h, &["t-spin &", "wait"]).0, 130);
    }

    #[test]
    fn wait_refuses_what_is_no_job_of_its_as_bash_s_does() {
        let mut h = with_jobs();
        for (line, status, said) in [
            ("wait %3", 127, "wait: %3: no such job"),
            (
                "wait 999",
                127,
                "wait: pid 999 is not a child of this shell",
            ),
            ("wait abc", 1, "wait: `abc': not a pid or valid job spec"),
            ("wait %x", 127, "wait: %x: no such job"),
            ("wait -n", 2, "wait: -n: invalid option"),
        ] {
            assert_eq!(
                h.spawning(line),
                (status, alloc::format!("relay-sh: {said}\n")),
                "{line}"
            );
        }
        assert_eq!(
            h.run("wait"),
            (0, String::new()),
            "no jobs without programs"
        );
        assert_eq!(h.run("wait %1").0, 127);
    }

    #[test]
    fn a_bare_wait_tells_of_a_killed_job_on_the_screen() {
        // bash 5.2: `kill -9 %1; wait` says the job was killed; a job that
        // exited it does not mention.
        let mut h = with_jobs();
        h.programs.known.push((
            "/bin/t-fault",
            WaitStatus::killed(relay_abi::wait::KILLED_KILL),
        ));
        h.programs.lives.push(("/bin/t-fault", 99));
        let out = typed(&mut h, &["t-fault x &", "t-spin &", "wait > /tmp/w"]);
        assert!(
            out.ends_with(
                "# wait > /tmp/w\n[1]-  Killed                  t-fault x\nroot@relay:/# "
            ),
            "{out}"
        );
        assert_eq!(h.get("/tmp/w"), b"", "never into the redirection");
    }

    #[test]
    fn wait_for_a_job_says_how_it_ended_on_the_screen() {
        let mut h = with_jobs();
        let out = typed(&mut h, &["t-spin &", "wait %1 > /tmp/w"]);
        assert!(
            out.ends_with(
                "# wait %1 > /tmp/w\n[1]+  Done                    t-spin\nroot@relay:/# "
            ),
            "{out}"
        );
        assert_eq!(h.get("/tmp/w"), b"");
    }

    #[test]
    fn wait_for_a_reported_job_s_pid_gives_its_status_once() {
        // bash 5.2: `false &`, Enter (`Exit 1`), then `wait $!` is 1.
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        let (status, out) = typed_status(&mut h, &["false &", "", "wait 101"]);
        assert!(out.ends_with("# wait 101\nroot@relay:/# "), "{out}");
        assert_eq!(status, 1);
        let mut h = with_jobs();
        h.programs.known.push(("/bin/false", WaitStatus::exited(1)));
        let (status, out) = typed_status(&mut h, &["false &", "", "wait 101", "wait 101"]);
        assert!(
            out.ends_with("relay-sh: wait: pid 101 is not a child of this shell\nroot@relay:/# "),
            "{out}"
        );
        assert_eq!(status, 127, "once");
    }

    #[test]
    fn kill_ends_a_job_s_whole_group_or_a_process() {
        let mut h = with_jobs();
        let out = typed(
            &mut h,
            &[
                "t-spin | t-spin &",
                "t-spin 3 &",
                "kill %1",
                "",
                "kill -9 103",
                "",
            ],
        );
        assert!(
            out.contains("# kill %1\n[1]-  Killed                  t-spin | t-spin\n"),
            "{out}"
        );
        assert!(
            out.contains("# kill -9 103\n[2]+  Killed                  t-spin 3\n"),
            "{out}"
        );
        assert_eq!(h.programs.kills, [-101, 103], "the group, then the process");
        assert!(h.programs.children().is_empty());
    }

    #[test]
    fn kill_says_what_it_could_not_kill_as_bash_s_does() {
        let mut h = with_jobs();
        let (status, out) = typed_status(&mut h, &["t-spin &", "kill 1 999 %2 %x abc 0x 101"]);
        assert!(
            out.contains(
                "relay-sh: kill: (1) - Operation not permitted\n\
                 relay-sh: kill: (999) - No such process\n\
                 relay-sh: kill: %2: no such job\n\
                 relay-sh: kill: %x: no such job\n\
                 relay-sh: kill: abc: arguments must be process or job IDs\n\
                 relay-sh: kill: 0x: arguments must be process or job IDs\n"
            ),
            "{out}"
        );
        assert_eq!(status, 1, "though 101 was killed");
        assert_eq!(h.programs.kills, [1, 999, 101]);
        assert_eq!(
            h.spawning("kill"),
            (
                2,
                "relay-sh: kill: usage: kill [-s KILL | -KILL] pid | %job ...\n".into()
            )
        );
        assert_eq!(h.spawning("kill -9").0, 2);
        // bash kills its own group with 0; the kernel's 0 names nothing.
        assert_eq!(
            h.spawning("kill 0"),
            (1, "relay-sh: kill: (0) - No such process\n".into())
        );
        // A shell without programs kills nothing.
        assert_eq!(
            h.run("kill 5"),
            (1, "relay-sh: kill: (5) - No such process\n".into())
        );
    }

    #[test]
    fn killing_is_the_only_signal() {
        let mut h = with_jobs();
        for line in [
            "kill -9 5",
            "kill -KILL 5",
            "kill -kill 5",
            "kill -SIGKILL 5",
            "kill -s KILL 5",
            "kill -s 9 -- 5",
        ] {
            h.programs.kills.clear();
            assert_eq!(
                h.spawning(line),
                (1, "relay-sh: kill: (5) - No such process\n".into()),
                "{line}"
            );
            assert_eq!(h.programs.kills, [5], "{line}");
        }
        h.programs.kills.clear();
        for (line, said) in [
            ("kill -TERM 5", "kill: TERM: not supported"),
            ("kill -15 5", "kill: 15: not supported"),
            ("kill -s HUP 5", "kill: HUP: not supported"),
            ("kill -FOO 5", "kill: FOO: invalid signal specification"),
            ("kill -99 5", "kill: 99: invalid signal specification"),
            ("kill -s", "kill: -s: option requires an argument"),
        ] {
            assert_eq!(
                h.spawning(line),
                (1, alloc::format!("relay-sh: {said}\n")),
                "{line}"
            );
        }
        assert!(h.programs.kills.is_empty(), "nothing killed");
        assert_eq!(
            h.spawning("kill -- -5"),
            (
                1,
                "relay-sh: kill: -5: arguments must be process or job IDs\n".into()
            )
        );
    }

    #[test]
    fn kill_of_a_job_that_has_ended_says_it_is_no_process_as_bash_s_does() {
        // bash 5.2: `true &`, a moment, `kill %1`: `(pid) - No such
        // process`, status 1, then the Done line. The job ended while the
        // line was typed, and is collected before it runs.
        let mut h = with_jobs();
        let (status, out) = typed_status(&mut h, &["sleep 1 &", "kill %1"]);
        assert!(
            out.ends_with(
                "# kill %1\nrelay-sh: kill: (101) - No such process\n\
                 [1]+  Done                    sleep 1\nroot@relay:/# "
            ),
            "{out}"
        );
        assert_eq!(status, 1);
    }
}
