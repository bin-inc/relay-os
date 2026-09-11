# Task 8 VFS Traversal And Read-Only Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a host-tested semantic VFS and read-only command shell above the controlled ext2 reader.

**Architecture:** `vfs` owns the read-only filesystem trait, its `Ext2` adapter, canonical current-directory state, and semantic component-by-component resolution. `shell::parser` turns bounded ASCII input into typed command values; `shell::commands` executes the read-only subset through `Vfs` and existing infallible `TextOutput`, leaving mutation behavior explicitly unavailable.

**Tech Stack:** Rust 1.98.1, edition 2024, `no_std` plus `alloc`, existing `relay-core::{ext2, fs, console}`, host ext2 image fixtures.

**Spec:** `docs/superpowers/specs/2026-09-11-task-8-vfs-shell-read-design.md`

## Global Constraints

- `relay-core` remains `#![no_std]`; host filesystem/process APIs stay in integration-test support.
- `FileSystem` is read-only in Task 8: root, metadata, lookup, directory listing, and offset reads only. Task 9 extends it for mutation.
- VFS resolution is semantic and left-to-right: a component is looked up before `..` is processed, so `missing/../file` returns `NotFound`.
- Paths are ASCII, at most 4,096 bytes, use `Name` validation for ordinary components, ignore repeated separators and `.`, and clamp `..` at root.
- Shell input is ASCII and at most 512 bytes. Quotes, backslash, whitespace, empty quoted arguments, and adjacent fragments follow the approved specification exactly.
- No unsafe code, raw ext2/block details, filesystem mutation, line editing, USB, framebuffer/keyboard runtime integration, redirection, globbing, or expansion belongs in this task.
- `cat` streams with one fixed 4 KiB buffer and never allocates based on file size. Non-printable bytes other than LF, CR, and tab render as `?`.
- All allocation, parser, VFS, and filesystem errors remain typed; a failed command emits one stable `error: ` line and cannot partially update Cwd.
- Maintain `cargo fmt --all --check`, workspace Clippy with `-D warnings`, and locked workspace tests.

---

## File Structure

```text
crates/relay-core/src/lib.rs                 Exposes VFS and shell modules.
crates/relay-core/src/vfs/mod.rs             Read-only FileSystem contract, errors, Ext2 adapter, Vfs API.
crates/relay-core/src/vfs/path.rs            Canonical Cwd, resolved path state, and semantic traversal.
crates/relay-core/src/shell/mod.rs           Shell owner, public errors, and command execution boundary.
crates/relay-core/src/shell/parser.rs        Bounded ASCII tokenizer and typed full-milestone command parsing.
crates/relay-core/src/shell/commands.rs      Read-only command rendering and unavailable mutation dispatch.
crates/relay-core/tests/vfs.rs               VFS semantic path and error-mapping tests.
crates/relay-core/tests/shell_parser.rs      Token/quote/arity/error tests.
crates/relay-core/tests/shell_read.rs        Exact TextOutput shell vertical-slice tests.
```

### Task 1: Read-Only FileSystem Adapter And Semantic VFS

**Files:**
- Create: `crates/relay-core/src/vfs/mod.rs`
- Create: `crates/relay-core/src/vfs/path.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-core/tests/vfs.rs`

**Interfaces:**
- Consumes: `ext2::{Ext2, Ext2Error}`, `fs::{NodeId, Name, Metadata, DirEntry, NodeKind}`.
- Produces: `FileSystem`, `FsError`, `Vfs<F>`, `Cwd`, `ResolvedPath`, `VfsError`, and read/list/change-directory operations for the shell.

- [ ] **Step 1: Write failing semantic traversal tests**

Create `tests/vfs.rs` using Task 7's `support::ext2_image::fixture_with_files` and `Ext2::mount`. Add these contract tests:

