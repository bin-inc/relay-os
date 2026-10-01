//! The shell's job commands (user-space gate §9.2, §9.3): `jobs`, and
//! later `wait` and `kill`. They work on the shell's jobs, which only the
//! shell has (`Ctx::control`).

use crate::ctx::Ctx;
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

#[cfg(test)]
mod tests {
    use crate::Shell;
    use crate::testing::Harness;
    use alloc::string::String;
    use relay_abi::WaitStatus;

    /// What an interactive `/bin/sh` prints for the lines typed.
    fn typed(h: &mut Harness, lines: &[&str]) -> String {
        for line in lines {
            h.console.type_in(line.as_bytes());
            h.console.type_in(b"\r");
        }
        Shell::spawning(&mut h.vfs, &mut h.console, &mut h.system, &mut h.programs).run();
        h.console.take()
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
}
