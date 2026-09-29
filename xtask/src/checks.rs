//! Checks a script's transcript against the output the script expects
//! (spec §15 item 12). The NUC's hardware checks are relay-sh scripts under
//! `/root/checks/`; `sh` writes what each shows into a transcript, and
//! `cargo xtask verify-usb` (on the stick) and the e2e step `check-script`
//! (in QEMU) check it here.
//!
//! Under each command, the script says what it must print:
//!
//! ```text
//! cat /root/notes/a
//! #> remember me          each `#>` line: a regex for one whole output line,
//! #> and me               in order
//! dmesg
//! #> ...                  any number of lines
//! #nuc> \[ ok \] usb: .*  only on the NUC (`#qemu>`: only in QEMU)
//! #> ...
//! #!> \[FAIL\].*          no output line may match
//! mkdir /root/x           nothing expected: it must print nothing
//! ```
//!
//! A command whose only expectations are `#!>` lines may print anything
//! else. Any other `#word>` is an error (a typo must not drop an
//! expectation); other `#` lines are comments.

use crate::e2e::strip_ansi;
use anyhow::{Context, Result, bail};
use regex::Regex;

/// Where a transcript was written: some output differs between the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    Qemu,
    Nuc,
}

#[derive(Debug)]
enum Expect {
    /// One whole output line.
    Line(Regex),
    /// Any number of lines.
    Any,
    /// No output line matches.
    Never(Regex),
}

/// One command of a script and what it must print.
#[derive(Debug)]
pub struct Command {
    /// Line number in the script.
    pub line: usize,
    /// The command as `sh` shows it, after `+ `.
    pub text: String,
    expect: Vec<Expect>,
}

/// Reads a script: its commands, each with the expectations that follow it
/// and apply on `machine`.
pub fn parse(script: &str, machine: Machine) -> Result<Vec<Command>> {
    let mut commands: Vec<Command> = Vec::new();
    for (i, raw) in script.lines().enumerate() {
        let line = raw.trim();
        let n = i + 1;
        if line.is_empty() {
            continue;
        }
        let Some(comment) = line.strip_prefix('#') else {
            commands.push(Command {
                line: n,
                text: line.to_string(),
                expect: Vec::new(),
            });
            continue;
        };
        let Some((tag, pattern)) = comment.split_once('>') else {
            continue;
        };
        let (on, never) = match tag {
            "" => (true, false),
            "!" => (true, true),
            "nuc" => (machine == Machine::Nuc, false),
            "qemu" => (machine == Machine::Qemu, false),
            // `#word>` looks like an expectation: a typo must not turn one
            // into a comment, or a wrong transcript could pass.
            t if t.chars().all(|c| c.is_ascii_alphabetic() || c == '!') => {
                bail!("line {n}: `#{t}>` is not an expectation (`#>`, `#!>`, `#nuc>`, `#qemu>`)")
            }
            _ => continue,
        };
        let command = commands
            .last_mut()
            .with_context(|| format!("line {n}: an expectation before any command"))?;
        if !on {
            continue;
        }
        let pattern = pattern.strip_prefix(' ').unwrap_or(pattern);
        let whole = || {
            Regex::new(&format!("^(?:{pattern})$")).with_context(|| format!("line {n}: bad regex"))
        };
        command.expect.push(match (never, pattern) {
            (false, "...") => Expect::Any,
            (false, _) => Expect::Line(whole()?),
            (true, _) => Expect::Never(whole()?),
        });
    }
    if commands.is_empty() {
        bail!("the script has no commands");
    }
    Ok(commands)
}

/// How a transcript compares with its script.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub commands: usize,
    pub passed: usize,
    /// One entry per command that printed something else, then one if the
    /// transcript ended early or went astray.
    pub failures: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Compares `transcript` (what `sh` wrote) with `commands`.