```rust
#[test]
fn traversal_does_not_lexically_cancel_a_missing_component() {
    let mut vfs = fixture_vfs(&[("file", b"relay")]);
    let cwd = vfs.initial_cwd();

    assert!(matches!(vfs.resolve(&cwd, "missing/../file"), Err(VfsError::Fs(FsError::NotFound))));
}

#[test]
fn resolution_normalizes_root_relative_dot_and_parent_components() {
    let mut vfs = fixture_vfs_with_directory("docs", "guide", b"read me");
    let cwd = vfs.initial_cwd();
    let docs = vfs.change_dir(&cwd, "/docs/./").unwrap();
    let root = vfs.change_dir(&docs, "../../").unwrap();

    assert_eq!(vfs.cwd_path(&docs), b"/docs");
    assert_eq!(vfs.cwd_path(&root), b"/");
    assert_eq!(vfs.cwd_path(&vfs.change_dir(&root, "docs//").unwrap()), b"/docs");
}

#[test]
fn trailing_slash_requires_a_directory() {
    let mut vfs = fixture_vfs(&[("file", b"relay")]);
    let cwd = vfs.initial_cwd();

    assert!(matches!(vfs.resolve(&cwd, "file/"), Err(VfsError::NotDirectory)));
}
```

- [ ] **Step 2: Run VFS tests to verify missing APIs fail**

Run: `cargo test -p relay-core --test vfs --locked`

Expected: FAIL because `relay_core::vfs`, `FileSystem`, and `Vfs` are absent.

- [ ] **Step 3: Define the read-only generic filesystem boundary**

In `vfs/mod.rs`, define these exact public types and implement `FileSystem` for `Ext2<D>` by mapping every existing `Ext2Error` to one `FsError` category:

```rust
pub trait FileSystem {
    fn root(&self) -> NodeId;
    fn metadata(&mut self, node: NodeId) -> Result<Metadata, FsError>;
    fn lookup(&mut self, dir: NodeId, name: &Name) -> Result<NodeId, FsError>;
    fn read_dir(&mut self, dir: NodeId) -> Result<Vec<DirEntry>, FsError>;
    fn read_at(&mut self, node: NodeId, offset: u64, dst: &mut [u8]) -> Result<usize, FsError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsError { NotFound, WrongNodeKind, Corrupt, Unsupported, Allocation, Io }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfsError { Fs(FsError), InvalidPath, NotDirectory, NotRegularFile, Allocation }

pub struct Vfs<F> { filesystem: F }
```

Map `Ext2Error::NotFound` to `FsError::NotFound`, `WrongNodeKind` to `WrongNodeKind`, `Allocation` to `Allocation`, `Block(_)` to `Io`, `UnsupportedFile` to `Unsupported`, and every profile/mount/corrupt/sparse/invalid-node error to `Corrupt`. Add `pub mod vfs;` to `lib.rs`.

- [ ] **Step 4: Implement canonical Cwd and semantic resolver**

In `vfs/path.rs`, make `Cwd` own `Vec<Name>` and `ResolvedPath` own a `NodeId` plus canonical components. Use fallible vector reservation before each component push. Implement these methods on `Vfs<F: FileSystem>`:

```rust
pub fn new(filesystem: F) -> Self;
pub fn initial_cwd(&self) -> Cwd;
pub fn cwd_path(&self, cwd: &Cwd) -> Vec<u8>;
pub fn resolve(&mut self, cwd: &Cwd, path: &str) -> Result<ResolvedPath, VfsError>;
pub fn change_dir(&mut self, cwd: &Cwd, path: &str) -> Result<Cwd, VfsError>;
pub fn list(&mut self, cwd: &Cwd, path: Option<&str>) -> Result<Vec<DirEntry>, VfsError>;
pub fn read_file(
    &mut self,
    cwd: &Cwd,
    path: &str,
    output: impl FnMut(&[u8]) -> Result<(), VfsError>,
) -> Result<(), VfsError>;
```

