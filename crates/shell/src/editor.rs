//! The line editor (spec §7.3): printable characters go in at the cursor,
//! Backspace and Delete remove, ←/→, Home/End, Ctrl-A/Ctrl-E move, ↑/↓
//! step through the last 50 lines, Ctrl-C cancels, Ctrl-L clears the
//! screen.
//!
//! It is a pure state machine: [`LineEditor::feed`] takes one input byte
//! and appends what the screen needs to `out`. Arrow keys arrive as the
//! escape sequences a terminal sends (`ESC [ A` …). A line longer than the
//! screen wraps; the editor knows the width and moves the cursor between
//! rows with relative moves (`ESC [ n A`, `\r`, `ESC [ n C`), which both
//! the kernel's terminal and a host terminal understand. `\n` in `out`
//! means "start of the next line" (the console does the carriage return).

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// How many lines the history keeps.
pub const HISTORY: usize = 50;

/// What a byte of input produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feed {
    /// Still editing.
    Pending,
    /// Enter: the finished line.
    Line(String),
    /// Ctrl-C: the line was dropped.
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Escape {
    None,
    /// After `ESC`.
    Start,
    /// After `ESC [` or `ESC O`: the first parameter so far, and whether
    /// a `;` has ended it (modifier parameters such as the `5` of Ctrl+→,
    /// `ESC [ 1 ; 5 C`, are ignored).
    Sequence {
        number: u16,
        ended: bool,
    },
}

pub struct LineEditor {
    prompt: String,
    prompt_width: usize,
    columns: usize,
    buf: Vec<u8>,
    pos: usize,
    /// The row (from the prompt's first row) the cursor is on.
    cursor_row: usize,
    history: VecDeque<String>,
    /// While browsing: the history entry shown, and the line being typed
    /// before browsing started.
    browsing: Option<(usize, Vec<u8>)>,
    escape: Escape,
    after_cr: bool,
}

impl Default for LineEditor {
    fn default() -> LineEditor {
        LineEditor::new()
    }
}

impl LineEditor {
    pub fn new() -> LineEditor {
        LineEditor {
            prompt: String::new(),
            prompt_width: 0,
            columns: 80,
            buf: Vec::new(),
            pos: 0,
            cursor_row: 0,
            history: VecDeque::new(),
            browsing: None,
            escape: Escape::None,
            after_cr: false,
        }
    }

    /// Starts a new line: prints `prompt` on a screen `columns` wide.
    pub fn start(&mut self, prompt: &str, columns: usize, out: &mut Vec<u8>) {
        self.prompt = String::from(prompt);
        self.prompt_width = prompt.chars().count();
        self.columns = columns.max(1);
        self.buf.clear();
        self.pos = 0;
        self.browsing = None;
        self.escape = Escape::None;
        out.extend_from_slice(prompt.as_bytes());
        self.cursor_row = self.wrap_at_end(self.prompt_width, out);
    }

    /// The history, oldest first.
    pub fn history(&self) -> impl Iterator<Item = &str> {
        self.history.iter().map(String::as_str)
    }

