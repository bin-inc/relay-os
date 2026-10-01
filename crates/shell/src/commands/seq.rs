//! `seq [FIRST [INCREMENT]] LAST` (user-space gate §9.1): the whole numbers
//! from FIRST (1) to LAST, INCREMENT (1) apart, one a line, as GNU seq
//! prints them. GNU's also take decimals; these do not, and say so.

use crate::ctx::{Ctx, outln, quote};
use alloc::string::String;
use alloc::vec::Vec;

pub fn seq(ctx: &mut Ctx<'_>, args: &[String]) -> i32 {
    // Numbers may start with `-`: there are no options, only a `--`.
    let ops = match args.split_first() {
        Some((first, rest)) if first == "--" => rest,
        _ => args,
    };
    if let Some(extra) = ops.get(3) {
        return ctx.fail("seq", format_args!("extra operand {}", quote(extra)));
    }
    let mut numbers = Vec::new();
    for op in ops {
        match whole(op) {
            Some(n) => numbers.push(n),
            None => return ctx.fail("seq", format_args!("not a whole number: {}", quote(op))),
        }
    }
    let (first, step, last) = match numbers[..] {
        [] => return ctx.fail("seq", format_args!("missing operand")),
        [last] => (1, 1, last),
        [first, last] => (first, 1, last),
        [first, step, last] => (first, step, last),
        _ => unreachable!("at most three operands"),
    };
    if step == 0 {
        let message = format_args!("invalid Zero increment value: {}", quote(&ops[1]));
        return ctx.fail("seq", message);
    }
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
            ("seq x", "seq: not a whole number: 'x'\n"),
            ("seq 1.5 3", "seq: not a whole number: '1.5'\n"),
            (
                "seq 99999999999999999999",
                "seq: not a whole number: '99999999999999999999'\n",
            ),
            ("seq - 3", "seq: not a whole number: '-'\n"),
        ] {
            assert_eq!(h.run(line), (1, String::from(said)), "{line}");
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