Reject a path over 4,096 bytes or containing non-ASCII bytes with `InvalidPath`. Start absolute paths at `filesystem.root()` and empty components; start relative paths at a freshly resolved copy of Cwd components. For each ordinary component, require current node metadata to be `Directory`, call `Name::new`, then call `filesystem.lookup` immediately. Process `..` only after all preceding ordinary components have resolved. For the final node, enforce trailing slash directory requirement; `change_dir` always requires a directory; `list` resolves its optional path or Cwd and requires a directory; `read_file` requires `Regular` and streams up to 4 KiB chunks until `read_at` returns zero or full metadata length is consumed.

- [ ] **Step 5: Add path, error, and streaming tests**

Extend `vfs.rs` to cover repeated separators, `.` at every position, root-clamped parents, relative paths after `change_dir`, missing final/intermediate nodes, regular intermediate nodes, non-ASCII, slash-only paths, 4,097-byte paths, and a file with binary bytes read through an output collector. Assert VFS error mappings with a small `FileSystem` fake that returns each `FsError`, and verify `read_file` invokes the callback with no more than 4,096 bytes per call for a file larger than one chunk.

- [ ] **Step 6: Run focused VFS verification**

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test vfs --locked
cargo clippy -p relay-core --all-targets --locked -- -D warnings
```

Expected: all semantic traversal, typed error, canonical Cwd, and bounded streaming tests pass without warnings.

- [ ] **Step 7: Commit VFS**

```bash
git add crates/relay-core/src/lib.rs crates/relay-core/src/vfs crates/relay-core/tests/vfs.rs
git commit -m "feat: add read-only VFS traversal"
```

### Task 2: Bounded Shell Tokenizer And Typed Commands

**Files:**
- Create: `crates/relay-core/src/shell/mod.rs`
- Create: `crates/relay-core/src/shell/parser.rs`
- Create: `crates/relay-core/src/shell/commands.rs`
- Modify: `crates/relay-core/src/lib.rs`
- Create: `crates/relay-core/tests/shell_parser.rs`

**Interfaces:**
- Consumes: `alloc::{String, Vec}` and Task 1 VFS path strings.
- Produces: `tokenize`, `parse_command`, `Command`, `ParseError`, and `ShellError` for Task 3 dispatch and Tasks 9-10 mutation extension.

- [ ] **Step 1: Write failing tokenizer and command-arity tests**

Create `tests/shell_parser.rs` and specify real parser behavior:

```rust
#[test]
fn quoted_fragments_and_empty_arguments_are_preserved() {
    assert_eq!(tokenize("write a x\" y\" ''").unwrap(), ["write", "a", "x y", ""]);
    assert!(matches!(
        parse_command("write a ''").unwrap(),
        Some(Command::Write { path, text }) if path == "a" && text.is_empty()
    ));
}

#[test]
fn quotes_and_backslashes_follow_shell_states() {
    assert_eq!(tokenize("echo a' b'c \"d\\ e\" 'f\\g'").unwrap(), ["echo", "a bc", "d e", "f\\g"]);
    assert!(matches!(tokenize("echo 'unterminated"), Err(ParseError::UnterminatedQuote)));
    assert!(matches!(tokenize("echo abc\\"), Err(ParseError::InvalidEscape)));
}

