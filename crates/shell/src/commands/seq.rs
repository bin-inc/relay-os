//! `seq [FIRST [INCREMENT]] LAST` (user-space gate §9.1): the whole numbers
//! from FIRST (1) to LAST, INCREMENT (1) apart, one a line, as GNU seq
//! prints them. GNU's also takes decimals, `inf`, hexadecimal and numbers
//! beyond 64 bits, and the options `-f`, `-s` and `-w`; this one says it
//! does not. What GNU's refuses, it refuses with GNU's first line.

use crate::ctx::{Ctx, outln, quote};
use alloc::string::String;
use alloc::vec::Vec;

pub fn seq(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    // Options come before the first operand, as GNU's; a number may start
    // with `-`.
    let ops = match args.split_first() {
        Some((first, rest)) if first == "--" => rest,
        Some((first, _)) if first.starts_with("--") => {
            return ctx.fail("seq", format_args!("unrecognized option {}", quote(first)));
        }
        Some((first, _)) if is_option(first) => {
            let c = first[1..].chars().next().unwrap_or('-');
            return ctx.fail("seq", format_args!("invalid option -- '{c}'"));
        }
        _ => args,
    };
    if let Some(extra) = ops.get(3) {
        return ctx.fail("seq", format_args!("extra operand {}", quote(extra)));
    }
    // GNU's refusals first, in its order: each operand must be a number,
    // and the increment of three not zero, which GNU checks as soon as it
    // has read it.
    for (i, op) in ops.iter().enumerate() {
        if is_nan(op) {
            let message = format_args!("invalid 'not-a-number' argument: {}", quote(op));
            return ctx.fail("seq", message);
        }
        if !is_number(op) {
            let message = format_args!("invalid floating point argument: {}", quote(op));
            return ctx.fail("seq", message);
        }
        if i == 1 && ops.len() == 3 && is_zero(op) {
            let message = format_args!("invalid Zero increment value: {}", quote(op));
            return ctx.fail("seq", message);
        }
    }
    // Then this seq's own: whole numbers only.
    let mut numbers = Vec::new();
    for op in ops {
        match whole(op) {
            Some(n) => numbers.push(n),
            None => {
                return ctx.fail("seq", format_args!("not a whole number: {}", quote(op)));
            }
        }
    }
    let (first, step, last) = match numbers[..] {
        [] => return ctx.fail("seq", format_args!("missing operand")),
        [last] => (1, 1, last),
        [first, last] => (first, 1, last),
        [first, step, last] => (first, step, last),
        _ => unreachable!("at most three operands"),
    };
    let mut n = first;
    while !ctx.interrupted() && !ctx.out_failed() {
        if (step > 0 && n > last) || (step < 0 && n < last) {
            break;
        }
        outln!(ctx, "{n}");
        match n.checked_add(step) {
            Some(next) => n = next,
            None => break,
        }
    }
    0
}

/// An option: `-` and something that cannot start a number.
fn is_option(s: &str) -> bool {
    s.strip_prefix('-')
        .and_then(|rest| rest.bytes().next())
        .is_some_and(|b| !b.is_ascii_digit() && b != b'.')
}

/// A number GNU's seq takes: decimal with a fraction or an exponent,
/// hexadecimal, or `inf`, each with a sign or none (leading blanks allowed).
fn is_number(s: &str) -> bool {
    let s = s.trim_start_matches([' ', '\t']);
    let s = s.strip_prefix(['+', '-']).unwrap_or(s).to_ascii_lowercase();
    if s == "inf" || s == "infinity" {
        return true;
    }
    let (digits, exponent_mark): (&str, char) = match s.strip_prefix("0x") {
        Some(hex) => (hex, 'p'),
        None => (&s, 'e'),
    };
    let is_digit = |c: char| {
        if exponent_mark == 'p' {
            c.is_ascii_hexdigit()
        } else {
            c.is_ascii_digit()
        }
    };
    let (mantissa, exponent) = match digits.split_once(exponent_mark) {
        Some((m, e)) => (m, Some(e)),
        None => (digits, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mantissa_ok = !(whole.is_empty() && fraction.is_empty())
        && whole.chars().all(is_digit)
        && fraction.chars().all(is_digit);
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.bytes().all(|b| b.is_ascii_digit())
    });
    mantissa_ok && exponent_ok
}

/// Whether a number (one `is_number` takes) is zero: all its digits
/// before any exponent are `0`.
fn is_zero(s: &str) -> bool {
    let s = s.trim_start_matches([' ', '\t']);
    let s = s.strip_prefix(['+', '-']).unwrap_or(s).to_ascii_lowercase();
    let (digits, exponent_mark) = match s.strip_prefix("0x") {
        Some(hex) => (hex, 'p'),
        None => (s.as_str(), 'e'),
    };
    let mantissa = digits.split(exponent_mark).next().unwrap_or("");
    mantissa.chars().all(|c| c == '0' || c == '.')
}

/// GNU's not-a-number (`nan`, either case, with a sign or none).
fn is_nan(s: &str) -> bool {
    let s = s.trim_start_matches([' ', '\t']);
    s.strip_prefix(['+', '-'])
        .unwrap_or(s)
        .eq_ignore_ascii_case("nan")
}