    pub fn feed(&mut self, byte: u8, out: &mut Vec<u8>) -> Feed {
        let after_cr = core::mem::replace(&mut self.after_cr, false);
        match self.escape {
            Escape::Start => {
                self.escape = Escape::None;
                if matches!(byte, b'[' | b'O') {
                    self.escape = Escape::Sequence {
                        number: 0,
                        ended: false,
                    };
                    return Feed::Pending;
                }
                // A lone Esc: the byte after it is an ordinary key.
            }
            Escape::Sequence { number, ended } => {
                match byte {
                    b'0'..=b'9' if !ended => {
                        let number = number
                            .saturating_mul(10)
                            .saturating_add((byte - b'0') as u16);
                        self.escape = Escape::Sequence { number, ended };
                    }
                    b'0'..=b'9' => {}
                    b';' => {
                        self.escape = Escape::Sequence {
                            number,
                            ended: true,
                        }
                    }
                    0x40..=0x7E => {
                        self.escape = Escape::None;
                        self.sequence(byte, number, out);
                    }
                    _ => self.escape = Escape::None,
                }
                return Feed::Pending;
            }
            Escape::None => {}
        }
        match byte {
            0x1B => self.escape = Escape::Start,
            b'\r' => {
                self.after_cr = true;
                return self.enter(out);
            }
            // A terminal may send CR LF for Enter; the LF is not a second line.
            b'\n' if after_cr => {}
            b'\n' => return self.enter(out),
            0x03 => {
                self.move_to_end(out);
                out.extend_from_slice(b"^C\n");
                self.buf.clear();
                self.pos = 0;
                self.browsing = None;
                return Feed::Cancelled;
            }
            0x01 => self.move_to(0, out),
            0x05 => self.move_to(self.buf.len(), out),
            0x0C => {
                out.extend_from_slice(b"\x1b[H\x1b[2J");
                self.cursor_row = 0;
                self.redraw(out);
            }
            0x08 | 0x7F if self.pos > 0 => {
                self.buf.remove(self.pos - 1);
                self.pos -= 1;
                self.redraw(out);
            }
            0x20..=0x7E => self.insert(byte, out),
            _ => {}
        }
        Feed::Pending
    }

    /// The final byte of `ESC [ <number> <byte>` (or `ESC O <byte>`).
    fn sequence(&mut self, byte: u8, number: u16, out: &mut Vec<u8>) {
        match (byte, number) {
            (b'A', _) => self.history_step(true, out),
            (b'B', _) => self.history_step(false, out),
            (b'C', _) if self.pos < self.buf.len() => self.move_to(self.pos + 1, out),
            (b'D', _) if self.pos > 0 => self.move_to(self.pos - 1, out),
            (b'H', _) | (b'~', 1 | 7) => self.move_to(0, out),
            (b'F', _) | (b'~', 4 | 8) => self.move_to(self.buf.len(), out),
            (b'~', 3) if self.pos < self.buf.len() => {
                self.buf.remove(self.pos);
                self.redraw(out);
            }
            _ => {}
        }
    }

    fn insert(&mut self, byte: u8, out: &mut Vec<u8>) {
        self.buf.insert(self.pos, byte);
        self.pos += 1;
        if self.pos == self.buf.len() {
            // Typing at the end, the common case: just echo the byte.
            out.push(byte);
            self.cursor_row = self.wrap_at_end(self.prompt_width + self.pos, out);
        } else {
            self.redraw(out);
        }
    }

    fn enter(&mut self, out: &mut Vec<u8>) -> Feed {
        self.move_to_end(out);
        out.push(b'\n');
        let line = String::from_utf8_lossy(&self.buf).into_owned();
        if !line.trim().is_empty() && self.history.back() != Some(&line) {
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(line.clone());
        }
        self.buf.clear();
        self.pos = 0;
        self.browsing = None;
        Feed::Line(line)
    }

    fn history_step(&mut self, back: bool, out: &mut Vec<u8>) {
        let len = self.history.len();
        let next = match (&self.browsing, back) {
            (None, true) if len > 0 => Some(len - 1),
            (Some((i, _)), true) => Some(i.saturating_sub(1)),
            (Some((i, _)), false) if i + 1 < len => Some(i + 1),
            (Some(_), false) => None,
            (None, _) => return,
        };
        match next {
            Some(i) => {
                let draft = match self.browsing.take() {
                    Some((_, draft)) => draft,
                    None => core::mem::take(&mut self.buf),
                };
                self.buf = self.history[i].as_bytes().to_vec();
                self.browsing = Some((i, draft));
            }
            None => {
                self.buf = self.browsing.take().map(|(_, d)| d).unwrap_or_default();
            }
        }
        self.pos = self.buf.len();
        self.redraw(out);
    }

    /// Moves the cursor to buffer position `pos`.
    fn move_to(&mut self, pos: usize, out: &mut Vec<u8>) {
        self.pos = pos;
        self.place_cursor(self.cursor_row, out);
    }

