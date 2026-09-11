# Task 8 VFS Traversal And Read-Only Shell Design

## Status

Approved for implementation on 2026-09-11.

## Objective

Task 8 adds a host-tested read-only virtual filesystem and shell above Task
7's ext2 reader. It gives the later kernel runtime a filesystem-independent
path and command boundary while exposing only the read-side behavior already
implemented by ext2: navigation, directory listing, file reading, command
parsing, and text output.

No mutation API is added in this task. Task 9 extends the filesystem contract
and Task 10 extends shell dispatch once durable ext2 mutation exists.

## Filesystem And VFS Boundaries

`relay-core::vfs` defines the read-only filesystem contract and adapts
`Ext2<D>` to it:

```rust
pub trait FileSystem {
    fn root(&self) -> NodeId;
    fn metadata(&mut self, node: NodeId) -> Result<Metadata, FsError>;
    fn lookup(&mut self, dir: NodeId, name: &Name) -> Result<NodeId, FsError>;
    fn read_dir(&mut self, dir: NodeId) -> Result<Vec<DirEntry>, FsError>;
    fn read_at(
        &mut self,
        node: NodeId,
        offset: u64,
        dst: &mut [u8],
    ) -> Result<usize, FsError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsError {
    NotFound,
    WrongNodeKind,
    Corrupt,
    Unsupported,
    Allocation,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfsError {
    Fs(FsError),
    InvalidPath,
    NotDirectory,
    NotRegularFile,
    Allocation,
}
```

`FsError` maps relevant lower-layer errors to stable generic categories needed
by VFS and shell: missing node, wrong node type, corrupt filesystem data,
unsupported file, resource exhaustion, and I/O failure.
`Ext2<D>` implements this trait by mapping its existing `Ext2Error` variants;
the VFS and shell never depend on `BlockDevice`, ext2 records, or inode IDs.

`Vfs<F>` owns the filesystem and stores no global mutable path cache. `Cwd`
stores a canonical absolute sequence of validated `Name` components, initially
empty for root. `Vfs::initial_cwd()` produces root; `Vfs::cwd_path(&Cwd)`
renders `/` or slash-prefixed component bytes.

Path traversal is semantic and left-to-right. A leading slash starts at root;
otherwise resolution starts at the supplied `Cwd`. Empty components from
repeated or leading/trailing separators are ignored. `.` is ignored. `..`
removes one already-resolved component but clamps at root. Every ordinary
component is converted through `Name::new` and looked up immediately against
the current directory before the next component is processed. Thus
`missing/../file` returns `NotFound`; it must never lexically cancel `missing`.
Each intermediate resolved node and the final node when required is verified
to be a directory. A trailing slash requires the resolved final node to be a
directory.

`resolve` returns a `ResolvedPath` containing its node and canonical absolute
components. `change_dir` accepts only a resolved directory and returns its new
canonical `Cwd`. `list` accepts a path or current working directory and
returns its validated entries. `read_file` accepts only a regular file and
performs bounded repeated `FileSystem::read_at` calls through a fixed 4 KiB
buffer, mapping all errors without assuming ext2 block layout. Its output is
delivered through `FnMut(&[u8]) -> Result<(), VfsError>`, so it does not
allocate based on file size and a shell output failure can stop streaming.

## Shell Parser

`relay-core::shell::parser` tokenizes a single UTF-8 Rust `&str` after
rejecting non-ASCII bytes and control bytes other than ASCII whitespace. Token
separators are space, tab, carriage return, and line feed. Single and double
quotes preserve separator bytes inside an argument and are removed. A
backslash outside or inside double quotes consumes and appends exactly the next
printable ASCII byte. Inside single quotes, every byte other than the closing
quote is literal, including backslash. Adjacent quoted and unquoted fragments
accumulate into one argument. Unterminated quotes, a backslash without a
following printable byte, and an over-512-byte input return typed parse errors.
Empty quoted arguments are preserved. An empty or whitespace-only line returns
`Ok(None)`.

The parser builds a typed `Command` enum for every milestone command now so
syntax stays stable across Tasks 9 and 10. Task 8 dispatches only:

```text
help
pwd
cd PATH
ls [PATH]
cat PATH
echo [TEXT ...]
```

Mutation command variants are parsed with their exact required arity but
dispatch returns `ShellError::Unavailable` until Task 10. Unknown commands and
wrong argument counts return concise typed errors. Command path/text arguments
are owned ASCII strings produced by tokenizer output, not borrowed console
buffers.

## Read-Only Command Dispatch

`Shell<F, O>` owns `Vfs<F>`, its `Cwd`, and an output implementing existing
`console::TextOutput`. `run(line)` parses one line and writes command output
or one concise error line; it returns a typed result for host tests. The shell
does not know framebuffer geometry, serial, keyboard scan codes, ext2, or block
devices.

- `help` prints the complete milestone command list and usage, marking mutation
  commands unavailable in the read-only build.
- `pwd` prints the canonical absolute working directory followed by LF.
- `cd PATH` resolves and commits a new Cwd only after it confirms a directory.
- `ls [PATH]` lists entries in on-disk order as `name type size` lines, where
  type is `file` or `dir`; absent path defaults to Cwd.
- `cat PATH` resolves a regular file and streams its bytes through the VFS. It
  writes printable ASCII, LF, CR, and tab unchanged; every other byte becomes
  `?`. It adds no newline not present in the file.
- `echo [TEXT ...]` writes parsed arguments joined with one ASCII space and a
  trailing LF.

On command failure, Cwd remains unchanged and the shell emits one line with a
stable `error: ` prefix. A filesystem or allocation error never causes a panic
or partial Cwd update.

## Resource Limits And Errors

VFS rejects paths longer than 4,096 bytes and components that `Name::new`
rejects. It uses fallible `Vec` growth and maps allocation failure to typed
`VfsError::Allocation` or `ShellError::Allocation`. Shell input is capped at
512 ASCII bytes before token allocation. `cat` uses one 4 KiB buffer and never
allocates based on file length; `ls` uses the filesystem's bounded directory
result and formats one entry at a time.

All expected parser, traversal, filesystem, and output errors are typed. No
unsafe code is needed. The read-only trait deliberately excludes create,
write, truncate, deletion, sync, unmount, and shutdown behavior until later
tasks add their durable semantics.

## Testing

Host tests use Task 7's file-backed ext2 fixture through `Ext2` and its
`FileSystem` implementation, plus a `TextOutput` recorder. VFS tests cover
root and relative paths, repeated separators, `.`, root-clamped `..`, semantic
missing-component behavior, intermediate/final wrong types, trailing slashes,
invalid/non-ASCII/overlong input, Cwd rendering, and error mapping.

Parser tests cover all quote and backslash states, empty arguments, line and
input limits, non-ASCII/control rejection, unknown commands, and every command
arity. Shell integration tests compare exact recorder output for `help`, `pwd`,
`cd`, `ls`, `cat`, and `echo`; cover binary `cat` rendering, read-only mutation
unavailability, unchanged Cwd after errors, and a multi-command ext2 fixture
vertical slice.

Verification runs:

```bash
cargo test -p relay-core --test vfs --test shell_parser --test shell_read --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Non-Goals

Task 8 does not modify ext2 media, add filesystem mutation methods, implement
line editing, attach shell execution to USB/keyboard/framebuffer runtime,
provide pipes/redirection/expansion/globbing/environment variables, support
non-ASCII input, or add process/user-mode behavior.