/// A whole number: a sign or none, then digits (leading blanks allowed,
/// as GNU's); `None` for anything else, or beyond 64 bits.
fn whole(s: &str) -> Option<i64> {
    let s = s.trim_start_matches([' ', '\t']);
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.strip_prefix('+').unwrap_or(s).parse().ok()
}

#[cfg(test)]
mod tests {
    use crate::testing::{Harness, host_tool};
    use alloc::string::String;

    #[test]
    fn seq_prints_what_gnu_seq_prints() {
        let cases: &[&[&str]] = &[
            &["seq", "5"],
            &["seq", "3", "5"],
            &["seq", "5", "3"],
            &["seq", "-2", "2"],
            &["seq", "1", "2", "9"],
            &["seq", "1", "2", "10"],
            &["seq", "10", "-3", "1"],
            // Only the increment may not be zero.
            &["seq", "0", "2", "6"],
            &["seq", "3", "-1", "0"],
            &["seq", "-2", "0"],
            &["seq", "0"],
            &["seq", "-3", "-1"],
            &["seq", "--", "5"],
            &["seq", "+3"],
            &["seq", " 4"],
            &["seq", "007"],
            &["seq", "9223372036854775806", "9223372036854775807"],
            &[
                "seq",
                "-9223372036854775807",
                "-1000000000000000000",
                "-9223372036854775808",
            ],
            &["seq", "1000"],
        ];
        for args in cases {
            let mut h = Harness::new();
            assert_eq!(
                h.like_host(args, &[], b""),
                host_tool(args, &[], b""),
                "{args:?}"
            );
        }
    }

    #[test]
    fn what_seq_refuses() {
        let mut h = Harness::new();
        // A seq that printed for ever would meet a Ctrl-C and fail the test.
        h.console.interrupt_after = Some(10_000);
        // GNU's first line; GNU also takes decimals, these do not.
        for (line, said) in [
            ("seq", "seq: missing operand\n"),
            ("seq 1 2 3 4", "seq: extra operand '4'\n"),
            ("seq 1 0 5", "seq: invalid Zero increment value: '0'\n"),
            ("seq 1.5 3", "seq: not a whole number: '1.5'\n"),
            (
                "seq 99999999999999999999",
                "seq: not a whole number: '99999999999999999999'\n",
            ),
            ("seq 1e3", "seq: not a whole number: '1e3'\n"),
            ("seq -.5 1", "seq: not a whole number: '-.5'\n"),
            ("seq inf", "seq: not a whole number: 'inf'\n"),
            ("seq 0x10", "seq: not a whole number: '0x10'\n"),
            // A hexadecimal increment's `e` is a digit (14), not zero.
            ("seq 1 0x0e 5", "seq: not a whole number: '0x0e'\n"),
            // GNU's options are not this seq's.
            ("seq -w 3", "seq: invalid option -- 'w'\n"),
        ] {
            assert_eq!(h.run(line), (1, String::from(said)), "{line}");
        }
    }

    /// What GNU seq refuses, it refuses with its first line here too.
    #[test]
    fn seq_refuses_what_gnu_seq_refuses_as_it_does() {
        for args in [
            &["seq", "x"][..],
            &["seq", "1", "x"],
            &["seq", "-"],
            &["seq", "+"],
            &["seq", "."],
            &["seq", "1e"],
            &["seq", "1", "-x"],
            &["seq", "-x", "3"],
            &["seq", "--foo", "3"],
            &["seq", "nan"],
            // GNU refuses a zero increment as soon as it has read it,
            // before the last operand, in any form a number takes.
            &["seq", "1", "0", "x"],
            &["seq", "1", "0", "1.5"],
            &["seq", "1", "0", "nan"],
            &["seq", "1.5", "0", "x"],
            &["seq", "0x10", "0", "x"],
            &["seq", "1", "0.0", "5"],
            &["seq", "1", "-0", "5"],
            &["seq", "1", " +0", "5"],
            &["seq", "1", ".0", "5"],
            &["seq", "1", "0x0", "5"],
            &["seq", "1", "0x.0p3", "5"],
            &["seq", "1", "0e5", "x"],
            &["seq", "x", "0", "1"],
            &["seq", "nan", "0", "1"],
            // And every operand is a number before this seq asks for a
            // whole one.
            &["seq", "1.5", "x"],
            &["seq", "1e3", "nan"],
        ] {
            let mut h = Harness::new();
            // A zero increment taken would print for ever: a Ctrl-C ends
            // it, and the test fails.
            h.console.interrupt_after = Some(10_000);
            let (status, out, err) = host_tool(args, &[], b"");
            let first = err.lines().next().unwrap_or("");
            assert_eq!(
                h.like_host(args, &[], b""),
                (status, out, alloc::format!("{first}\n")),
                "{args:?}"
            );
        }
    }

    #[test]
    fn ctrl_c_and_a_write_error_stop_seq() {
        let mut h = Harness::new();
        h.console.interrupt_after = Some(3);
        assert_eq!(
            h.run("seq 9223372036854775807"),
            (130, "1\n2\n3\n^C\n".into())
        );
        let mut h = Harness::with_capacity(5 * 4096);
        assert_eq!(
            h.run("seq 9223372036854775807 > /tmp/o"),
            (1, "seq: write error: No space left on device\n".into())
        );
    }
}