    /// Moves the cursor after the last character, so what follows the line
    /// starts below all of it.
    fn move_to_end(&mut self, out: &mut Vec<u8>) {
        if self.pos != self.buf.len() {
            self.move_to(self.buf.len(), out);
        }
    }

    /// Rewrites the prompt and the line from the prompt's first row, then
    /// puts the cursor back at `pos`.
    fn redraw(&mut self, out: &mut Vec<u8>) {
        if self.cursor_row > 0 {
            let _ = write!(Bytes(out), "\x1b[{}A", self.cursor_row);
        }
        out.extend_from_slice(b"\r\x1b[J");
        out.extend_from_slice(self.prompt.as_bytes());
        out.extend_from_slice(&self.buf);
        let end_row = self.wrap_at_end(self.prompt_width + self.buf.len(), out);
        self.place_cursor(end_row, out);
    }

    /// After writing up to screen position `end`: when that is exactly the
    /// end of a row the terminal has not wrapped yet, so wrap explicitly.
    /// Returns the cursor's row.
    fn wrap_at_end(&self, end: usize, out: &mut Vec<u8>) -> usize {
        if end > 0 && end.is_multiple_of(self.columns) {
            out.push(b'\n');
        }
        end / self.columns
    }

    /// Moves the cursor from row `from_row` to where `pos` is on screen.
    fn place_cursor(&mut self, from_row: usize, out: &mut Vec<u8>) {
        let at = self.prompt_width + self.pos;
        let (row, col) = (at / self.columns, at % self.columns);
        let mut out = Bytes(out);
        if row < from_row {
            let _ = write!(out, "\x1b[{}A", from_row - row);
        } else if row > from_row {
            let _ = write!(out, "\x1b[{}B", row - from_row);
        }
        out.0.push(b'\r');
        if col > 0 {
            let _ = write!(out, "\x1b[{col}C");
        }
        self.cursor_row = row;
    }
}

/// Lets `write!` append to a byte vector.
struct Bytes<'a>(&'a mut Vec<u8>);