#[test]
fn parser_requires_exact_read_command_arities() {
    assert!(matches!(parse_command("cd"), Err(ParseError::Arity { .. })));
    assert!(matches!(parse_command("ls a b"), Err(ParseError::Arity { .. })));
    assert!(matches!(parse_command("cat"), Err(ParseError::Arity { .. })));
}
```

- [ ] **Step 2: Run parser tests to verify missing APIs fail**

Run: `cargo test -p relay-core --test shell_parser --locked`

Expected: FAIL because `relay_core::shell`, `tokenize`, and `Command` are absent.

- [ ] **Step 3: Implement bounded ASCII tokenization**

In `shell/parser.rs`, define `ParseError::{NonAscii, InputTooLong, ControlByte, UnterminatedQuote, InvalidEscape, UnknownCommand, Allocation, Arity { command: &'static str }}`. Reject lines longer than 512 bytes before allocation, non-ASCII bytes, and control bytes except space/tab/CR/LF. Scan bytes using three states: unquoted, single-quoted, double-quoted. Outside quotes and in double quotes, backslash requires and appends a following byte in `b' '..=b'~'`; in single quotes it is literal. Whitespace ends a nonempty accumulated token; quote transitions do not end it, allowing adjacent fragments. Track whether an empty quote constructed an argument. Use fallible allocation for token and argument vectors, mapping failure to `Allocation`.

Expose `pub fn tokenize(line: &str) -> Result<Vec<String>, ParseError>`; an empty/whitespace line produces an empty vector.

- [ ] **Step 4: Parse all milestone commands into owned typed values**

Define in `shell/commands.rs`:

```rust
pub enum Command {
    Help, Pwd, Cd { path: String }, Ls { path: Option<String> }, Cat { path: String }, Echo { text: Vec<String> },
    Touch { path: String }, Write { path: String, text: String }, Append { path: String, text: String },
    Mkdir { path: String }, Rm { path: String }, Rmdir { path: String }, Sync, Shutdown,
}
```

`parse_command(line)` calls `tokenize`, returns `Ok(None)` for zero tokens, recognizes every listed command, rejects unknown names with `ParseError::UnknownCommand`, and applies exact arity. `echo` has zero or more text arguments. `write` and `append` require only a path; they join zero or more trailing text tokens with one ASCII space, producing an empty text string when none are present. All other mutation commands have their specified single/no argument shape. Define `ShellError::{Parse(ParseError), Vfs(VfsError), Unavailable, Allocation}` in `shell/mod.rs`, and export parser/command public types from that module. Add `pub mod shell;` to `lib.rs`.

- [ ] **Step 5: Add negative parser coverage and run focused verification**

Add tests for empty lines, spaces/tabs/CR/LF separation, empty single/double quotes, adjacent fragments, escape in and out of double quotes, literal single-quote backslash, non-ASCII, invalid controls, 513-byte input, unknown names, and exact arities for every read and mutation command. Assert `write a` and `append a` parse to empty text, while their missing-path forms fail. Assert mutation variants parse successfully, not that they execute.

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test shell_parser --locked
cargo clippy -p relay-core --all-targets --locked -- -D warnings
```

Expected: every valid command becomes its intended owned `Command`, every malformed input returns the precise parse category, and output is warning-free.

- [ ] **Step 6: Commit parser and commands**

```bash
git add crates/relay-core/src/lib.rs crates/relay-core/src/shell crates/relay-core/tests/shell_parser.rs
git commit -m "feat: parse relay shell commands"
```

### Task 3: Read-Only Shell Dispatch And Ext2 Vertical Slice

**Files:**
- Modify: `crates/relay-core/src/shell/mod.rs`
- Modify: `crates/relay-core/src/shell/commands.rs`
- Create: `crates/relay-core/tests/shell_read.rs`

**Interfaces:**
- Consumes: `Vfs<F>`, `Cwd`, `VfsError`, typed `Command`, and `console::TextOutput`.
- Produces: `Shell<F, O>::new`, `Shell::run`, and exact output behavior for read commands.

- [ ] **Step 1: Write failing exact-output shell tests**

Create `tests/shell_read.rs` with a `Recorder(Vec<u8>)` implementing `TextOutput` and a helper that mounts a Task 7 ext2 fixture through `Vfs`. Add this vertical-slice test:

```rust
#[test]
fn shell_runs_read_commands_against_an_ext2_fixture() {
    let mut shell = fixture_shell(&[("alpha", b"hello\n\x01")]);

    shell.run("pwd").unwrap();
    shell.run("ls").unwrap();
    shell.run("cat alpha").unwrap();
    shell.run("echo relay os").unwrap();

    assert_eq!(shell.output().0, b"/\nalpha file 7\nhello\n?relay os\n");
}

#[test]
fn failed_cd_preserves_the_working_directory_and_writes_one_error_line() {
    let mut shell = fixture_shell(&[("file", b"relay")]);

    shell.run("cd missing").unwrap();
    shell.run("pwd").unwrap();

    assert_eq!(shell.output().0, b"error: not found\n/\n");
}
```

- [ ] **Step 2: Run shell tests to verify dispatch is absent**

Run: `cargo test -p relay-core --test shell_read --locked`

Expected: FAIL because `Shell` and `Shell::run` are absent.

- [ ] **Step 3: Implement Shell ownership and stable error rendering**

In `shell/mod.rs`, define:

```rust
pub struct Shell<F, O> { vfs: Vfs<F>, cwd: Cwd, output: O }

impl<F: FileSystem, O: TextOutput> Shell<F, O> {
    pub fn new(vfs: Vfs<F>, output: O) -> Self;
    pub fn run(&mut self, line: &str) -> Result<(), ShellError>;
    pub fn output(&self) -> &O;
    pub fn output_mut(&mut self) -> &mut O;
}
```

`run` parses the line, does nothing for `None`, dispatches the command, and writes exactly one `error: <stable message>\n` line for handled parse/VFS/unavailable failures before returning `Ok(())`. Return `Err(ShellError::Allocation)` only when shell-owned formatting allocation fails. Map errors to concise stable ASCII text: `not found`, `not a directory`, `not a regular file`, `invalid path`, `invalid command`, `invalid arguments`, `command unavailable`, `filesystem corrupt`, `unsupported file`, `out of memory`, and `I/O failure`.

- [ ] **Step 4: Dispatch read-only commands through VFS**

In `shell/commands.rs`, implement `execute_read_command`. `help` emits a stable multi-line command/usage table containing every milestone command, with mutation rows marked `unavailable`. `pwd` emits `cwd_path` plus LF. `cd` obtains a candidate Cwd from VFS then assigns it only after success. `ls` calls VFS list and emits each `name type size\n`, where `type` is `file` or `dir` and `size` is retrieved with `Vfs::metadata`; no sort occurs. `cat` calls `read_file` with a callback that writes each byte as printable ASCII/LF/CR/tab or `?`; do not add an implicit trailing LF. `echo` joins already parsed arguments with spaces and emits one LF.

For `Touch`, `Write`, `Append`, `Mkdir`, `Rm`, `Rmdir`, `Sync`, and `Shutdown`, emit `error: command unavailable\n` without calling VFS. Do not add mutation methods or placeholders to `FileSystem`.

- [ ] **Step 5: Add complete shell behavior coverage**

Extend `shell_read.rs` with exact output assertions for `help`, empty `echo`, quoted `echo`, relative/absolute `cd`, `ls PATH`, `ls` defaulting to Cwd, `cat` binary replacement plus preserved tab/CR/LF, missing/wrong-type paths, parser errors, every unavailable mutation command, and Cwd preservation after failed `cd`/`cat`/`ls`. Add a recorder output test proving large `cat` output arrives correctly across more than one VFS 4 KiB callback.

- [ ] **Step 6: Run the read-only vertical slice and workspace verification**

Run:

```bash
cargo fmt --all --check
cargo test -p relay-core --test vfs --test shell_parser --test shell_read --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: VFS, parser, and ext2-backed shell integration tests pass with exact recorded output, warning-free linting, and no regression in the workspace suite.

- [ ] **Step 7: Commit read-only shell**

```bash
git add crates/relay-core/src/shell crates/relay-core/tests/shell_read.rs
git commit -m "feat: add read-only shell"
```