pub fn check(commands: &[Command], transcript: &str) -> Report {
    let text = strip_ansi(transcript);
    let lines: Vec<&str> = text.lines().collect();
    let mut report = Report {
        commands: commands.len(),
        passed: 0,
        failures: Vec::new(),
    };
    let mut at = 0;
    for (i, cmd) in commands.iter().enumerate() {
        let trace = format!("+ {}", cmd.text);
        match lines.get(at) {
            None => {
                report.failures.push(missing(commands, i));
                return report;
            }
            Some(l) if *l != trace => {
                report.failures.push(format!(
                    "line {}: expected `{trace}` in the transcript, found `{l}`",
                    cmd.line
                ));
                return report;
            }
            Some(_) => at += 1,
        }
        // The output runs to the next command's trace, or to the end.
        let next = commands.get(i + 1).map(|c| format!("+ {}", c.text));
        let found = next
            .as_ref()
            .and_then(|n| lines[at..].iter().position(|l| l == n));
        let end = found.map_or(lines.len(), |k| at + k);
        match mismatch(&cmd.expect, &lines[at..end]) {
            None => report.passed += 1,
            Some(why) => report
                .failures
                .push(format!("line {}: `{}`: {why}", cmd.line, cmd.text)),
        }
        // Without the next trace, the next command finds the end.
        at = end;
    }
    report
}

/// The failure for command `i` and those after it not being in the
/// transcript (the machine hung or restarted, or the script changed).
fn missing(commands: &[Command], i: usize) -> String {
    let cmd = &commands[i];
    format!(
        "line {}: `{}` and the {} command(s) after it are not in the transcript",
        cmd.line,
        cmd.text,
        commands.len() - i - 1
    )
}

/// Why `output` does not meet `expect`, if it does not.
fn mismatch(expect: &[Expect], output: &[&str]) -> Option<String> {
    for e in expect {
        if let Expect::Never(re) = e
            && let Some(l) = output.iter().find(|l| re.is_match(l))
        {
            return Some(format!("printed `{l}`, which matches /{}/", show(re)));
        }
    }
    let mut lines: Vec<&Expect> = expect
        .iter()
        .filter(|e| !matches!(e, Expect::Never(_)))
        .collect();
    if lines.is_empty() && expect.iter().any(|e| matches!(e, Expect::Never(_))) {
        lines.push(&Expect::Any);
    }
    line_mismatch(&lines, output)
}

/// The pattern as written in the script.
fn show(re: &Regex) -> &str {
    let s = re.as_str();
    &s[4..s.len() - 2]
}