impl Write for Bytes<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROMPT: &str = "$ ";

    fn editor(columns: usize) -> (LineEditor, Vec<u8>) {
        let mut e = LineEditor::new();
        let mut out = Vec::new();
        e.start(PROMPT, columns, &mut out);
        (e, out)
    }

    /// Feeds every byte; returns the last result and what was drawn.
    fn feed(e: &mut LineEditor, input: &[u8]) -> (Feed, Vec<u8>) {
        let mut out = Vec::new();
        let mut last = Feed::Pending;
        for &b in input {
            last = e.feed(b, &mut out);
        }
        (last, out)
    }

    fn line(e: &mut LineEditor, input: &[u8]) -> String {
        match feed(e, input).0 {
            Feed::Line(l) => l,
            other => panic!("expected a line, got {other:?}"),
        }
    }

    /// A tiny terminal: applies the editor's output to a grid, so the tests
    /// check what the user sees rather than the exact escape sequences.
    struct Screen {
        cols: usize,
        rows: Vec<Vec<u8>>,
        x: usize,
        y: usize,
    }

    impl Screen {
        fn new(cols: usize) -> Screen {
            Screen {
                cols,
                rows: vec![Vec::new()],
                x: 0,
                y: 0,
            }
        }

        fn apply(&mut self, bytes: &[u8]) {
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                match b {
                    b'\n' => {
                        self.x = 0;
                        self.y += 1;
                    }
                    b'\r' => self.x = 0,
                    0x1B => {
                        assert_eq!(bytes[i], b'[');
                        i += 1;
                        let start = i;
                        while bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                        let n: usize = core::str::from_utf8(&bytes[start..i])
                            .unwrap()
                            .parse()
                            .unwrap_or(1);
                        match bytes[i] {
                            b'A' => self.y -= n,
                            b'B' => self.y += n,
                            b'C' => self.x += n,
                            b'H' => (self.x, self.y) = (0, 0),
                            b'J' if n == 2 => self.rows = vec![Vec::new()],
                            b'J' => {
                                self.rows[self.y].truncate(self.x);
                                self.rows.truncate(self.y + 1);
                            }
                            other => panic!("unexpected CSI {}", other as char),
                        }
                        i += 1;
                    }
                    _ => {
                        if self.x == self.cols {
                            self.x = 0;
                            self.y += 1;
                        }
                        while self.rows.len() <= self.y {
                            self.rows.push(Vec::new());
                        }
                        let row = &mut self.rows[self.y];
                        if row.len() <= self.x {
                            row.resize(self.x + 1, b' ');
                        }
                        row[self.x] = b;
                        self.x += 1;
                    }
                }
                while self.rows.len() <= self.y {
                    self.rows.push(Vec::new());
                }
            }
        }

        fn text(&self) -> Vec<String> {
            self.rows
                .iter()
                .map(|r| String::from_utf8_lossy(r).into_owned())
                .collect()
        }
    }

    /// Runs `input` through a fresh editor on a `cols`-wide screen.
    fn screen_after(cols: usize, input: &[u8]) -> (Screen, LineEditor) {
        let (mut e, out) = editor(cols);
        let mut s = Screen::new(cols);
        s.apply(&out);
        let (_, out) = feed(&mut e, input);
        s.apply(&out);
        (s, e)
    }

    #[test]
    fn typing_and_enter_give_the_line() {
        let (mut e, out) = editor(80);
        assert_eq!(out, b"$ ");
        let (result, out) = feed(&mut e, b"echo hi\r");
        assert_eq!(result, Feed::Line("echo hi".into()));
        assert_eq!(out, b"echo hi\n");
    }

    #[test]
    fn cr_lf_is_one_enter_and_lf_alone_is_enter() {
        let (mut e, _) = editor(80);
        assert_eq!(line(&mut e, b"a\r"), "a");
        assert_eq!(feed(&mut e, b"\n").0, Feed::Pending);
        assert_eq!(line(&mut e, b"b\n"), "b");
        assert_eq!(line(&mut e, b"\n"), "");
    }

    #[test]
    fn cursor_keys_edit_in_the_middle() {
        // "helo", ←, insert "l", Home, insert ">", End, "!".
        let (s, mut e) = screen_after(80, b"helo\x1b[Dl\x01>\x05!");
        assert_eq!(s.text(), ["$ >hello!"]);
        assert_eq!(s.x, 9);
        assert_eq!(line(&mut e, b"\r"), ">hello!");
        let (s, mut e) = screen_after(80, b"abc\x1b[H\x1b[C\x1b[3~\x1b[F\x1b[1~x\x1b[4~y");
        assert_eq!(s.text(), ["$ xacy"]);
        assert_eq!(line(&mut e, b"\r"), "xacy");
        let (_, mut e) = screen_after(80, b"ab\x1bOH-\x1bOF+");
        assert_eq!(line(&mut e, b"\r"), "-ab+");
    }

    #[test]
    fn backspace_deletes_before_the_cursor() {
        let (s, mut e) = screen_after(80, b"abcd\x7f\x1b[D\x08");
        assert_eq!(s.text(), ["$ ac"]);
        assert_eq!(s.x, 3);
        assert_eq!(line(&mut e, b"\r"), "ac");
        // Nothing to delete at the start of the line.
        let (s, _) = screen_after(80, b"\x7f\x7f");
        assert_eq!(s.text(), ["$ "]);
    }

    #[test]
    fn ctrl_c_cancels_the_line() {
        let (mut e, _) = editor(80);
        let (result, out) = feed(&mut e, b"rm -r x\x01\x03");
        assert_eq!(result, Feed::Cancelled);
        assert!(out.ends_with(b"^C\n"));
        assert_eq!(e.history().count(), 0);
    }

    #[test]
    fn ctrl_l_clears_the_screen_and_redraws_the_line() {
        let (s, _) = screen_after(80, b"ls -l\x1b[D\x0c");
        assert_eq!(s.text(), ["$ ls -l"]);
        assert_eq!((s.x, s.y), (6, 0));
    }

    #[test]
    fn history_steps_back_and_forth_and_keeps_the_draft() {
        let (mut e, _) = editor(80);
        line(&mut e, b"one\r");
        line(&mut e, b"two\r");
        let mut s = Screen::new(80);
        let mut out = Vec::new();
        e.start(PROMPT, 80, &mut out);
        s.apply(&out);
        let mut keys = |input: &[u8]| {
            let (_, out) = feed(&mut e, input);
            s.apply(&out);
            s.text()
        };
        assert_eq!(keys(b"dra\x1b[A"), ["$ two"]);
        assert_eq!(keys(b"\x1b[A\x1b[A"), ["$ one"], "stops at the oldest");
        assert_eq!(keys(b"\x1b[B"), ["$ two"]);
        assert_eq!(keys(b"\x1b[B"), ["$ dra"], "back to the draft");
        assert_eq!(line(&mut e, b"ft\r"), "draft");
    }

    #[test]
    fn history_keeps_the_last_50_distinct_non_empty_lines() {
        let (mut e, _) = editor(80);
        for i in 0..60 {
            line(&mut e, format!("cmd {i}\r").as_bytes());
        }
        line(&mut e, b"cmd 59\r");
        line(&mut e, b"   \r");
        let h: Vec<&str> = e.history().collect();
        assert_eq!(h.len(), HISTORY);
        assert_eq!(h[0], "cmd 10");
        assert_eq!(h[49], "cmd 59");
    }

    #[test]
    fn long_lines_wrap_and_edits_redraw_every_row() {
        // 10 columns: "$ " plus 12 characters takes two rows.
        let (s, mut e) = screen_after(10, b"abcdefghijkl");
        assert_eq!(s.text(), ["$ abcdefgh", "ijkl"]);
        assert_eq!((s.x, s.y), (4, 1));
        // Home, then insert at the start: everything shifts across the wrap.
        let mut s = s;
        let (_, out) = feed(&mut e, b"\x01XY");
        s.apply(&out);
        assert_eq!(s.text(), ["$ XYabcdef", "ghijkl"]);
        assert_eq!((s.x, s.y), (4, 0));
        let (_, out) = feed(&mut e, b"\x05");
        s.apply(&out);
        assert_eq!((s.x, s.y), (6, 1));
    }

    #[test]
    fn a_line_ending_exactly_at_the_edge_moves_to_the_next_row() {
        let (s, _) = screen_after(10, b"12345678");
        assert_eq!(s.text(), ["$ 12345678", ""]);
        assert_eq!((s.x, s.y), (0, 1));
        let (s, _) = screen_after(10, b"12345678\x7f");
        assert_eq!(s.text(), ["$ 1234567"]);
        assert_eq!((s.x, s.y), (9, 0));
    }

    #[test]
    fn a_lone_escape_does_not_swallow_the_next_key() {
        let (mut e, _) = editor(80);
        assert_eq!(line(&mut e, b"ab\x1b\r"), "ab");
        assert_eq!(feed(&mut e, b"x\x1b\x03").0, Feed::Cancelled);
        assert_eq!(line(&mut e, b"\x1bq\r"), "q");
    }

    #[test]
    fn modified_keys_act_like_plain_ones() {
        // Ctrl+← and Ctrl+Delete send a second parameter.
        let (_, mut e) = screen_after(80, b"abc\x1b[1;5D\x1b[1;5D\x1b[3;5~");
        assert_eq!(line(&mut e, b"\r"), "ac");
    }

    #[test]
    fn unknown_bytes_and_sequences_are_ignored() {
        let (s, mut e) = screen_after(80, b"a\x02\x1b[5~\x1b[1;5C\xc3\xa9b");
        assert_eq!(s.text(), ["$ ab"]);
        assert_eq!(line(&mut e, b"\r"), "ab");
    }
}