/// Whether the patterns match all of `output`, line by line, `...`
/// matching any number of lines; if not, the first pattern that cannot
/// match and where. Dynamic programming over (pattern, line), so many
/// `...` cost no more than one.
fn line_mismatch(patterns: &[&Expect], output: &[&str]) -> Option<String> {
    // ok[j]: the patterns so far match output[..j].
    let mut ok = vec![false; output.len() + 1];
    ok[0] = true;
    for p in patterns {
        let mut next = vec![false; output.len() + 1];
        match p {
            Expect::Any => {
                let mut reached = false;
                for j in 0..=output.len() {
                    reached |= ok[j];
                    next[j] = reached;
                }
            }
            Expect::Line(re) => {
                for j in 0..output.len() {
                    next[j + 1] = ok[j] && re.is_match(output[j]);
                }
            }
            Expect::Never(_) => next = ok.clone(),
        }
        if let Expect::Line(re) = p
            && !next.contains(&true)
        {
            // The lines where this pattern could have matched.
            let from: Vec<usize> = (0..=output.len()).filter(|&j| ok[j]).collect();
            return Some(match from[..] {
                [j] if j == output.len() => {
                    format!("expected /{}/, but nothing more was printed", show(re))
                }
                [j] => format!(
                    "expected /{}/, printed `{}` (line {})",
                    show(re),
                    output[j],
                    j + 1
                ),
                _ => format!(
                    "expected /{}/ on a line from line {} on, but none matches",
                    show(re),
                    from[0] + 1
                ),
            });
        }
        ok = next;
    }
    let last = (0..=output.len()).rev().find(|&j| ok[j])?;
    (last < output.len()).then(|| {
        format!(
            "expected nothing more, printed `{}` (line {})",
            output[last],
            last + 1
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "\
# A check.
mkdir -p /root/n
cat /root/n/a
#> remember me
#> and \\w+
dmesg
#> ...
#nuc> usb: 00:14\\.0 port 15: .*Kingston.*
#qemu> usb: 00:02\\.0 port 2: .*QEMU HARDDISK.*
#> ...
#!> \\[FAIL\\].*
ls /
#!> .*lost\\+found.*
";

    fn run(machine: Machine, transcript: &str) -> Report {
        check(&parse(SCRIPT, machine).unwrap(), transcript)
    }

    const QEMU_LOG: &str = "\
+ mkdir -p /root/n
+ cat /root/n/a
remember me
and me
+ dmesg
[ ok ] pci: 3 devices
usb: 00:02.0 port 2: 46f4:0001 SuperSpeed, disk QEMU QEMU HARDDISK, 256 MiB
[ ok ] mount /
+ ls /
bin  etc
";

    #[test]
    fn a_transcript_that_matches_passes() {
        let r = run(Machine::Qemu, QEMU_LOG);
        assert_eq!((r.commands, r.passed), (4, 4), "{:?}", r.failures);
        assert!(r.ok());
    }

    #[test]
    fn lines_tagged_for_the_other_machine_do_not_count() {
        let r = run(Machine::Nuc, QEMU_LOG);
        assert_eq!(r.passed, 3);
        assert_eq!(r.failures.len(), 1);
        assert_eq!(
            r.failures[0],
            "line 6: `dmesg`: expected /usb: 00:14\\.0 port 15: .*Kingston.*/ \
             on a line from line 1 on, but none matches"
        );
    }

    #[test]
    fn every_output_line_must_be_expected() {
        // A command with nothing expected prints nothing: an error fails it.
        let log = QEMU_LOG.replace(
            "+ mkdir -p /root/n\n",
            "+ mkdir -p /root/n\nmkdir: cannot create directory '/root/n': Read-only file system\n",
        );
        let r = run(Machine::Qemu, &log);
        assert_eq!(r.passed, 3);
        assert_eq!(
            r.failures,
            [
                "line 2: `mkdir -p /root/n`: expected nothing more, printed \
                 `mkdir: cannot create directory '/root/n': Read-only file system` (line 1)"
            ]
        );
        // Lines are whole lines, in order, and none may be left over.
        for bad in [
            "remember me\nand me\nextra\n",
            "and me\nremember me\n",
            "remember me!\nand me\n",
            "remember me\n",
        ] {
            let log = QEMU_LOG.replace("remember me\nand me\n", bad);
            assert_eq!(run(Machine::Qemu, &log).passed, 3, "{bad:?}");
        }
    }

    #[test]
    fn a_line_that_must_not_appear_fails_the_command() {
        let log = QEMU_LOG.replace("[ ok ] mount /", "[FAIL] mount /: no USB disk");
        let r = run(Machine::Qemu, &log);
        assert_eq!(
            r.failures,
            [
                "line 6: `dmesg`: printed `[FAIL] mount /: no USB disk`, which matches /\\[FAIL\\].*/"
            ]
        );
        // Alone, `#!>` lines allow any other output.
        let log = QEMU_LOG.replace("bin  etc", "bin  lost+found");
        assert_eq!(run(Machine::Qemu, &log).passed, 3);
    }

    #[test]
    fn a_transcript_that_ends_early_or_goes_astray() {
        let log = &QEMU_LOG[..QEMU_LOG.find("+ dmesg").unwrap()];
        let r = run(Machine::Qemu, log);
        assert_eq!(r.passed, 2);
        assert_eq!(
            r.failures,
            ["line 6: `dmesg` and the 1 command(s) after it are not in the transcript"]
        );
        let r = run(Machine::Qemu, &QEMU_LOG.replace("+ ls /", "+ ls /root"));
        assert_eq!(
            r.failures,
            ["line 12: `ls /` and the 0 command(s) after it are not in the transcript"]
        );
        assert_eq!(
            run(Machine::Qemu, "").failures,
            ["line 2: `mkdir -p /root/n` and the 3 command(s) after it are not in the transcript"]
        );
        assert_eq!(
            run(Machine::Qemu, "+ mkdir /root/n\n").failures,
            ["line 2: expected `+ mkdir -p /root/n` in the transcript, found `+ mkdir /root/n`"]
        );
    }

    #[test]
    fn a_failure_names_the_expectation_and_the_line() {
        // A long output (dmesg) that is wrong at its 30th line.
        let script: String = core::iter::once("dmesg\n".to_string())
            .chain((1..=40).map(|i| format!("#> l{i}\n")))
            .collect();
        let mut log: Vec<String> = (1..=40).map(|i| format!("l{i}")).collect();
        log[29] = "oops".into();
        let c = parse(&script, Machine::Qemu).unwrap();
        let r = check(&c, &format!("+ dmesg\n{}\n", log.join("\n")));
        assert_eq!(
            r.failures,
            ["line 1: `dmesg`: expected /l30/, printed `oops` (line 30)"]
        );
        // After `...`, the line may be anywhere further on.
        let c = parse("dmesg\n#> l1\n#> ...\n#> l99\n", Machine::Qemu).unwrap();
        let r = check(&c, &format!("+ dmesg\n{}\n", log.join("\n")));
        assert_eq!(
            r.failures,
            ["line 1: `dmesg`: expected /l99/ on a line from line 2 on, but none matches"]
        );
        // Output that ends too soon.
        let c = parse("ls\n#> a\n#> b\n", Machine::Qemu).unwrap();
        assert_eq!(
            check(&c, "+ ls\na\n").failures,
            ["line 1: `ls`: expected /b/, but nothing more was printed"]
        );
    }

    /// The real check scripts against a transcript of each machine: QEMU's
    /// from the `checks` scenario, the NUC's as the NUC wrote them in the
    /// full hardware checklist of 2026-09-29 (`docs/hardware-test.md`),
    /// copied off the stick, with the lines of `check3-a.sh`'s `system`
    /// startup line and `ls -l /bin` put in by hand until the next NUC check
    /// records them (milestone 2, plan 1). A `#nuc>` line must not need a line
    /// of its own next to the `#>` line for the same output, which QEMU
    /// alone cannot show.
    #[test]
    fn the_check_scripts_pass_on_both_machines() {
        let parts = [
            (
                include_str!("../../rootfs/root/checks/check3-a.sh"),
                include_str!("../fixtures/checks/check3-a.qemu.log"),
                include_str!("../fixtures/checks/check3-a.nuc.log"),
            ),
            (
                include_str!("../../rootfs/root/checks/check3-b.sh"),
                include_str!("../fixtures/checks/check3-b.qemu.log"),
                include_str!("../fixtures/checks/check3-b.nuc.log"),
            ),
        ];
        for (i, (script, qemu, nuc)) in parts.iter().enumerate() {
            for (machine, log) in [(Machine::Qemu, qemu), (Machine::Nuc, nuc)] {
                let r = check(&parse(script, machine).unwrap(), log);
                assert!(r.ok(), "part {i} on {machine:?}: {:?}", r.failures);
            }
            // The NUC's own lines are checked there: QEMU's transcript is
            // not the NUC's.
            let r = check(&parse(script, Machine::Nuc).unwrap(), qemu);
            assert!(!r.ok(), "part {i}");
        }
    }

    #[test]
    fn many_wildcards_stay_fast() {
        let script = format!("dmesg\n{}#> never\n", "#> ...\n".repeat(40));
        let log = format!("+ dmesg\n{}", "x\n".repeat(2000));
        let r = check(&parse(&script, Machine::Qemu).unwrap(), &log);
        assert_eq!(r.passed, 0);
    }

    #[test]
    fn colour_codes_and_carriage_returns_are_ignored() {
        let log = QEMU_LOG.replace("remember me\n", "\x1b[1mremember me\x1b[0m\r\n");
        assert!(run(Machine::Qemu, &log).ok());
    }

    #[test]
    fn script_errors() {
        assert!(parse("#> x\nls\n", Machine::Qemu).is_err());
        assert!(parse("ls\n#> (\n", Machine::Qemu).is_err());
        assert!(parse("# only a comment\n", Machine::Qemu).is_err());
        // Comments, including ones with `>`, are not expectations.
        let c = parse("ls\n# see > there\n# a `#> regex` line\n", Machine::Qemu).unwrap();
        assert_eq!(c.len(), 1);
        assert!(c[0].expect.is_empty());
    }

    #[test]
    fn a_mistyped_expectation_tag_is_an_error_not_a_comment() {
        // A dropped `#!>` or `#nuc>` line would let a wrong transcript pass.
        for tag in ["Nuc", "NUC", "quemu", "other", "!!", "nuc!"] {
            let err = parse(&format!("ls\n#{tag}> x\n"), Machine::Nuc).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("line 2: `#{tag}>` is not an expectation (`#>`, `#!>`, `#nuc>`, `#qemu>`)")
            );
        }
    }
}
