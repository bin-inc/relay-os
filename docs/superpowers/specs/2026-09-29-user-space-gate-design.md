# Relay OS — User-Space Gate Design: programs, not built-ins (milestones 2 and 3)

- **Date:** 2026-09-29
- **Status:** Approved 2026-09-29; revised while planning milestone 2's plans 1, 2, 3a, 3b, 4a, 4b and 5
  and milestone 3's plans 1, 2 and 3 (see §16)
- **Builds on:** milestone 1 (version 0.2.0,
  `docs/superpowers/specs/2026-09-26-milestone-1-boot-shell-fs-design.md`,
  cited below as "M1 §n")
- **Branch:** `main`, one pull request at a time, as in milestone 1

## 1. Intent

Milestone 1 runs the shell and every command inside the kernel. This gate
moves them out: the shell and each command become separate programs running
in ring 3, each in its own address space, loaded from a read-only `/bin`.
A program that crashes is killed and the shell carries on; a busy program is
preempted; programs are connected with pipes and run in the background.

The gate spans two milestones, each built like milestone 1 (a roadmap of
plans, a version, a NUC check).

### 1.1 What the owner specified

- The next gate is about 1–2 months of work and can be split into more than
  one milestone.
- Direction: user space ("programs, not built-ins"), chosen over a network
  console, an aarch64 port, a UI and a richer in-kernel shell.
- The long-term plan includes an ARM handheld target. It stays in this
  repository, behind an architecture layer, when it comes; this gate must
  not make that harder.
- Required on top of the core: pipes, background jobs, `ps` and `kill`,
  script arguments and variables.

### 1.2 Decisions made during brainstorming

| Topic | Decision |
|---|---|
| System-call interface | Our own Relay ABI (§7), not a Linux-compatible one. Every program comes from this repository, written in Rust against our own runtime crate. |
| Where the system programs live | A read-only archive, `system.img`, on the ESP next to `kernel.elf`, loaded by `relay-boot` and mounted at `/bin` (§4). `flash --kernel` updates the kernel and the programs together. |
| Kernel structure | Monolithic: drivers, VFS and ext2 stay in the kernel. |
| Process creation | `spawn(path, args, fds)`; no `fork`, no copy-on-write. |
| Kernel preemption | None. The timer preempts user code only; a system call runs until it returns or blocks at a defined point. |
| Milestone split | Milestone 2 "Programs" (0.3.0): processes, and the shell and commands as programs. Milestone 3 "Pipes and jobs" (0.4.0): pipes, jobs, `ps`/`kill`, script variables. |
| Cut line if time runs short | Background jobs and `jobs` (§9.2) move to a later milestone. |

### 1.3 Assumptions (carried over from milestone 1 unless stated)

- Only x86_64, one CPU core. Drivers keep polling (M1 §14).
- Single user, `root`; permission bits are not enforced (M1 §1.3).
- User programs are Rust, `no_std + alloc`, built on stable Rust. They use
  no floating point (§5.5).
- Programs are static ELF executables; there is no dynamic linking.

### 1.4 Success criteria (definition of done)

**Milestone 2 (version 0.3.0):**

1. `cargo xtask ci` passes, including every scenario of M1 §9.3 (unchanged
   except for the new startup line, §4.3) and the new scenarios of §12.3.
2. On the NUC, with a stick written by `cargo xtask flash --full`, the check
   scripts `check3-a.sh`, `check3-b.sh` and `check4.sh` pass, and
   `cargo xtask verify-usb` reports their transcripts clean.
3. `relay-kernel` no longer depends on the `shell` crate: every command the
   user types runs as a ring-3 program from `/bin`, except the shell
   built-ins listed in §8.3.

**Milestone 3 (version 0.4.0):** all of the above, plus `check5.sh` on the
NUC and the milestone 3 scenarios of §12.3.

## 2. Gate structure

| Milestone | Version | Delivers | Ends with |
|---|---|---|---|
| 2 · Programs | 0.3.0 | `system.img` and `/bin`; ring 3, address spaces, ELF loading, the system calls of §7 except those reserved for milestone 3; process table, scheduler, blocking, fault isolation, Ctrl-C; `relay-rt`; `/bin/sh` and one program per command; the kernel stops linking `shell` | §1.4 items 1–3; NUC checks 3 and 4 |
| 3 · Pipes and jobs | 0.4.0 | Pipes; reading standard input; `grep`, `seq`, `sleep`, `true`, `false`; background jobs, `jobs`, `wait`; `ps`, `kill`; script arguments and variables | NUC check 5 |

Each milestone gets its own roadmap in `docs/superpowers/plans/`, written
when the milestone starts; §13 sketches its plans.

## 3. Repository layout and components

New and changed units (the rest is as in M1 §3):

```
relay-os/
├─ crates/
│  ├─ relay-abi/    NEW  call numbers, #[repr(C)] types, error numbers, VERSION (no dependencies)
│  ├─ relay-rt/     NEW  program runtime: _start, system calls, allocator, panic handler, Vfs/Console/System over system calls
│  ├─ heap/         NEW  the allocator moved out of kernel/src/mm/heap.rs, shared by the kernel and relay-rt
│  ├─ sysimg/       NEW  system archive format: writer (xtask) and reader (kernel), SysImgFs
│  ├─ shell/        CHANGED  runs a command in-process (tests, host-shell) or by spawning /bin/<name> (/bin/sh)
│  └─ vfs/          CHANGED  new error numbers; mount points missing from their parent (§4.4)
├─ kernel/          CHANGED  arch/ gains user segments, syscall entry, context switch; new proc/, sched/, syscall/, elf/, pipe/
├─ boot/            CHANGED  loads \EFI\RELAY\system.img
├─ userland/        NEW
│  ├─ sh/           /bin/sh
│  ├─ utils/        one [[bin]] per command
│  └─ tests/        the t-* test programs (§8.5)
└─ xtask/           CHANGED  builds userland/, writes system.img
```

### 3.1 Principles

M1 §3.1 still holds, with three additions:

- **Architecture-specific code stays under `kernel/src/arch/` and
  `relay-rt`'s `arch` module.** That is system-call entry, the context
  switch, user segments and the page-table format. The process table, the
  scheduler, the system-call dispatcher and the ELF checks are
  architecture-neutral and host-tested through the kernel's library target
  (M1 §15 item 3).
- **`relay-abi` holds no x86 detail.** Only fixed-width integers and
  `#[repr(C)]` structs; register assignments are documented per
  architecture (§7.1) but appear only in the `arch` modules.
- **Commands keep their code.** The command functions in `crates/shell` do
  not change to become programs; `relay-rt` implements the `Vfs`, `Console`
  and `System` traits they already use.

### 3.2 Crate policy

M1 §3.2 is unchanged. No new external crates are needed; user programs use
`core`, `alloc` and the crates of this repository only.

## 4. Boot and the system archive

### 4.1 Disk layout

As M1 §4.1, with one more ESP file: `\EFI\RELAY\system.img`. The 64 MiB ESP
has room for it (the archive is expected to be a few MiB).

### 4.2 Archive format (`crates/sysimg`)

Little-endian throughout.

| Part | Contents |
|---|---|
| Header (64 bytes) | magic `RELAYSYS` (offset 0), format version `u32` (1, offset 8), ABI version `u32` (`relay_abi::VERSION`, offset 12), entry count `u32` (offset 16), total length `u64` (offset 24), CRC-32 of everything after the header `u32` (offset 32), build time `u64` in seconds since 1970 (offset 40), zeros elsewhere |
| Entry table | 88 bytes per entry: name (up to 64 bytes, no `/`, no NUL, not `.` or `..`, zero-padded, offset 0), its length `u8` (offset 64), mode `u16` (Unix permission bits only, offset 66), data offset `u64` (offset 72), data length `u64` (offset 80) |
| Data | each entry's bytes, starting on a 4 KiB boundary |

The writer sorts the entries by name. The reader refuses: fewer than 64
bytes, a wrong magic or format version, a length that does not match what
the loader read, a bad CRC, an entry table that does not fit, an invalid
name or mode, names out of order or repeated, and data outside the archive,
inside the header or table, or not on a 4 KiB boundary. The archive is flat:
every entry is a file in `/bin`.

`SysImgFs` implements `vfs::FileSystem` read-only directly over the loaded
bytes, with no copy: the root directory lists the entries, `read_at` reads
from the data, every change returns `EROFS`, and `stat` shows the entry's
mode, its length, and the archive's build time as the file times.

### 4.3 Loader and kernel

- **`relay-boot`** reads `\EFI\RELAY\system.img` if present, into pages it
  allocates as `LOADER_DATA`, and puts its physical address and length in
  `BootInfo` (new fields `system_image_phys`, `system_image_len`; address 0
  when the file cannot be read, an address and length 0 for an empty file).
  The loader does not check the archive; a missing or unreadable file is not
  a loader error. `BootInfo::version` goes up by one.
- **The kernel** keeps those frames reserved, checks the archive (§4.2) and
  its ABI version, and mounts `SysImgFs` at `/bin`. New startup step 10,
  before the shell (M1 §4.4 step 10 becomes step 11):
  `[ ok ] system: 42 programs, ABI 3` or `[FAIL] system: <reason>`
  (§11.2).

### 4.4 Mounting `/bin`

The images xtask writes have an empty `/bin` (`ROOT_DIRS`, since 0.2.0), and
the archive is mounted over it. But the empty read-only root the kernel falls
back to without a usable one (M1 §10) has none and cannot get one, and a root
made elsewhere may not either. So `MountTable::mount` also accepts a path
whose last name does not exist in its parent directory. Such a mount point
shows up as a directory in the parent's `read_dir` and resolves like any
other name, and the name behaves as a mount point does: `create` and `mkdir`
of it are `EEXIST`, `unlink` is `EISDIR`, and `rmdir` and `rename` from or to
it are `EBUSY`.

## 5. Processes and memory

### 5.1 Address spaces

Every process has its own PML4. Entries 0–255 (the lower half) belong to the
process; the kernel's entries (256–511: the linear map, the heap, the
kernel stacks and the kernel image) are copied in when the address space
is created, so the kernel is mapped in every process. None of them changes
after the kernel has set up its memory (§16 item 2).

| Range | Use |
|---|---|
| `0x0000_0000_0000_0000`–`0x0000_0000_003F_FFFF` | never mapped (null pointers fault) |
| `0x0000_0000_0040_0000`–`0x0000_0FFF_FFFF_FFFF` | the program's ELF segments |
| `0x0000_1000_0000_0000`–`0x0000_6FFF_FFFF_FFFF` | `mem_map` area, first fit, growing up |
| `0x0000_7FFF_FFEF_F000`–`0x0000_7FFF_FFEF_FFFF` | stack guard page (never mapped) |
| `0x0000_7FFF_FFF0_0000`–`0x0000_7FFF_FFFF_EFFF` | user stack (1 MiB less one page) |
| `0x0000_7FFF_FFFF_F000`–`0x0000_7FFF_FFFF_FFFF` | never mapped (keeps every user address canonical with room to spare) |

- Memory is allocated and zeroed when it is mapped (`spawn`, `mem_map`).
  There is no demand paging, so every user page fault is a real fault
  (§11.1).
- Page permissions come from the ELF flags: code read + execute, read-only
  data read-only NX, data read-write NX. A segment that is both writable and
  executable is refused. The stack and `mem_map` memory are read-write NX.
- SMEP and SMAP are enabled when CPUID reports them (the i7-1260P does;
  QEMU with KVM passes them through; TCG with `-cpu max` has them too). The
  kernel never touches a program's addresses: `UserSlice` and `UserStr` copy
  through the linear map, from the frames the program's page tables name
  (§16 item 2), so no copy needs `stac` and `clac`.
- `kernel/src/mm/paging.rs` gains a user bit, the permissions above,
  `unmap`, and freeing a whole lower half with its page tables and frames.
  Its host tests (fake `PhysMem`) check that tearing down an address space
  frees every frame it allocated.

### 5.2 Program loading

`spawn` loads a program from any path the VFS reaches (usually `/bin`). The
ELF must be:

- ELF64, little-endian, `EM_X86_64`, `ET_EXEC`, at most 16 MiB;
- only `PT_LOAD`, `PT_NOTE` and `PT_GNU_STACK` program headers;
- every `PT_LOAD` inside `0x40_0000`–`0x0000_0FFF_FFFF_FFFF`, not
  overlapping another, with `p_offset` and `p_vaddr` congruent modulo 4 KiB
  and `p_filesz ≤ p_memsz`;
- the entry point inside an executable segment;
- a `PT_NOTE` with name `Relay`, type 1 and a 4-byte descriptor equal to
  `relay_abi::VERSION`.

Anything else is `ENOEXEC`. The checks are a host-tested function over the
file's bytes, with randomized inputs. The logic of `boot/src/elf.rs` is
reused where it fits.

### 5.3 Entry state

A new process starts at the ELF entry point in ring 3 with interrupts
enabled and:

- `rdi` = the address of its argument bytes, `rsi` = their length, `rdx` =
  the number of arguments. The arguments are copied to the top of the
  stack, each followed by a NUL; argument 0 is what the caller gives (the
  shells give the command's name as typed, as bash does; §16 item 3). At
  most 64 KiB of arguments (`E2BIG` otherwise, a new error number).
- `rsp` 16-byte aligned below the arguments; every other register zero.
- fds 0–2 as mapped by `SpawnArgs` (§7.3), the current directory from
  `SpawnArgs`.

### 5.4 The process

| Field | Contents |
|---|---|
| `pid` | `u32`, from 1, increasing, not reused while the kernel runs |
| `ppid`, `pgid` | parent and process group (§6.4) |
| `state` | ready, running, blocked (on what), zombie |
| address space | PML4 and the list of user mappings |
| kernel stack | 64 KiB with an unmapped guard page, in a kernel-stack area in the upper half with one slot per process-table entry |
| saved context | callee-saved registers and the kernel stack pointer while switched out |
| fd table | 32 slots, each a shared reference to an open file (a VFS node and offset, the console, or a pipe end) |
| current directory | a path; `MountTable`'s current directory is switched per process |
| name, arguments | for `ps` |
| frames, CPU ticks | for `ps` and `free` |
| exit status | `Exited(code)` or `Killed(reason)`, reason one of fault (with its kind and address), Ctrl-C, `kill` |
| console tees | the tee targets it pushed (§6.5) |

The process table holds at most 64 processes (`EAGAIN` beyond). When a
process ends, its memory, fds and tees are released at once; the entry stays
as a zombie until its parent `wait`s for it. The children of a process that
ends pass to process 1.

### 5.5 FPU and SSE

User programs are built for `x86_64-unknown-none`, which uses no floating
point or SIMD registers, and the kernel saves no FPU state. So that a
program cannot corrupt or read another's registers, CR0.TS stays set while
user code runs: any FPU/SSE instruction raises #NM and the process is
killed (`killed (FPU/SSE instruction, ip …)`). Supporting floating point
(saving state with XSAVE) is out of scope (§15).

## 6. Scheduling, blocking and the console

### 6.1 The scheduler

- Round-robin over the ready processes. A time slice is 10 ticks of the
  1 kHz timer (M1 §4.4 step 5).
- The kernel is not preemptible. A tick that interrupts ring 3 may switch to
  the next ready process when the slice is used up. A tick that interrupts
  the kernel only sets `need_resched`, which is acted on when the system
  call returns to ring 3 or blocks.
- A process blocks in exactly these places: a console read with no input,
  `wait` with no finished child, `sleep`, and (milestone 3) a pipe read on an
  empty pipe or a pipe write on a full one. Blocking saves the kernel
  context on the process's kernel stack and switches.
- **The idle task** (process 0, kernel only) runs when nothing is ready: the
  input poll of today's `KernelConsole::poll`, `usb::service`, waking
  sleepers whose time has come, then `hlt`. Sleepers are also woken
  whenever the running process's ticks are counted (§16 item 3), so a
  program that never blocks does not keep a sleeper asleep.

### 6.2 System-call entry (x86_64)

- The GDT gains user code and data segments in the order `syscall`/`sysret`
  needs (kernel code, kernel data, user data, user code). The TSS gains
  `rsp0`, set to the running process's kernel stack on every switch.
- `STAR`, `LSTAR` and `SFMASK` are set up; `SFMASK` clears IF, DF, TF, AC
  and NT on entry (§16 item 2). The entry stub switches to the kernel stack through a
  per-CPU block reached with `swapgs`, saves the user registers, re-enables
  interrupts and calls the architecture-neutral dispatcher.
- Before returning with `sysret` the stub checks that the user `rip` is
  canonical and uses `iretq` otherwise (a non-canonical `rcx` makes `sysret`
  fault in ring 0 on Intel CPUs).
- Interrupts and exceptions from ring 3 enter through the IDT with the TSS
  `rsp0` stack; their stubs swap to the kernel's `gs` and load the
  kernel's flags first (§16 item 3).

### 6.3 Polling input while programs run

A program that never makes a system call must not freeze the keyboard. Every
tick that interrupts ring 3 therefore also runs the input poll (the USB and
COM1 part of `KernelConsole::poll`; `usb::service` stays in the idle task).
This is safe because the kernel holds no borrowed state while user code
runs. §14 covers its cost.

### 6.4 Foreground and Ctrl-C

- Every process belongs to a process group (`pgid`). `spawn` either puts
  the child in its parent's group or starts a new group with the child's
  pid (§7.3). The interactive shell starts every command in a new group
  (in milestone 3, one group for all stages of a pipeline). A shell running
  a script starts the script's commands in its own group, so Ctrl-C ends the
  script together with its command, as in M1 §15 item 12.
- The console has one **foreground group**, set with `console_foreground`.
  Only processes in it read the console; others get end of input at once
  (milestone 3's background jobs, §9.2). Only a group the console was
  handed to, through the groups that held it since process 1, may change
  its group or mode (§16 item 9).
- The console is in **raw** or **line** mode (§6.5). In line mode, Ctrl-C
  kills every process of the foreground group (`Killed(CtrlC)`) and drops
  the typed input before it, as M1 §15 item 12 does. In raw mode Ctrl-C is
  delivered as byte 0x03, so the shell's line editor cancels the line at its
  prompt exactly as today.
- The interactive shell sets raw mode and makes its own group the
  foreground at its prompt, and sets line mode and the command's group
  before it waits for a command.

### 6.5 Console modes and tees

- **Raw mode** returns bytes as they are typed, editing keys as escape
  sequences (M1 §7.2), with no echo.
- **Line mode** is a minimal line discipline: typed characters are echoed,
  Backspace removes the last character of the line, Enter ends the line
  (the reader gets it with a `\n`), Ctrl-D on an empty line is end of input,
  and Ctrl-C is handled as in §6.4. A read returns at most one line.
- **Tees** copy console output into files: everything written to the
  console by any process (the shell's messages about killed commands
  included) also goes to every file on the tee stack (at most 4 a process,
  64 in all, §16 item 9). `console_tee_push`
  adds a file the process has open for writing; `console_tee_pop` removes
  the newest tee the process pushed; a process's tees are popped when it
  ends. Tee output is buffered and written every 4 KiB and at every `sync`,
  so a big `cat` is never held whole (as M1 §15 item 12). A write error
  removes that tee and is logged; the next `console_tee_pop` of its owner
  returns `EIO`, so the shell reports it when the script ends, and the script
  goes on as in M1 §15 item 12.

  `sh FILE` pushes the script's transcript, so the transcript gets what the
  screen gets, including the output of the programs the script starts and
  of any script it runs (whose own transcript is pushed on top).

### 6.6 Process 1

Process 1 is the kernel's own ("init", §16 items 5 and 6). It prints
`/etc/motd` once (M1 §4.4 step 10) and starts `/bin/sh` in `/root` (or `/`
without one) as its child, in a process group of its own that gets the
console. If the shell ends, init says how, on the screen and in the kernel
log, and starts it again. If it ended three times within 10 s, or cannot be
started at all, init shows the error screen of §11.2 instead. Init collects
every orphan as it ends.

## 7. The Relay system-call ABI (`crates/relay-abi`)

### 7.1 Calling convention

| | x86_64 | aarch64 (later) |
|---|---|---|
| Instruction | `syscall` | `svc #0` |
| Call number | `rax` | `x8` |
| Arguments (up to 6 × `u64`) | `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9` | `x0`–`x5` |
| Result | `rax` | `x0` |
| Clobbered | `rcx`, `r11` | none besides `x0` |

A result from 0 to 2^63 − 1 is a value; −4095 to −1 is a negated error
number. Pointers are user addresses; a pointer and a length describe a byte
range. Paths and arguments are bytes (not necessarily UTF-8), as file names
are today. Structs are `#[repr(C)]` with fixed-width fields and no implicit
padding; a host test checks the size and every field offset of each.

### 7.2 Error numbers

Linux's numeric values. `vfs::Errno` gains `EFAULT` (14), `EBADF` (9),
`ENOEXEC` (8), `E2BIG` (7), `ECHILD` (10), `ESRCH` (3), `ENOMEM` (12),
`EMFILE` (24), `EAGAIN` (11), `ENOSYS` (38), `EPERM` (1), `EINTR` (4) and,
in milestone 3, `EPIPE` (32), with Linux's messages.

### 7.3 Calls

Numbers are assigned in this order from 1 and never reused; the ones marked
M3 are reserved in milestone 2 and return `ENOSYS` until milestone 3.

| Call | Arguments → result | Notes |
|---|---|---|
| `exit` | code `u8` → never returns | |
| `spawn` | `&SpawnArgs` → pid | `SpawnArgs`: path, argument bytes (NUL-separated), working directory, up to 8 `(child_fd, parent_fd)` pairs (unlisted child fds are closed), flags (`NEW_GROUP`, `FOREGROUND`), and the group of a child of the caller's to join (`pgid`, milestone 3, §16 item 8) |
| `wait` | pid or −1 for any child, flags (`NOHANG`; `CTRL_C`, milestone 3), `&mut WaitStatus` → pid, or 0 with `NOHANG` and nothing finished | `WaitStatus`: `Exited(code)` or `Killed(reason, fault kind, address, ip)`; `ECHILD` with no such child; with `CTRL_C`, `EINTR` at a Ctrl-C typed while the caller's group has the console in raw mode (§16 item 9) |
| `kill` | pid, or −pgid for a group → 0 | `ESRCH`; `EPERM` for process 1 |
| `getpid` | → pid | |
| `proc_list` | buffer → count | M3: `ProcInfo` per process, by pid, as many as fit: pid, ppid, pgid, state (what a blocked process waits for), frames, CPU ticks, name (up to 64 bytes); the count is of all processes (§16 item 9) |
| `mem_map` | length → address | length rounded up to pages; `ENOMEM` (§11.1) |
| `mem_unmap` | address, length → 0 | whole pages of earlier `mem_map`s only; `EINVAL` otherwise |
| `open` | path, flags → fd | flags: `READ`, `WRITE`, `CREATE`, `TRUNCATE`, `APPEND`, `EXCLUSIVE`, `DIRECTORY` |
| `close` | fd → 0 | |
| `read`, `write` | fd, buffer → bytes | |
| `seek` | fd, offset `i64`, whence (start, current, end) → new offset | on the console or a pipe: `EINVAL` |
| `fstat` | fd, `&mut Stat` → 0 | `Stat::dev` is the file's filesystem, the mount's number from 1, and 0 for the console and pipes (§16 items 5 and 8) |
| `stat` | path, `NOFOLLOW` flag, `&mut Stat` → 0 | |
| `read_dir` | directory fd, buffer → bytes | packed `DirEntry` records (inode, type, name length, name), continuing where the last call stopped |
| `mkdir`, `rmdir`, `unlink`, `truncate`, `touch`, `readlink` | as the `Vfs` trait | one call per `Vfs` operation |
| `rename` | from, to → 0 | |
| `statfs` | path, `&mut StatFs` → 0 | |
| `sync` | → 0 | syncs every filesystem and every tee |
| `chdir`, `getcwd` | path / buffer | |
| `console_mode` | raw or line → previous mode | `EPERM` unless the caller's group holds the console (milestone 3, §16 item 9) |
| `console_size` | → columns and rows | |
| `console_foreground` | pgid → 0 | `EPERM` unless the caller's group holds the console (milestone 3, §16 item 9) |
| `console_tee_push`, `console_tee_pop` | fd / none → 0 | §6.5 |
| `time` | `&mut Time` → 0 | Unix seconds and uptime in nanoseconds |
| `sleep` | milliseconds → 0 | |
| `sys_info` | kind, buffer → bytes | kinds: memory (`MemInfo` of M1's `free`), `uname` fields, kernel log (for `dmesg`) |
| `power` | reboot or poweroff → error only | the kernel syncs and shuts the filesystems down first (M1 §7.4); when that fails it returns the error and the machine stays up, as M1's `unplug` scenario requires |
| `pipe` | `&mut [u32; 2]` → 0 | M3 (§9.1): the read end first |

`read` and `write` on the console and on pipes may return fewer bytes than
asked. The kernel reaches the files through the same `MountTable` the
in-kernel shell uses today; there is one table, shared by all processes,
with the current directory switched per process (§5.4).

### 7.4 Versioning

`relay_abi::VERSION` (`u32`, 2 for 0.3.0, §16 item 5, and 3 from milestone 3's
plan 1, §16 item 8) changes whenever a call's meaning
or a struct's layout changes; adding a call does not change it. It is
written into `system.img`'s header (§4.2) and into every program's ELF note
(§5.2); the kernel refuses a mismatch in either.

## 8. Userland

### 8.1 `crates/relay-rt`

The runtime every program links:

- `_start`: turns the entry registers (§5.3) into an argument list, calls
  the program's `main(args) -> u8`, and exits with its result.
- Safe wrappers for every call of §7.3; the `asm!` for the system-call
  instruction is in its `arch` module, the only architecture-specific part.
- A `#[global_allocator]` from `crates/heap` over memory from `mem_map`,
  growing in 1 MiB steps. When it cannot grow, the program prints
  `<name>: out of memory` and exits with 134.
- A panic handler that prints `<name>: panicked at <location>: <message>`
  on fd 2 and exits with 101.
- `SysVfs`, `SysConsole` and `SysSystem`, implementing `vfs::Vfs`,
  `shell::Console` and `shell::System` over system calls, so a command
  function runs unchanged in a program. `Console::interrupted` returns
  false, since Ctrl-C now kills the process.
- A write to fd 1 that fails with `EPIPE` ends the program quietly with 141
  (milestone 3, §9.1).
- The ELF note (§5.2) and the user linker script, which `build.rs` of each
  program passes with `cargo:rustc-link-arg-bins`, as `kernel/build.rs`
  does.

### 8.2 `crates/shell` after the move

The crate keeps its line editor, parser, command functions and their tests.
It gains a `Runner` trait with two implementations:

- **In-process**, as today: the command functions run against a `Vfs`,
  `Console` and `System`. Used by the unit tests (with the in-memory `Vfs`)
  and by `cargo xtask host-shell`.
- **Spawning** `/bin/<name>`: used by `/bin/sh`. A name that is not a
  built-in and not in `/bin` is `relay-sh: <name>: command not found` (status
  127); a path containing `/` is run as given.

Redirections (`>`, `>>`) open the file in the shell and pass it to the child
as fd 1 (M1 §7.3: standard error is never redirected).

### 8.3 `/bin/sh` (`userland/sh`)

- The interactive loop of today's shell over `relay-rt`, with the prompt
  `root@relay:<cwd># `.
- Built-ins: `cd`, `exit [code]`, `help` (lists the built-ins and every
  command with its help text), and, in milestone 3, `jobs`, `wait`, `kill`
  (§9.2, §9.3).
- After every foreground command it calls `sync`, as today (M1 §8.3).
- `sh FILE [args]` runs a script: `/bin/sh` started as a child with the
  script's path. It keeps M1 §15 item 12's rules (trace lines, transcript,
  64 KiB limit, BOM and CRLF, a failing line does not stop the script,
  Ctrl-C and `reboot`/`poweroff` do), except that one script may now run
  another. The transcript is pushed as a console tee (§6.5).

### 8.4 Programs (`userland/utils`)

One `[[bin]]` per command, each a thin `main` over the existing command
function: `cat`, `clear`, `cp`, `date`, `df`, `dmesg`, `echo`, `free`,
`head`, `ls`, `mkdir`, `mv`, `poweroff`, `pwd`, `reboot`, `rm`, `rmdir`,
`stat`, `sync`, `tail`, `touch`, `uname`, `wc`. Milestone 3 adds `grep`,
`seq`, `sleep`, `true`, `false` and `ps` (§9, §16 item 8). Every command prints exactly
what it prints in milestone 1. They are built with the `relay` profile plus
LTO.

### 8.5 Test programs (`userland/tests`)

Shipped in every `system.img`, because the NUC checks use them too:

| Program | Does |
|---|---|
| `t-fault KIND` | faults on purpose: `null-read`, `null-write`, `write-code`, `exec-data`, `ud`, `div0`, `stack`, `kernel-read` (reads a kernel address), `sse`; and `flags-exit`, `flags-ud`, `flags-ac`, `flags-tf` (the flags a program sets never reach the kernel) and `gsbase` (a program cannot set its own `gs` base) |
| `t-spin [secs]` | spins without system calls, forever or for `secs` seconds (reading the clock only every 2^20 iterations), then prints how many iterations it made |
| `t-spawn N` | starts N children that exit at once and waits for each; prints the free frames before and after, and how many were lost. Also `kill` (kills a spinning child while it sleeps), `kill-new` (kills one before it has run), `orphan`, `fill` (fills the process table with napping orphans), `sleepers` (a group blocked in `wait` and `sleep`, for Ctrl-C) and, in milestone 3, `join` (children in another child's group, §16 item 8) and `ctrl-c` (a wait a raw Ctrl-C ends, §16 item 9) |
| `t-abi` | a program whose ELF note has the wrong ABI version (built by xtask) |
| `t-args` | prints its arguments one per line, as `[n] <arg>` |
| `t-files KIND` | the file calls: `basic` (`open`'s flags, `read`, `write`, `seek`, `fstat`, `close`, an offset shared with a child, 32 fds), `dir` (the calls on paths and `read_dir`), `cwd` (`chdir`, `getcwd`, a child's working directory), `gone` (another process removes its working directory and open file), `full` (write errors on a full disk) |
| `t-mem KIND` | memory: `map` (`mem_map` and `mem_unmap`), `unmapped` and `unmapped-many` (a page read after it was given back is killed), `grow N` (N MiB of heap), `oom` (a child takes memory until there is none), `churn` (maps and gives back nearly all free memory over and over, in the kernel almost all the time) |
| `t-read [KIND]` | reads the console and prints each read: in line mode, `raw`, `apart` (outside the foreground group), `refused` (a group never given the console may not change it, §16 item 9), `size` (and the console calls' refusals), `leave` (a child left behind that may not set line mode) |
| `t-tee KIND` | console tees: `basic`, `end` (left by a process that ends), `gone` (its file removed), `typed` (a reader's), `full` (on a full disk) |
| `t-pipe KIND` | milestone 3's `pipe` call (§16 item 8): `basic` (the fds, a write and a read, `fstat`, what an end is not), `room` (16 KiB of a bigger write), `child` (1 MiB to a child, both blocking in turn), `eof` (the end of the data once the last writer ends), `epipe` (a write with no reader; `t-args` writing to one ends with 141), `killed` (a blocked reader and a blocked writer killed), `many` (pipes until `EMFILE`) |
| `t-sys KIND` | `sys_info`'s names (`uname`) and kernel log (`log`), and the `power` call (`poweroff` and `reboot`, each with or without `-f`) |
| `t-proc KIND` | milestone 3's `proc_list` call (§16 item 9): `list` (process 1, the shell, itself and children sleeping, ended, on a pipe and reading the console), `short` (a buffer of one entry, and of none), `long` (a name cut at 64 bytes), `end-shell` (a child kills the shell while it reads the console in line mode) |

The kinds a program runs its own children with (`t-files child`, `pwd` and
`gone-child`, `t-mem hog`, `t-read leave-child`) are left out.

## 9. Milestone 3: pipes, jobs, `ps`/`kill`, script variables

### 9.1 Pipes

- `pipe` makes a pipe with a 16 KiB ring buffer. `read` blocks while the
  pipe is empty and a writer is open, and returns 0 once all writers have
  closed. `write` blocks while it is full, and returns `EPIPE` once all
  readers have closed.
- The parser accepts `a | b | c`. The shell makes the pipes, spawns every
  stage in one new process group with the pipe ends mapped to fds 0 and 1
  (the later stages join the first one's group, `SpawnArgs::pgid`),
  closes its own copies, and waits for every stage. `$?` is the last
  stage's status. `>` and `>>` apply to the last stage (on another stage
  they are refused). A built-in in a pipeline is refused with
  `relay-sh: cd: cannot be used in a pipeline` (§16 item 8).
- `cat`, `wc`, `head` and `tail` read standard input when given no file;
  `Ctx` gains an input for that. On the console that input is line mode.
  `wc` gains `-c`, `-l` and `-w` (§16 item 8).
- New programs: `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]` (a pattern is
  literal characters, `.`, `*`, `^`, `$` and `[...]` with ranges and `^`),
  `seq [FIRST [INCREMENT]] LAST` (whole numbers), `sleep SECONDS`, `true`,
  `false` (§16 item 8).

### 9.2 Background jobs

- `cmd &` or `a | b &` starts a job (its own process group) without making
  it the foreground; the shell prints `[n] <pid of the last stage>`.
- Before each prompt the shell collects finished children with
  `wait(-1, NOHANG)` and reports each finished job as bash does,
  `[1]+  Done                    sleep 5` (or `Exit <code>`, `Killed`; the
  state padded to 24 columns, §16 item 9).
- `jobs` lists the jobs: `[1]+  Running                 sleep 5 &`.
- `wait` waits for every job; `wait %n` or `wait PID` for one, and sets `$?`.
- A background process reading the console gets end of input at once; its
  output goes to the screen.
- There is no `fg`, `bg` or Ctrl-Z.

### 9.3 `ps` and `kill`

- `/bin/ps` prints `PID PPID STATE MEM TIME CMD` (MEM in KiB, TIME as
  `m:ss` of CPU time) from `proc_list`, STATE saying what a blocked process
  waits for (§16 item 9).
- `kill PID...` or `kill %n...` (built-in, since `%n` needs the job table)
  kills processes or a whole job. Killing process 1 is refused:
  `relay-sh: kill: (1) - Operation not permitted` (§16 item 9).

### 9.4 Script arguments and variables

- `sh FILE a b` sets `$0` (the script's path), `$1`…`$9` (and `${10}` on),
  `$#` and `$@` (all arguments, each one word). `$?` is the status of the
  last command or pipeline (§16 item 10).
- `NAME=value` alone on a line sets a shell variable (a name is
  `[A-Za-z_][A-Za-z0-9_]*`); `$NAME` and `${NAME}` read it; an unset
  variable reads as empty. Variables work at the prompt too.
- Expansion happens outside quotes and inside double quotes, not inside
  single quotes. Its result is never split into words: a value with spaces
  stays one argument (unlike bash). An unquoted expansion that is empty
  produces no word; a quoted one produces an empty word.
- Variables belong to the shell; programs get no environment.
- Still refused: `` ` ``, `$(`, `$((`, and bash's other parameters and
  operators (`$*`, `$$`, `$!`, `$-`, `${NAME:-x}`, …) and `NAME=value cmd`
  (§16 item 10). Not supported: `&&`, `||`, `if`, loops, `export`.

## 10. Tooling (`xtask`)

| Command | Change |
|---|---|
| `cargo xtask build` | Also builds `userland/` for `x86_64-unknown-none` with the `relay` profile, and `t-abi` with its wrong note |
| `cargo xtask image` | Writes `system.img` to the ESP |
| `cargo xtask flash --full` | Writes `system.img` too |
| `cargo xtask flash --kernel` | Writes the loader, the kernel **and** `system.img`; still leaves the ext2 root alone |
| `cargo xtask verify-usb` | Also checks `check4.sh` and (milestone 3) `check5.sh` transcripts |
| `cargo xtask host-shell` | Unchanged (in-process runner, §8.2) |
| `cargo xtask ci`, `lint`, `unit` | Pick up the new crates automatically; `lint` also covers `userland/` |

The e2e runner (M1 §9.3) gains the steps it needs for the scenarios of
§12.3, for example sending Ctrl-C while a program runs, which the existing
`key` step already covers.

## 11. Error handling

### 11.1 Programs

- **Faults in ring 3** (page fault, #GP, #UD, #DE, #NM, a stack overflow
  into the guard page, and any other exception) kill only that process with
  `Killed(Fault)`. The kernel logs the process, the kind, the address and
  `rip`; the shell prints
  `relay-sh: <command>: killed (page fault at 0x0, read, ip 0x401a2c)`, and
  `$?` is 128 + 11 for page faults and #GP, 128 + 4 for #UD, 128 + 8 for
  #DE and #NM (bash's numbers for SIGSEGV, SIGILL and SIGFPE); Ctrl-C gives
  128 + 2 and `kill` 128 + 9. Faults in
  ring 0 still go to the panic screen (M1 §10), including those caused by a
  bad user pointer outside a `UserSlice` copy.
- **Bad pointers** passed to a system call return `EFAULT`: `UserSlice` and
  `UserStr` check that a range is in the lower half, mapped, user-accessible
  and has the needed permission, by walking the process's tables. Nothing
  can unmap it during the call (§6.1).
- **Limits:** 64 processes (`EAGAIN`), 32 fds per process (`EMFILE`), 64 KiB
  of arguments (`E2BIG`). User memory (`spawn`, `mem_map`) is refused with
  `ENOMEM` once fewer than 8 MiB of frames would be left, so DMA buffers,
  page tables and kernel stacks never run out. An unknown call number is
  `ENOSYS`.
- **Ctrl-C and `kill`** end a process at once; its fds close and its tees
  are popped, so a half-written redirection file stays as far as it got, as
  a Ctrl-C leaves it in milestone 1.

### 11.2 The system archive and process 1

A missing, corrupt or wrong-ABI `system.img`, a `/bin/sh` that cannot be
started, or a shell that died three times within 10 s leads to an error
screen: the reason, the last lines of the kernel log, and
`Press any key to reboot.` Init shows it, in its own context (§16 item 6),
after ending every other process, so that nothing writes over it (§16
item 7). The kernel then syncs and reboots through the ACPI path of M1
§7.4. There is no in-kernel fallback shell.

### 11.3 Storage

As in milestone 1 (M1 §10). Because `/bin` is in RAM, commands still start
after the stick is unplugged; their file operations on `/` fail with `EIO`.

## 12. Testing

### 12.1 Host unit tests

- `relay-abi`: size and field offsets of every struct; numbers are unique.
- `sysimg`: write and read round trips; every refusal of §4.2.
- `paging`: user mappings with each permission, `unmap`, and teardown that
  frees every frame (fake `PhysMem`).
- ELF checks (§5.2): each rule, plus randomized headers that must never
  panic.
- `UserSlice`: ranges across the lower-half end, unmapped pages, read-only
  pages, kernel addresses.
- Process table and scheduler, through the kernel's library target: ready
  queue order, blocking and waking, zombies and `wait`, orphans passing to
  process 1, group kills, the respawn rule of §6.6.
- Line discipline (§6.5) and the tee stack.
- `vfs`: mount points missing from their parent (§4.4).
- Milestone 3: the pipe buffer (blocking, end of file, `EPIPE`), the parser
  (`|`, `&`, variables, assignments), the job table, `grep`'s matcher,
  `seq`.

The command functions keep their tests against the in-memory `Vfs`.

### 12.2 Frame accounting

`t-spawn` and `free` let scenarios compare free memory before and after, so
a leak of frames, kernel stacks or page tables shows up as a failed
scenario.

### 12.3 QEMU end-to-end scenarios

All 20 scenarios of milestone 1 pass, changed only for the new startup line.
New ones:

| Milestone | Scenario | Checks |
|---|---|---|
| 2 | `userfault` | every `t-fault` kind is killed with its message and `$?`; the shell carries on; `free` shows the same free memory before and after |
| 2 | `ctrlc` | `t-spin`, then Ctrl-C through QMP: killed, `$?` 130, the prompt returns; `t-spin 2` ends by itself |
| 2 | `spawn` | `t-spawn 1000` reports the same free frames before and after; `t-args a 'b c' ''` prints three arguments |
| 2 | `system_missing` | no `system.img`: `[FAIL] system: …` and the error screen |
| 2 | `system_abi` | an archive with another ABI version: the error screen names both versions |
| 2 | `system_empty` | an empty `system.img`: `system.img: only 0 bytes`, not `no system.img` |
| 2 | `stale_program` | `t-abi` copied to `/root`: running it prints `Exec format error` |
| 2 | `respawn` | `exit` restarts the shell with a log line; three quick exits reach the error screen |
| 2 | `system_nosh` | a `system.img` without `sh`: the error screen says `/bin/sh cannot start` (§16 item 7) |
| 2 | `utils` | every program of `/bin` prints what the shell's command of its name prints (§16 item 5) |
| 2 | `sh` | `/bin/sh` runs its commands as programs, redirections into files, nested scripts with their transcripts (§16 item 5) |
| 3 | `pipe_calls` | `t-pipe`: the `pipe` call's reads, writes, ends and refusals, blocking both ways, a blocked reader and writer killed, `EPIPE` and the quiet 141, the fds running out; `free` the same before and after (§16 item 8) |
| 3 | `pipes` | `cat` of the 8 MiB file through `wc -c` gives the exact size; `cat big \| head -n 1` ends at once; `seq 5 \| grep -c .` prints 5 |
| 3 | `proc_calls` | `t-proc`: `proc_list`'s entries and their states, a short buffer, a long name (§16 item 9) |
| 3 | `screen_console` | the shell ends a third time while its command holds the console in line mode: the error screen takes it back, and a key restarts the machine (§16 item 9) |
| 3 | `jobs` | `sleep 5 &` and `t-spin &`; `jobs` and `ps` show both; `kill %2`; `wait`; the Done lines |
| 3 | `script_vars` | a script run with arguments prints `$0`, `$1`, `$#`, `$@`, `$?` and variables, with and without quotes |

### 12.4 NUC checks

- `check3-a.sh` and `check3-b.sh` pass as they are, apart from the new
  startup line in their `dmesg` expectations, now with every command in user
  space.
- `check4.sh` (milestone 2): the `t-fault` kinds, `t-spawn`, `free` before
  and after, `t-args`, and a script that runs another. Two manual steps
  follow: `exit` at the prompt, which init answers with a new shell
  (§16 item 6; `t-spin` and Ctrl-C are one of check 3's steps by hand),
  and, after a `reboot`, three quick `exit`s, which reach the error screen,
  where a key on the K120 restarts the machine (§16 item 7).
- `check5.sh` (milestone 3): pipes, a background job and `kill`, script
  arguments and variables.
- `cargo xtask verify-usb` checks their transcripts, as in M1 §15 item 12.

### 12.5 CI

Unchanged jobs (`lint`, `unit`, `e2e`); the new crates, programs and
scenarios are picked up by `cargo xtask ci`.

## 13. Build order

Each milestone's roadmap is written when it starts, from the code that
exists then. The expected plans:

**Milestone 2**

1. **Toolchain and archive.** `relay-abi`, `heap` moved out of the kernel,
   `relay-rt` with its first program `t-args` built on stable, `sysimg`, the
   loader loading `system.img`, `/bin` mounted (§4.4). The in-kernel shell
   can `ls /bin`. This plan proves the user-program build first (§14).
2. **Ring 3 and one program.** User segments, TSS `rsp0`, system-call
   entry, per-process address spaces, the ELF checks, `UserSlice`, `exit`,
   `write`, `spawn` and `wait` for one child at a time. The in-kernel shell
   runs a name it does not know from `/bin` and waits for it.
3. **Processes and scheduling**, in two plans (§16 item 3):
   - **3a.** Process table, scheduler, blocking, the idle task, polling on
     ticks, fault kills, groups, foreground and Ctrl-C, `spawn`, `wait`,
     `kill`, `getpid`, `time`, `sleep` and `sys_info`'s memory figures from
     ring 3, SMEP/SMAP and CR0.TS. NUC check 3.
   - **3b.** The file, memory and remaining system calls, `relay-rt`'s
     allocator, the console's line discipline and calls, and tees.
4. **The shell moves out**, in two plans (§16 item 5):
   - **4a.** `relay-rt`'s trait implementations, the `Runner` split,
     `/bin/sh` and the programs, run by path from the in-kernel shell.
   - **4b.** Process 1 and its respawn rule, the error screen; the kernel
     drops `shell`; `check4.sh`. NUC check 4.
5. **Hardening and 0.3.0.** Fixes from the NUC checks, the spec's §16 up to
   date, version 0.3.0.

**Milestone 3** (its roadmap names these plans `m3-plan-1` to `m3-plan-4`)

6. **Pipes and standard input.** `pipe`, the parser, the stdin readers,
   `grep`, `seq`, `sleep`, `true`, `false`.
7. **Jobs, `ps` and `kill`.** (The cut line of §1.2.)
8. **Script arguments and variables.**
9. **Hardening and 0.4.0.** `check5.sh`, NUC check 5, version 0.4.0.

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Building `no_std` user programs with our own linker script and ELF note on stable Rust turns out awkward | Plan 1 builds `t-args` and checks it with `readelf` before anything depends on it; plan 2 runs it first |
| `sysret` with a non-canonical return address faults in ring 0 (Intel) | The check of §6.2; `t-fault` covers it indirectly, a unit test covers the check |
| SMAP makes a forgotten user access fault in the kernel | That is the point: it shows up as a panic in a scenario instead of silently. `UserSlice` is the only path. |
| Polling input on every ring-3 tick costs too much | `t-spin`'s loop count on the NUC, in check 3's and check 4's transcripts, and plan 5's count with a kernel built without the poll: 4594860032 iterations in 1 s with the poll, 4677697536 without it (1.8 %, about 18 µs a tick), so the poll stays on every tick (§16 item 7); if needed, poll every 4th tick |
| QEMU TCG lacks SMEP/SMAP, so a SMAP bug shows only with KVM | CI's e2e job runs with KVM, as today; `RELAY_QEMU_ACCEL=tcg` stays a local convenience. (Plan 3a found that TCG with the runner's `-cpu max` has both; the `panic_smap` and `panic_smep` scenarios pass under either.) |
| The gate does not fit two months | Background jobs are the cut line (§1.2); the other milestone 3 items do not depend on them |
| Programs with floating point are wanted later | CR0.TS kills them clearly today (§5.5); XSAVE support is a contained later change |

## 15. Out of scope for this gate

- `fork`, threads, shared memory, `mmap` of files, dynamic linking.
- Signals other than killing; `fg`, `bg`, Ctrl-Z, stopped processes.
- Floating point and SIMD in user programs.
- Environment variables, `export`, command substitution, arithmetic,
  `&&`/`||`, `if`, loops, globbing, `<` redirection.
- Users, permissions, `setuid`; SMP; interrupt-driven I/O.
- Networking, a UI, the aarch64 port (designed for, not built: §3.1, §7.1).
- Everything else M1 §14 lists that this document does not add.

## 16. Revisions made during planning

Changes made while planning either milestone are recorded here, as M1 §15
does. Facts found before the spec was first merged are already in its body.

1. **Decisions made while planning milestone 2's plan 1** (toolchain and
   archive):
   - **Code model** (§8.1, §8.4). User programs use the target's default
     code model (`kernel`): the prebuilt `core` for `x86_64-unknown-none`
     is compiled for it, LTO needs one model throughout, and its
     sign-extended 32-bit addresses reach everything a program linked at
     `0x40_0000` holds below 2 GiB. A program past 2 GiB fails to link
     ("relocation truncated"), never silently. They need no `RUSTFLAGS` of
     their own; xtask builds them in `target/user`, so a build from inside
     `cargo test` never waits for the outer build's lock.
   - **The `user` profile** (§8.4) is `relay` (overflow checks and debug
     assertions on) plus LTO, stripped of debug info: `t-args` is about
     20 KiB. The user packages are listed in `xtask`'s `USER_PACKAGES` and
     are not default members; `cargo xtask lint` clippies each for
     `x86_64-unknown-none`.
   - **Checks at build time** (§5.2, §10). xtask checks every program with
     binutils' `readelf` (`LC_ALL=C`), independently of our own code, before
     it packs `system.img`: an x86_64 `EXEC` with only `PT_LOAD`, `PT_NOTE`
     and `PT_GNU_STACK` program headers; loadable segments inside
     `0x40_0000`–`0x1000_0000_0000`, not overlapping (by page), with offsets
     and addresses congruent modulo 4 KiB, no more file than memory, never
     writable and executable; the entry point in an executable segment; and
     a `PT_NOTE` segment (not merely a section) holding the `Relay` note of
     type 1 with this ABI's version. A missing `readelf` fails the build
     naming binutils. The kernel's own checks come with plan 2.
   - **Moved crates** (§3). The heap allocator is `crates/heap` (the kernel
     keeps `KernelHeap`, the locked `#[global_allocator]`), and CRC-32 is
     `crates/crc32`, shared by the kernel's GPT code and `sysimg`.
   - **The first program** is `t-args` (§8.5), which plan 2 runs first.
   - **Plan 1 keeps the in-kernel shell** (§11.2). A missing, empty, damaged
     or wrong-ABI archive gives `[FAIL] system: <reason>` and the shell runs
     on with an empty `/bin`; the error screen comes with plan 4, when the
     shell leaves the kernel. `mount_fail`'s empty root now lists `bin`.
   - **The e2e runner** (§12.3) gains the before-boot steps `esp-delete
     <path>` and `system-abi <n>` (the same programs under ABI `n`).
   - **Check scripts** (§12.4). `check3-a.sh` expects the `system` line and
     runs `ls -l /bin`, which must list `t-args`; the recorded transcripts of
     it in `xtask/fixtures/checks/` get those lines by hand until plan 1's
     NUC check records real ones.
2. **Decisions made while planning milestone 2's plan 2** (ring 3 and one
   program):
   - **The kernel's whole upper half is shared** (§5.1, corrected in its
     body). PML4 entries 256–384 hold the linear map (`PHYS_OFFSET`, up to
     64 TiB) and the heap, which system calls need as much as the kernel
     image; every address space copies entries 256–511. `mm::init` gives
     every upper-half entry a table (about 1 MiB), so the kernel half never
     gains a PML4 entry later and a copy made once sees every later kernel
     mapping (`map_mmio`, kernel stacks).
   - **A program's flags stay its own** (§6.2, corrected in its body; found
     by the prototype's review). A program sets any flag `popfq` allows, and
     neither `syscall` nor an exception clears all of them. `SFMASK` also
     clears NT, which would otherwise make the next `iretq` into a program
     fault in ring 0 once an `exit` carried it back to the kernel, as Linux
     found in 2014; the way back to the waiting kernel (`leave`) loads the
     kernel's own flags, so none of a program's flags reach the code that
     waited for it; and a debug assertion checks that no TF, DF, AC or NT
     came back. Interrupts and exceptions taken in ring 3 still run their
     handlers with the program's AC (their gates clear only IF, TF, NT and
     RF, and the stubs clear DF): harmless without SMAP, but when plan 3
     turns SMAP on, AC would switch it off in those handlers, so each
     entry stub must then clear AC first (`clac`, only on a CPU that has
     SMAP, where it exists). The `sysenter` MSRs are zeroed, so `sysenter`
     is a general protection fault whatever the firmware left in them.
     (Corrected after the final review: the plan's decision said a
     program's AC never reaches ring 0.)
   - **A return to a non-canonical address** (§6.2). A system call whose
     return address is not canonical kills the program as a general
     protection fault at that address, instead of returning with `iretq`:
     on Intel CPUs `iretq` to a non-canonical `rip` faults in ring 0 as
     well, only on the kernel stack. The top user page is never mapped
     (§5.1), so no program reaches this; `is_canonical` is unit-tested.
   - **`UserSlice` copies through the linear map** (§5.1, §11.1). After the
     walk of the program's tables it copies from the frames they name, so
     the kernel never touches a program's addresses and SMAP (plan 3) needs
     no `stac`/`clac`. Plan 2 only copies in (`write`); copying out and
     `UserStr` come with plan 3's first calls that need them. `write`
     copies a page at a time, so a buffer that runs into a page the program
     does not have writes every byte before it (as Linux does).
   - **The ELF checks are `crates/elf`** (§3.1, §5.2), shared by the
     kernel's `spawn` and xtask's build as `sysimg` is, so one test runs
     every patched `t-args` through both and the build refuses exactly
     what the kernel refuses; `readelf` stays the independent check. Both
     also check ELF64, little-endian and at most 16 MiB (plan 1's deferred
     finding). OS/ABI and the ident version are not rules.
   - **Scheduler scope: none in plan 2** (§5.4, §6.1). One child at a time:
     `spawn` loads it (`EAGAIN` while one exists), `wait` runs it to
     completion on its own kernel stack, saving the waiting kernel's
     callee-saved registers and stack pointer (`arch::user::enter`, and
     `leave` back); the timer only counts ticks meanwhile. There is no
     preemption, blocking, idle task or Ctrl-C yet, so a program that
     never ends hangs the shell until plan 3. `spawn` and `wait` are kernel
     functions the in-kernel shell uses; from ring 3 every call but `exit`
     and `write` is `ENOSYS` until plan 3 brings the process table, when a
     parent can block. Plan 3 turns `enter`/`leave` into the context switch.
   - **The in-kernel shell's hook** (§8.2). `shell::System` gains `spawn`
     (reads the program through the `Vfs`) and `wait` (runs it, its fd 1
     going to the command's standard output and fd 2 to the screen, a
     running script's transcript getting both); both default to "no
     programs", as on the host. A name that is no built-in runs
     `/bin/<name>`, a name with a `/` runs as given, argument 0 is the path
     as typed. Not found is `relay-sh: <name>: command not found` (127), a
     missing path `No such file or directory` (127), any other refusal
     `relay-sh: <name>: <message>` (126), as bash. `..`, `.` and an empty
     name are directories in `/bin`, and a search for a command skips
     directories: they are `command not found`, as in milestone 1. A
     program's write error is reported as a built-in's; a program that was
     killed keeps its report and status next to it. Plan 4's `Runner`
     replaces the hook, and the kernel drops it with the `shell` crate.
   - **Kernel stacks** (§5.4) are in PML4 entry 509: 64 slots, each an
     unmapped guard page and 64 KiB. Their page tables stay once made, so
     frame comparisons in scenarios start after a first program.
   - **Faults** (§11.1). Exceptions 2, 8 and 18 (NMI, double fault, machine
     check) are the machine's, not the program's, and reach the panic
     screen; NMIs and machine checks now use the double fault's IST stack,
     so one taken with a program's stack pointer in ring 0 (between
     `syscall` and the switch, or before `sysret`) still does. Every other
     exception in ring 3 kills the program. A page fault in the stack's
     guard page is a stack overflow (`stack overflow at <address>`); a
     stack-segment fault counts as a general protection fault; any other
     exception is `CPU exception <n>`, status 139. The words are
     `relay_abi::WaitStatus`'s `Display`, so the kernel log
     (`pid <n> (<path>): killed: …`), the in-kernel shell and plan 4's
     `/bin/sh` say the same; the shell adds `killed (…)` and bash's status.
     Plan 2's in-kernel shell names itself `relay-sh`, as in milestone 1.
     Every other exception is status 139, although bash would give some of
     them another signal's (a debug exception SIGTRAP): the kinds are the
     ABI's, and the shell does not read architecture numbers. A refusal of
     a program (`ENOEXEC`) always has its reason in the kernel log, a file
     over 16 MiB too, which is refused before it is read.
   - **Limits** (§11.1). The 8 MiB reserve applies to every frame of a
     program's address space, its page tables included, not to kernel
     stacks. An argument holding a NUL is `EINVAL`.
   - **Error numbers** (§7.2). `vfs::Errno` gains the numbers plan 2
     returns (`E2BIG`, `ENOEXEC`, `EBADF`, `ECHILD`, `EAGAIN`, `ENOMEM`,
     `EFAULT`, `ENOSYS`) and `number()`, checked against the host C
     library's messages; the others come with the plans that return them.
   - **Tests** (§8.5, §12.3). `t-fault` has every kind but `sse`, which
     needs plan 3's CR0.TS, and two more: `flags-exit` and `flags-ud` set
     NT, AC and DF, then exit or fault. The e2e runner gains `expect-same NAME REGEX`
     (the regex's group must capture what it did the first time), used to
     compare `free` before and after. Scenarios `programs` (arguments,
     paths, redirection, refusals) and `userfault` (every `t-fault` kind,
     and a program after each); plan 3 adds `sse` to `userfault`. The
     scenarios check the messages, not `$?` (§12.3): no shell of milestone
     2 prints it, since shell variables are milestone 3's (§9.4); the
     statuses are host-tested.
   - **Check scripts** (§12.4). `check3-a.sh` runs `t-args a 'b c' ''` and
     `t-fault null-read`; the recorded transcripts get those lines by hand
     until plan 2's NUC check records real ones (their `ls -l /bin` still
     lists only `t-args`, which the script's `...` lines allow).
   - **The stick's root is 2 GiB** (M1 §4.1, added at the owner's request
     after plan 2's NUC check). `flash --full` makes the ext2 root 2 GiB
     instead of the rest of the stick, and leaves the rest unused: ext2
     has no lazy inode-table initialisation and a USB stick takes no
     discard, so `mke2fs` writes every inode table over USB, about 32 MiB
     for 2 GiB instead of 230 MiB for the whole stick, and the check after
     it reads them all back. The images xtask builds keep a root that is
     the rest of the image. The startup line becomes `mount /: ext2 on
     00:14.0 port 15 partition 2, 2.0 GiB`, which the check scripts
     expect; the recorded transcripts are those of its NUC check.
3. **Decisions made while planning milestone 2's plan 3a** (processes and
   scheduling):
   - **Plan 3 is two plans** (§13). 3a brings the processes: the table,
     the scheduler, blocking, the idle task, polling on ticks, Ctrl-C and
     `kill`, the process calls from ring 3, SMEP/SMAP and CR0.TS, and a NUC
     check. 3b brings the file, memory and remaining calls, `relay-rt`'s
     allocator, the console's line discipline and calls, and tees; it is
     written after 3a merges. Plans 4 and 5 keep their numbers.
   - **The in-kernel shell is process 1** (§6.6), a process without a
     program: the kernel's page tables and a kernel stack of its own. It
     blocks in `wait` while its command runs and on the console while it
     waits for a line; being kernel code, it is never preempted. The
     context the kernel booted in becomes the idle task, process 0, with
     no entry in the table. Plan 4 starts `/bin/sh` as process 1 instead.
   - **The context switch** (§5.4) saves the callee-saved registers and the
     flags on the process's own kernel stack and keeps only the stack
     pointer, by kernel-stack slot. Every switch happens in the kernel with
     interrupts off and no lock held, and points the CPU at the next
     process's page tables, TSS `rsp0` and `syscall` stack; debug
     assertions check the flags (none of TF, DF, AC, NT), that `rsp0`
     equals the `syscall` stack, and that no kernel lock is held. A new
     process's kernel stack starts with a frame whose return calls its
     function; a program's first run goes to ring 3 through
     `arch::user::enter`, which puts `exec::Entry` in the registers §5.3
     names, so that detail stays in `arch`.
   - **Every way in from ring 3 gives the kernel its own `gs` and flags**
     (§6.2, §16 item 2). The kernel always runs with the per-CPU block as
     its `gs` base; `syscall` and the interrupt and exception stubs swap
     it in when they come from ring 3 (the stubs test the interrupted
     CPL) and back out on the way back, so every kernel context has the
     same `gs` and a switch never mixes them. The stubs also load the
     kernel's flags (`push 2; popfq`) when they interrupted ring 3, on
     every CPU, instead of a `clac` that exists only with SMAP: AC, which
     an interrupt or trap gate leaves as the program set it, never reaches
     a handler (the switch's assertion caught it doing so). Debug
     assertions in both dispatchers check the `gs` and the flags.
   - **Ending and killing** (§5.4, §11.1). A process that ends gives back
     its memory and fds at once and stays a zombie, with its kernel stack,
     until its parent collects it. `kill` and Ctrl-C mark a process and
     wake it if it is blocked; it ends before its program runs another
     instruction: on its way back to ring 3 from a system call or a tick,
     when its blocking point returns, or before its first instruction if
     it had not run yet. The kernel log says `pid <n>
     (<path>): killed: Ctrl-C` or `kill`. `kill` of a zombie succeeds and
     changes nothing; a pid or group that holds process 1 is `EPERM`, and
     nothing is killed; 0 names no process (`ESRCH`).
   - **Ticks** (§6.1, §6.3). A tick that interrupts ring 3 polls the console
     (the USB keyboards and COM1), acts on a Ctrl-C, wakes the sleepers
     whose time has come and ends the slice if another process is ready. A
     tick that interrupts the kernel is only counted, and the count is
     settled when the process returns to ring 3 or gives up the CPU, where
     the sleepers are woken too: the idle task alone would never run while
     a program spins. A sleep ends on the tick after the one its time
     reaches, so it is never shorter than asked. When the timer could not
     start, nothing is preempted and `sleep` returns at once (nothing
     waits for ever).
   - **The console's mode and foreground group** (§6.4, §6.5) exist in 3a as
     far as Ctrl-C needs them. The in-kernel shell gives the console to its
     command's group in line mode while it waits, and takes it back (raw,
     its own group) after. In line mode a Ctrl-C kills the foreground group
     and drops what was typed before it; one typed before the command has
     started waits in the queue and is the command's. The shell then says
     `^C`, as when Ctrl-C stops a built-in (status 130), and a script
     stops there. Reading in line mode and the `console_*` calls come with
     3b, milestone 1's two serial findings with them.
   - **The in-kernel shell's output hook stays** (§16 item 2, until plan 4's
     tees). Each command gets fresh outputs of the shell's as its fds 1 and
     2 (its descendants inherit them), whose writes reach the waiting shell
     (the redirection, the screen, a script's transcript), and the shell's
     answer is the write's result: a program sees its redirection's write
     error from the write that flushes the file's 4 KiB buffer on. What a
     program writes to another command's outputs (an orphan of an earlier
     command) goes to the screen, never into this command's redirection.
   - **The mount table is the kernel's** (§7.3), locked for one operation at
     a time, and each process has its own current directory, put in while
     the table works for it. A removal marks as gone only the current
     directory in the table at the time.
   - **The process calls** (§7.3). Paths and working directories are at most
     4096 bytes (`ENAMETOOLONG`); `spawn`'s arguments are at least
     argument 0, each ending in its NUL (`EINVAL` otherwise); at most 8 fd
     pairs, a child fd below 32 and given once, a parent fd that is open;
     `NEW_GROUP` is the only flag. `wait`'s status memory is checked before
     a child is collected, so a bad pointer loses no child; `wait` takes a
     pid or −1 (0 and below −1 are `EINVAL`). Once pid 2^32 − 1 has been
     used, no process starts (`EAGAIN`) instead of the counter wrapping
     around. `time`'s uptime comes from
     the TSC. `sys_info` has the memory figures in 3a (`MemInfo`, a
     shorter buffer or another kind is `EINVAL` until 3b). `UserSlice`
     copies out whole or not at all; `UserStr` copies a byte string in and
     refuses a length over its limit before it reads anything.
   - **Orphans** (§5.4) pass to process 1. The in-kernel shell collects the
     ones that have ended before every command it starts and after every
     one it waited for, so their zombies never fill the table; plan 4's
     `/bin/sh` does so before every prompt.
   - **Argument 0** (§5.3, corrected in its body) is what the caller gives;
     the in-kernel shell gives the command's name as typed, as bash does,
     and a program still says its last path name in messages. A bare name
     too long for a file name is `command not found` (127), as in bash.
   - **SMEP, SMAP and CR0.TS** (§5.1, §5.5). SMEP and SMAP are on when CPUID
     reports them, and the kernel log says `cpu: SMEP on, SMAP on`, which
     the NUC's check script expects. The panic tests `panic=user-read` and
     `panic=user-exec` show that each is on (the scenarios `panic_smap` and
     `panic_smep`); QEMU's TCG with `-cpu max` has both too. CR0.TS (with
     MP) is set once at boot, since the kernel uses no floating point, so
     x87 and SSE state never passes from one program to the next.
     CR4.FSGSBASE is cleared whatever the firmware left, so a program
     cannot set its own `gs` base, which the next program would inherit
     (a switch saves no segment bases).
   - **Interrupts** (milestone 1's deferred findings). The LAPIC gets an EOI
     for any vector it has in service, 32–47 included; an unexpected vector
     is a storm at 1000 within one second, not over the whole uptime; the
     timer masks the PIC before anything can fail.
   - **Plan 2's deferred minors.** A kernel stack that cannot get frames is
     `ENOMEM` (a full process table stays `EAGAIN`); NMIs, double faults and
     machine checks have an IST stack each, so one arriving while
     another's handler runs keeps that one's frame.
   - **Error numbers** (§7.2). `vfs::Errno` gains `ESRCH`, `EPERM` and
     `EINTR` (the last never reaches a program: it ends a blocking call of
     one that was killed).
   - **Tests** (§8.5, §12.3). `t-spin [SECONDS]`; `t-spawn N` also prints
     `frames lost: <n>` for the NUC's script, `t-spawn kill` kills a
     spinning child while it sleeps, `kill-new` one before it has run,
     `orphan` leaves one behind, `fill` fills the table with napping
     orphans, `sleepers` blocks a group in `wait` and `sleep` for Ctrl-C;
     `t-fault` gains `sse`, `gsbase`, `flags-ac` (a spin through ticks with
     AC, NT and DF set) and `flags-tf` (the trap flag before a call that
     returns), and `flags-exit` sets the trap flag too. New scenarios: `ctrlc`,
     `spawn`, `panic_smap`, `panic_smep`.
   - **Check scripts** (§12.4). `check3-a.sh` runs `t-spin 1`,
     `t-spawn 100`, `t-spawn kill`, `t-fault sse`, `flags-ac` and `gsbase`,
     and expects the `cpu:` line; two steps follow by hand (typing during
     `t-spin 5`, and Ctrl-C of `t-spin`). The recorded NUC transcripts
     are those of plan 3a's NUC check.
4. **Decisions made while planning milestone 2's plan 3b** (files, memory
   and the console):
   - **Plan 3b is one plan** (§13), in five pull requests: this plan; the
     parts, host-tested and not used yet; the file calls; memory; the console,
     tees, `sys_info` and `power`, with a NUC check.
   - **Open files** (§5.4, §7.3). A file of the VFS open in the fd table is its
     node, what it was opened for and an offset, shared by the fds that share
     the open file (the ones `spawn` hands a child); each `open` makes its own,
     as on Linux. `open` takes the lowest free fd, and is `EMFILE` when all 32
     are in use, before anything is opened or created. Its flags' errors are
     Linux's where they apply: neither `READ` nor `WRITE`, `TRUNCATE` or
     `APPEND` without `WRITE`, `EXCLUSIVE` without `CREATE`, or `DIRECTORY`
     with `CREATE`, is `EINVAL`; `CREATE` with `EXCLUSIVE` of what exists is
     `EEXIST`; a directory opened for writing, or a name to create that ends in
     `/`, is `EISDIR`; `DIRECTORY` of anything else is `ENOTDIR`; a read-only
     filesystem says `EROFS` when something would change. Reading or writing
     what an fd was not opened for is `EBADF`, reading a directory `EISDIR`.
     Symbolic links are never followed (milestone 1): one can be opened, and
     reading it is the filesystem's `EINVAL`.
   - **`seek`** may go past the end (a write there leaves a hole); before the
     start, past 2^63 − 1 or with an unknown whence it is `EINVAL`, and so it
     is on the console and the in-kernel shell's outputs (§7.3). A directory
     only goes back to its start, where `read_dir` begins again.
   - **`read_dir`** (§7.3) writes records of a 16-byte header (inode `u64`, the
     record's length `u16`, the name's length `u16`, the kind `u8`, 3 bytes
     reserved) and the name, padded to 8 bytes, which `relay_abi::file` writes
     and reads for the kernel and the programs alike. Entries come sorted by
     name, and the continuation token is the last name given, kept in the open
     file: an entry that is there throughout comes exactly once however the
     directory changes between calls. `EINVAL` if not even the next record
     fits, 0 after the last; `ENOMEM` for a directory whose size, ten times
     over, is more than the kernel's heap has room for (its heap panics when it
     runs out; an ext2 entry of 12 bytes takes up to 112 in memory while the
     list grows); at most 64 KiB of the buffer is used per call. The kind comes
     from `Vfs::entry_kind`: a name something is mounted on, `.` and `..` are
     directories.
   - **The other calls on files** (§7.3). `stat` never follows a link, so
     `NOFOLLOW` changes nothing; `touch` sets the times of a file that exists
     (programs create files with `open`); `readlink` cuts the target at the
     buffer's length, as Linux does; `getcwd` is `ERANGE` for a buffer that is
     too short and gives a removed directory the path it had, as the in-kernel
     shell's `pwd` does. `read` and `write` copy a page at a time: a read
     checks each page writable before it reads into it, so the offset never
     moves past what the program got, and stops at the end of the file; a write
     stops where the filesystem is full, and the next piece's `ENOSPC` ends it.
     A call is not preempted (§6.1), so a big read holds the CPU while it runs.
   - **A removal reaches every process** (§5.4; the roadmap's note from plan
     3a). An inode a removal frees may go to the next file at once. The mount
     table records what each removal and move did (`vfs::Change`: a directory,
     or a file's last name, removed; a directory moved), and after every
     operation the kernel applies it: every other process's working directory
     follows (a removed one resolves nothing, a moved one takes its new path),
     and every open file and tee of a freed inode is gone, so everything but
     closing it is `ENOENT`, what the filesystem says of an inode it has freed.
     The prototype showed the danger: without it, a program wrote into the new
     file that got its removed file's inode.
   - **`mem_map` and `mem_unmap`** (§5.1, §7.3). Fresh zeroed read-write NX
     pages, the length rounded up to pages, first fit from `0x1000_0000_0000`
     to `0x7000_0000_0000`; regions that meet are one, and a process has at
     most 1024 (`ENOMEM` beyond, for an unmap that would cut one in two as
     well), so its list cannot fill the kernel's heap. A length of 0 is
     `EINVAL`; more than the frames above the 8 MiB reserve is `ENOMEM` before
     anything is mapped, and running out midway leaves nothing mapped.
     `mem_unmap` takes an aligned address and whole pages of one region
     (`EINVAL` otherwise) and flushes their TLB entries, all of them for more
     than 64 pages.
   - **The heap of `relay-rt`** (§8.1). `crates/heap` gains regions
     (`Heap::add`; regions that meet are one block). The allocator grows by the
     block's size, alignment and a slab, rounded up to whole mebibytes; when
     `mem_map` refuses, the allocator itself prints `<name>: out of memory` and
     exits with 134 (not the panic handler, whose status is 101). Nothing is
     given back to the kernel before the program ends.
   - **Reading the console** (§6.4, §6.5). Only the foreground group reads; any
     other process gets end of input at once, and a reader killed while it
     waits gets `EINTR`. In line mode the line discipline runs as keys come in
     (on the ticks that poll, in the idle task, and in reads), so what is typed
     is echoed at once, to the screen and the tees, even while no program
     reads. It works on keys: an escape sequence (`ESC [` up to its final byte)
     is one key and does nothing; `ESC` and any other byte are the Escape key
     and that key (nothing here sends Alt). Backspace erases a whole UTF-8
     character (`\b \b`); Enter is CR, LF or CR LF; Ctrl-D on an empty line is
     end of input and on another hands the line over without its `\n`, as Linux
     does; other control characters do nothing; the lines waiting and the one
     typed hold 4096 bytes together, and Enter always fits. A read gets at most
     one line. In raw mode a read gets what was typed. Going to line mode hands
     the discipline what was typed ahead, without echoing again what it echoed
     before; going back to raw mode hands back what was typed and not read, as
     bash sees it, and a Ctrl-C nobody took as a raw Ctrl-C, which the shell's
     line editor and a running script see (plan 3a's ruling that such a Ctrl-C
     was swallowed no longer holds). A LF right after the CR that ended the
     shell's line is part of that Enter, for a terminal that sends CR LF.
   - **The console calls** (§7.3). `console_mode` takes raw (0) or line (1),
     `EINVAL` otherwise, and returns the previous one; `console_size` returns
     the columns in the low and the rows in the high 32 bits (80×25 without a
     console); `console_foreground` takes a group some process is in (`ESRCH`
     otherwise, 0 too). Any process may call them (single user, §1.3); a change
     wakes the blocked readers.
   - **Tees** (§6.5). `console_tee_push` takes an fd of a VFS file open for
     writing (`EBADF` otherwise, `EINVAL` for the console and the in-kernel
     shell's outputs); the tee holds the open file, so closing the fd keeps it.
     At most 4 (`EBUSY`). A tee gets what programs and the in-kernel shell
     write to the console and the line discipline's echo, not the kernel's own
     messages. Its copies are written once 4 KiB wait, in a process's context
     (a write or a read of the console), at every `sync` (the call's and the
     in-kernel shell's after each command) and when it is popped; the echo a
     tick makes only waits, 16 KiB at most, beyond which it is dropped, as a
     terminal drops what nobody reads. A failed write removes the tee from the
     copying and is logged; the owner's next pop returns that write's error
     (`ENOSPC`, `EIO`, `ENOENT`), not always `EIO` as §6.5 says, so the shell
     reports the real reason. A pop with no tee is `EINVAL`. A process's tees
     stop at its end and are written at the next flush, since a process may end
     in a fault or a tick, with interrupts off, where no file can be written.
   - **`sys_info`** (§7.3). `INFO_UNAME` fills `relay_abi::Uname`, four fields
     of 64 bytes padded with NULs (`Relay`, `relay`, the kernel's version,
     `x86_64`), `EINVAL` for a shorter buffer; `INFO_LOG` gives the newest
     bytes of the kernel log that fit (it holds at most `LOG_MAX`, 64 KiB).
   - **`power`** (§7.3) takes reboot (1) or poweroff (2) and the flag
     `POWER_FORCE` (added: milestone 1's `reboot -f` and `poweroff -f` go ahead
     when the filesystems cannot be shut down cleanly). It writes the tees,
     shuts the filesystems down and restarts or switches off; if the shutdown
     fails it returns the error, unless forced, and the machine stays up, as
     milestone 1's `unplug` scenario requires. In test mode poweroff makes QEMU
     exit, as the shell's does.
   - **The in-kernel shell until plan 4** (§16 item 3). It keeps its own
     `Transcript` and its outputs (`File::ShellOutput`); its commands get the
     console as fd 0, in line mode while it waits for them. At its prompt it
     reads raw bytes from the input queue itself, taking the console back (raw
     mode, its own group) before each one, whatever mode a program left it in.
     Its redirection files and transcripts are nodes, not open files, so a
     removal does not reach them; plan 4's `/bin/sh` holds them as fds, which
     it does.
   - **Milestone 1's serial findings.** An `ESC` over COM1 with nothing after
     it for 50 ms (TSC time) goes in as the Escape key; any Ctrl-C, from the
     keyboard too, ends a half-arrived serial sequence.
   - **Error numbers** (§7.2). `vfs::Errno` gains `EMFILE` and `ERANGE` (34,
     for `getcwd`); `relay_abi::errno::name` names each number for test
     programs.
   - **Tests** (§8.5, §12). Test programs `t-files`, `t-mem`, `t-read`, `t-tee`
     and `t-sys`; scenarios `files`, `memory`, `console`, `tees` and `sysinfo`;
     `diskfull` and `unplug` gain a program's write errors, a tee on a full
     disk and `power` on an unplugged stick. The e2e runner gains `type TEXT`
     (keys without Enter, for Ctrl-D), `send-crlf TEXT` and `poweroff COMMAND`.
   - **Check scripts** (§12.4). `check3-a.sh` runs `t-files basic`, `dir`,
     `cwd` and `gone`, `t-mem map` and `grow 64`, `t-tee end`, `t-sys uname`
     and `t-read apart` (84 commands); a third step by hand types into `t-read`
     on the K120, with Backspace and Ctrl-D. The recorded NUC transcripts are
     those of plan 3b's NUC check.
5. **Decisions made while planning milestone 2's plan 4a** (the shell and
   its commands as programs):
   - **Plan 4 is two plans** (§13). 4a brings the shell and its commands
     as programs: the `Runner` split, `relay-rt`'s traits, `/bin/sh` and a
     program per command, which the in-kernel shell, still process 1, runs
     by path. 4b brings process 1, the error screen, the kernel without
     `shell`, `check4.sh` and NUC check 4; it is written after 4a merges.
     4a has no NUC check: of what the NUC runs it changes only the count
     of `/bin` and the ABI version in check 3's startup lines, which the
     check scripts' patterns allow; the recorded transcripts get `ABI 2`
     by hand until 4b's NUC check records real ones.
   - **Process 1** (§5.4, §6.6, corrected in its body; built by 4b). Process
     1 stays a process of the kernel's own, "init", as the in-kernel shell
     is now: it prints `/etc/motd`, starts `/bin/sh` in `/root` (or `/`
     without one) as its child, and when the shell ends logs how and starts
     another; three ends within 10 s, or a shell that cannot be started,
     reach the error screen. It collects every orphan as it ends (plan 3a's
     deferred finding), and `kill` still refuses it. So `/bin/sh` is not
     process 1 and collects no orphans.
   - **The shell's own commands and the runners** (§8.2, §8.3). The shell
     runs `cd`, `exit` and `help` itself, over its `Vfs`; every other name
     goes to its runner. The in-process runner runs the command table's
     functions, and any other name through `System::spawn` (the tests,
     `host-shell`, and the in-kernel shell until 4b). The spawning runner
     (`/bin/sh`) starts `/bin/<name>`, or the path as given, through the
     `shell::Programs` trait: it opens a redirection in the shell (created,
     emptied or appended to) and passes it as fd 1, closing its own copy
     once the child has it, and reports what cannot start, a killed
     program and a Ctrl-C as the in-kernel shell does. `exit [code]` stops
     the shell with the code modulo 256, or the last status, as bash does:
     the code may have blanks around it and must fit in 64 bits (otherwise
     it is status 2 and the shell stops anyway), `--` before it is
     dropped, and more than one argument stops nothing. In a script `exit`
     ends only the script, under either runner. `Shell::greet` (the motd, `/root`)
     is apart from `Shell::run`.
   - **Messages** (§8.2, §11.1, corrected in their bodies). `/bin/sh` names
     itself `relay-sh`, as the in-kernel shell does, so every milestone 1
     scenario and check script stays as it is (§1.4); `sh`'s own messages
     keep `sh:`, as in milestone 1.
   - **A command as a program** (§8.1, §8.4). `shell::run_command` runs a
     command function with its standard output on the program's fd 1
     (`shell::Stdout`), written at once to the console, so that it keeps
     its place among the errors on fd 2, and in pieces of 4 KiB to a file;
     a write error is `<name>: write error: <message>`, status 1, as the
     shell said it in milestone 1. Each program names its own command
     function, so it holds no other command's code. With the `user`
     profile (debug assertions and overflow checks on) the programs are
     71 to 133 KiB, `/bin/sh` 268 KiB, `system.img` about 2.7 MiB; the 24
     build in about 10 s. `opt-level = "s"` would save a quarter and is not
     used.
   - **`SysVfs` goes by path** (§8.1). A `Node` is the file's filesystem
     and inode, as `stat` gives them; `SysVfs` remembers the path it found
     each node by and opens that path for each operation, so a command
     holds no fd between calls and always reaches the file the path names
     now. `read_dir`'s records give the entries' kinds. `SysVfs::shutdown`
     does nothing: the kernel's `power` shuts the filesystems down.
   - **`Stat::dev` and ABI 2** (§7.3, §7.4, corrected in its body).
     `relay_abi::Stat` gains `dev` (offset 72, 80 bytes): the mount's number
     from 1, and 0 for the console. With `ino` it names a file, so that
     `cp`, `mv`, `rm -r` and `cat` tell a file of `/bin` from one of the
     root, whose inode numbers overlap, as the in-kernel shell does. The
     changed layout makes `relay_abi::VERSION` 2: `system: 33 programs,
     ABI 2`, `ABI 99, kernel wants 2`.
   - **`SysConsole`, `SysSystem`, `SysStdout`, `SysPrograms`** (§8.1).
     `SysConsole` reads fd 0 and writes fd 2 (a program's errors, and a
     shell's prompt and messages, as bash writes them); an interactive
     shell's takes the console back, raw and for its own group, before
     every read, whatever a program left it in (§16 item 4). An interactive
     shell that does not lead a group (one a script starts, in the
     script's group) takes back only raw mode and runs its commands in
     that group, without `FOREGROUND`, so the console never leaves it (the
     prototype's review found such a shell reading end of input after its
     first command); it gives each command line mode itself, as
     `FOREGROUND` does, so that Ctrl-C ends the command, with the shell and
     the script around it (the final review found them impossible to stop).
     `Console::interrupted` is false, since Ctrl-C kills. `SysSystem` has
     `time`, `sys_info`'s memory figures and log, and `power`:
     `System::reboot` and `System::poweroff` take `-f` (`POWER_FORCE`) and
     return the error that kept the machine up, from which `reboot` and
     `poweroff` print milestone 1's two lines. `SysStdout` is fd 1, the
     console when `fstat` says a character device. `SysPrograms` is
     `/bin/sh`'s `Programs` over `open`, `spawn`, `wait` and the tee calls.
   - **The console at the prompt** (§6.4, §7.3). `spawn` gains the flag
     `FOREGROUND` (with `NEW_GROUP`, `EINVAL` otherwise): the child's new
     group gets the console, in line mode, before the child runs. Without
     it the scheduler could run a child that reads the console before its
     parent handed it over, and the child would get end of input. A
     script's commands run in the script shell's group, without it.
   - **Scripts** (§6.5, §8.3). `sh FILE` in `/bin/sh` is a child `/bin/sh`,
     which checks and reads the script as `sh` does in milestone 1 (its
     operands, 64 KiB, UTF-8, a byte-order mark and CRLF, output not
     redirected, the transcript emptied or created), pushes the transcript
     as a console tee, traces, runs and syncs each line, and pops the tee.
     So the transcript gets the output of the script's programs and of any
     script it runs, whose own transcript is pushed on top, four deep at
     most (a fifth is `sh: cannot write the transcript …: Device or
     resource busy`). A write that failed is reported when the script ends:
     `sh: <log>: <error>; the transcript ends here`. `sh` without an
     operand starts an interactive shell, as bash's does (milestone 1's
     built-in said `sh: missing operand`). A script's `cd` stays in it; Ctrl-C ends the script with its command, which share its
     group, so its transcript has no `^C`. The in-process runner keeps
     milestone 1's scripts, which cannot run scripts.
   - **The in-kernel shell until 4b** (§16 item 3). It greets once, and
     reads on at a new prompt after `exit`. While it waits for a command it
     collects every orphan that ends (plan 3a's deferred finding, for the
     in-kernel shell): the prototype's review found that under `/bin/sh`
     each script stopped by Ctrl-C left its command's zombie to process 1,
     and that 62 of them filled the process table.
   - **Milestone 1's deferred findings** (plan 4's). The re-exports of
     `commands/mod.rs` come after its module list. A command's big output
     is no longer held whole under `/bin/sh`, whose transcripts are tees
     written every 4 KiB; the in-process runner keeps its transcript, and
     after 4b only the host uses it. A script that redirects into its own
     transcript garbles it, as it would under bash: ruled, and `sh`'s
     documentation says so.
   - **Left to 4b.** The kernel's findings carried forward (polling the
     console on the way back to ring 3, `console_foreground` of a group of
     zombies, `open(READ|CREATE)` of a directory, `POWER_FORCE` and
     `POWER_REBOOT` end to end, a host test of `push_tee`'s checks) come
     with 4b's kernel work. The console calls refusing a caller outside the
     foreground group wait for milestone 3's background jobs: no process of
     milestone 2 runs in the background, and a rule that lets every shell,
     nested ones too, take the console back needs job control.
   - **Error numbers** (§7.2). `vfs::Errno::ALL` lists them, and
     `Errno::from_number` turns a system call's number back into one.
   - **Tests** (§12). Scenarios `utils` (every program by path under the
     in-kernel shell, whose output hook is never a file) and `sh`
     (`/bin/sh` under the in-kernel shell: redirections into files, the
     console at once, Ctrl-C, nested scripts and their transcripts, `exit`);
     `system` lists the new `/bin`. Host tests: the runners against
     `FakePrograms`, `run_command` against `FakeStdout`, and `SysVfs`
     against the file calls over a `MountTable`, where every command prints
     what it prints over the table itself.
6. **Decisions made while planning milestone 2's plan 4b** (process 1):
   - **Plan 4b is one plan** (§13), in five pull requests: this plan; the
     kernel's findings carried forward and `t-abi`; process 1 as init, the
     error screen, and every scenario under `/bin/sh`; the kernel and the
     shell crate without the in-kernel shell's glue; `check4.sh` with NUC
     check 4.
   - **Process 1 is init** (§5.4, §6.6; `kernel/src/init.rs`). A process of
     the kernel's own, without a program. It prints `/etc/motd` once, at
     boot, and starts `/bin/sh` with no argument (argument 0 its path) in
     `/root`, or in `/` without one, chosen again at each start; in a new
     process group that gets the console (`NEW_GROUP | FOREGROUND`, so the
     next shell takes the console back whatever the last one left it in),
     its fds 0-2 the console. The shell is pid 2, then whatever pid is
     next. Init waits for it, collecting every orphan that ends meanwhile.
     When the shell ends init says how, on the screen and in the kernel log
     (`init: /bin/sh (pid 2) exited with 0; starting it again`, or
     `killed: <how>`), and starts another; the third end within 10 s (by
     the TSC's clock, the tick's without one) reaches the error screen, as
     does a shell that cannot be started. The rule is host-tested over a
     clock; `kill` still refuses process 1. Test mode (`test=1`, power-off
     exits QEMU) is set at startup, before process 1. The `noroot`
     scenario removes `/root` and sees the next shell start in `/`.
   - **The error screen** (§11.2; `kernel/src/error_screen.rs`). Init shows
     it, in its own context with interrupts on, so the idle task polls the
     keyboards and COM1 while it waits: it is no kernel bug, and not the red
     panic screen. The screen is cleared and shows `*** Relay OS cannot run
     its shell ***`, the reason (`system: <the startup line's reason>`,
     `/bin/sh cannot start: <error>` or `/bin/sh ended 3 times within
     10 s`), the kernel log's last 20 lines (27 of the NUC's 33 rows in
     all when none wraps; of those, the newest that fit the console's rows,
     a line wider than it taking more than one and the cursor's row after
     the last counting too, so the heading never scrolls away: the
     prototype's review found the NUC's real lines wrapping), and `Press
     any key to reboot.` What was typed before it came (the
     LF of a terminal's CR LF after the last `exit`) is dropped; then it
     waits for a key as long as it takes, since a restart on its own would
     come back to it. After the key it syncs, shuts the filesystems down (a
     failure does not keep the machine up: nothing can be fixed here) and
     restarts through the ACPI path (M1 §7.4). A `system.img` that cannot
     be mounted (startup step 10) is recorded, and init takes the record
     out before it shows the screen for it, starting nothing (the review
     found it held the record's lock across the wait; the switch's check
     now covers that lock); `mount_fail`'s empty root still reaches a
     prompt, in `/`, without a motd. The screen takes the console back (raw
     mode, init's group) and wakes its readers, which no test shows: in
     milestone 2 the console is init's or the last shell's, in raw mode,
     whenever the screen comes, since nothing a person can run ends the
     shell while a command has the console (ruled; milestone 3's `kill`
     can, and its plan tests it).
   - **The kernel without the shell** (§1.4 item 3, §8.2). `relay-kernel`
     no longer depends on `shell`, even through another crate, which a test
     checks by walking `cargo metadata`'s resolved graph (the review found
     one that looked at direct dependencies only).
     `session.rs` goes (its `KernelEnv`, the filesystems' clock and log,
     moves to `storage.rs`), and with it `File::ShellOutput`,
     `FdTable::shell`, the output hook (`SHELL_OUT`, `proc::wait`),
     `renew_outputs`, `collect_orphans` and `give_console`; process 1's fds
     are the console. `take_console` stays, for the error screen, and wakes
     the console's readers, which plan 3b found it did not; both callers of
     `KernelVfs::shutdown` (`power` and the error screen) write the tees
     first. The shell
     crate's `System::spawn`, `System::wait`, `Output`, `run_program` and
     `Ctx::wait_program` go: the in-process runner (`host-shell`, the tests)
     runs no programs, and names it does not know are `command not found`.
   - **Milestone 1's scenarios under `/bin/sh`** (§1.4 item 1, §12.3). A
     prototype of init starting `/bin/sh` ran every scenario first: all of
     milestone 1's passed unchanged. What changed is later plans':
     `diskfull` expected a program's redirection write error to be reported
     by the in-kernel shell (`t-files: write error: …`), and under
     `/bin/sh` a program reports its own, as under bash; `t-spawn fill`
     finds room for 61 children, 60 under a nested shell, since the shell
     is a process now (`sh`, `spawn`); `sh` runs by path from the shell
     init started, and `utils` sees a program write lines into a file and
     refuse `cat f >> f`.
   - **`t-abi`** (§8.5, §12.3). xtask makes it from `t-args`, its note
     saying ABI 1, and checks that the build's checks and the kernel's
     refuse it for that alone (`built for ABI 1`); it is in `/bin`, so
     `system: 34 programs, ABI 2`. `stale_program` runs it from `/bin` and
     copied to `/root` (`relay-sh: …: Exec format error`, `spawn …: built
     for ABI 1` in the kernel log).
   - **Findings carried forward** (plans 3a, 3b, 4a). A program that spends
     its time in system calls has the console polled on its way back to
     ring 3 when a tick passed during the call (`t-mem churn` maps and gives
     back nearly all free memory over and over, and Ctrl-C must stop it
     within 3 s: without the poll it did not, in three runs of three);
     `console_foreground` of a group whose members are all zombies is
     `ESRCH`; `open` with `CREATE` of a directory is `EISDIR`, as on Linux;
     what a tee may be has a host test; a write that takes nothing is
     `ENOSPC` in `relay-rt`, as gnulib's `full_write` says (and
     `shell::Stdout`). `reboot -f` and `poweroff -f` ask `power` without
     `POWER_FORCE` first, so they say milestone 1's `cannot shut the
     filesystems down cleanly: …` before going ahead (plan 4a's final
     review), and `unplug` runs both to the end.
   - **The e2e runner** (§10). `reset [TEXT]` types the text and Enter (a
     key at the error screen, `reboot -f`), and the machine must restart as
     after `reboot`; after `unplug` the root's clean flag is not checked,
     since the disk keeps what it had when it was pulled out; an `expect`
     after `poweroff` reads what the machine printed before it went away.
   - **Check scripts** (§12.4). `#same> NAME REGEX` is a line whose group
     must capture what it captured the first time (`free` before and
     after). `check4.sh` (33 commands) runs after `check3-b.sh`: `t-spawn
     fill` and `1000`, `free`, every `t-fault` kind, `t-spawn kill-new` and
     `orphan`, `t-args`, `t-abi`, `ls /bin` into a file and `cat` of a file
     into itself, a script that runs another, and `free` again; then `exit`
     by hand, which restarts the shell, and `poweroff`. `check3-a.sh` and
     `check3-b.sh` run under `/bin/sh` unchanged, their transcripts tees.
     The recorded NUC transcript of `check4.sh` is QEMU's until NUC check 4
     records the NUC's. §14's measurement of the tick poll's cost "with and
     without" is not check 4's (§12.4 and §14, corrected in their bodies):
     `t-spin`'s count with the poll is in check 3's and check 4's
     transcripts, and a count without it needs a kernel without it, which
     plan 5 builds if those counts call for it.
   - **Left to plan 5**: the NUC check's fixes, the tick poll's cost without
     the poll if the NUC's counts call for it, `flush_pages` outside
     `arch/`, the guest tests' unbounded loops, `read_dir`'s heap
     measurement, and milestone 1's plan-5 findings (roadmap). A `/bin/sh`
     that cannot be started reaches the error screen only in a host test
     (no scenario builds an archive without it). The console calls refusing
     a caller outside the foreground group wait for milestone 3.
7. **Decisions made while planning milestone 2's plan 5** (hardening and
   0.3.0):
   - **Plan 5 is one plan** (§13), in four pull requests: this plan; the
     kernel's fixes and the guest tests' bounds; xtask's fixes and the
     runner's new steps; version 0.3.0, with NUC checks 3 and 4 on a stick
     written by `flash --full`. No item waits for milestone 3.
   - **The error screen ends every other process first** (§11.2,
     corrected in its body). Before it draws, init kills every process
     but itself, as `kill` does: each ends before it runs another
     instruction of its program, and says so in the kernel log only
     (`pid <n> (<path>): killed: kill`), so nothing writes over the screen
     or scrolls its heading away (plan 4b's final review found an orphan's
     `t-spin` line under `Press any key to reboot.`). Killing them, rather
     than dropping their output, also keeps a spinning program from sharing
     the CPU with the wait; their zombies stay, since the machine
     restarts. `respawn` leaves a `t-spin 1` orphan running at the third
     `exit`.
   - **The error screen counts rows as the terminal moves** (§11.2). A
     line's rows come from `term`'s own parser and the terminal's cursor
     rules: a tab moves to the next multiple of 8 but never past the last
     column, a carriage return to the first, a backspace one back, and a
     character after the last column starts a row. A test checks each kind
     of line against `term::Terminal` itself at the NUC's 120×33. An escape
     that is not a colour, which a program's bytes can leave in the kernel
     log (`ESC c` resets the terminal), is shown as `?`, not obeyed; and the
     screen's clear starts with CAN, so an escape a program left unfinished
     cannot swallow the reset of its colours (the prototype's review drew
     the screen black on black after a lone ESC).
   - **What nothing uses goes** (§16 item 6). `exec::arg_bytes` (a test
     helper now), `InputQueue::take_interrupt` (its tests check the queue's
     bytes instead), `tty::is_line_mode` and `InputQueue::is_line_mode`;
     a scan for `pub` functions with test callers only also found
     `OpenFile::is_dir` (since plan 3b) and `arch::idle_forever` (since
     milestone 1's shell ended the boot), and the prototype's review
     `PageTables::translate`, which only tests use (`#[cfg(test)]` now).
   - **`read_dir`'s heap** (§16 item 4). The room it allows the
     directory's list leaves out its own buffer, up to 64 KiB of the same
     heap.
   - **The TLB is `arch`'s** (§3.1). `mm` flushes it through `arch::tlb`,
     and a host test walks the kernel's sources and fails if a file outside
     `arch/` names a `tlb` path other than `arch::tlb`, however imported, or
     `invlpg` (the prototype's review fooled a first test, which looked for
     `instructions::tlb` only, with an import in braces). `mm::init`'s CR3 and PAT setup and
     milestone 1's port I/O stay where they are, for the aarch64 port's
     architecture layer after the gate (§15).
   - **Every loop in a guest test is bounded** (§8.5). `t-files`' opens
     (64) and `read_dir` calls (64), `t-tee`'s reads (64 KiB) and pushes
     (16): a kernel that never says stop makes the scenario fail at once
     (`and no end`), instead of a program looping on (plan 3b's finding).
   - **Milestone 1's plan-5 findings** (M1 §15 item 12). A connection that
     is still bouncing when the debounce gives up after 2 s is
     `UsbError::Unstable` (`connection not stable`), which the host does not
     try again (it cost 3 × 2 s); a QMP message cut at a wait's deadline is
     kept and read whole later, and the wait sets the timeout on the
     reader's own socket; the loader-rules test finds comments outside
     string and character literals only; `verify-usb` fails a transcript
     older than the `system.img` on the stick's ESP (`the transcript is
     older than the system on the stick (system.img built …): run the
     script again`), since it shows what an older kernel and programs did:
     `flash --kernel` keeps the transcripts, and `flash --full` erases them
     (the prototype compared with the script, which the review found could
     only fail when the NUC's clock runs behind; it is UTC, as the stick's
     times show); `parse_fadt` reads the 32-bit DSDT field once.
   - **The e2e runner** (§10, §12.3). `system-drop NAME` is a before-boot
     step (`system.img` without that program), and the scenario
     `system_nosh` sees the error screen say `/bin/sh cannot start: No such
     file or directory`: §16 item 6 said a host test covered it, but only
     the reason's text was. `reset-key` leaves the error screen with Enter
     on the USB keyboard, as a person does on the NUC; `system_missing`
     uses it.
   - **The tick poll's cost** (§14, corrected in its body). On the NUC,
     `t-spin 1` makes 4594860032 iterations with the console polled on
     every tick that interrupts ring 3 (as in plans 3b and 4b), and
     4677697536 with a throwaway kernel built without that poll (never
     merged): the poll takes 1.8 % of a spinning program's time, about
     18 µs of each 1 ms tick. That is not too much for milestone 2, so the
     poll stays on every tick; §14's fallback, every 4th tick, would save
     about 1.3 % and poll the keyboards a quarter as often, and waits for a
     workload that calls for it.
   - **Version 0.3.0** (§1.4, §2). The workspace's version, `uname -a` in
     `shell` (milestone 1's scenario, changed only by the version, as
     milestone 1's own bump did) and in `check3-a.sh`, and the recorded
     transcripts. A spike bumped the version first: nothing else depends
     on it. Milestone 2 ends with NUC checks 3 and 4 on a stick written by
     `flash --full`; check 4 gains the error screen by hand (§12.4,
     corrected in its body): after a `reboot`, so that the kernel log's
     last lines are the boot's and one of them is wider than the screen
     (right after `check4.sh` none is, the prototype's review found), three
     quick `exit`s, and a key on the K120 restarts the machine. The stick's
     `/root/README` no longer says milestone 1.
   - **Milestone 2's definition of done** (§1.4). Item 1: `cargo xtask ci`
     runs 41 scenarios, milestone 1's unchanged but for the startup lines
     and `shell`'s version. Item 2: NUC checks 3 and 4, in plan 5's last
     pull request. Item 3: an xtask test walks `cargo metadata`'s resolved
     graph (since plan 4b), and every command but `cd`, `exit` and `help`
     runs from `/bin`. Nothing is unmet.
   - **Earlier deferred findings, settled.** Plan 3a's: orphans piling up
     (plans 4a and 4b); §5.1's `stac`/`clac` (its body says no copy needs
     them) and §8.5's table (corrected in its body: `t-sys`'s kinds, and
     the kinds a program's children run are left out); a program seeing
     its redirection's write error end to end (`diskfull`'s `t-files full
     > more` under `/bin/sh`). Plan 3b's: `EISDIR`, zombie groups,
     `POWER_FORCE`, the tees before a shutdown and the readers woken (plan
     4b); `flush_pages`, the guest loops and `read_dir` (above). Plan 4a's
     `ENOSPC` for a write that takes nothing (plan 4b). Left for milestone
     3: the console calls refusing a caller outside the foreground group,
     and a test of the error screen taking the console back, which only
     milestone 3's `kill` can reach; milestone 1's "still out of the gate"
     list stays out (§15).
8. **Decisions made while planning milestone 3's plan 1** (pipes and
   standard input):
   - **Milestone 3's plans** (§13). Its roadmap keeps §13's four plans,
     named `m3-plan-1` to `m3-plan-4` (steps 6–9). Background jobs stay
     in milestone 3 (§1.2): nothing calls for the cut. Plan 1 is one plan
     in four pull requests: this plan; the kernel's pipes and the group a
     child joins; the shell's pipelines and standard input; the new
     programs. It has no NUC check: nothing it changes is particular to
     the NUC (QEMU's USB keyboard types into a pipeline), and plan 4's
     `check5.sh` runs pipes there.
   - **Pipes** (§7.3, §9.1; `kernel/src/pipe.rs`). `pipe` makes a 16 KiB
     ring in four contiguous frames of its own, not the kernel's heap,
     which would hold only about 1000 of them and which `spawn` needs for
     the programs it reads. The frames are user memory's: `ENOMEM` once
     fewer than 8 MiB of frames would be left (§11.1), and `free` counts
     them. The memory for the two fds is checked first (`EFAULT`), then
     two free fds (`EMFILE`), before anything is made; the ends take the
     lowest free fds, the read end first. A read gets what the pipe holds,
     up to its length (and the ring's), and waits only while it is empty
     and a writer is open; 0 once every write end has closed. A write
     takes what fits and waits only while nothing of it has gone in, so a
     full pipe gives fewer bytes than asked (which `relay-rt`'s
     `write_all` continues); `EPIPE` once every read end has closed. An
     end closes when the last fd that has it closes, in its process or any
     child `spawn` gave it to, or when a process ends, and the processes
     waiting on the pipe are woken. A process killed while it waits gets
     `EINTR`, which never reaches its program (it ends first, §16 item 3).
     `fstat` says a FIFO, with `dev` 0; `seek` is `EINVAL`, `read_dir`
     `ENOTDIR`; reading a write end or writing a read end is `EBADF`; a
     pipe is no tee (`EINVAL`, as the console: a tee is written from any
     process, where a full pipe could not wait). The ring's calls never
     wait; the dispatcher does, through the process (`Caller::pipe_wait`),
     on `Blocked::Pipe` of the pipe's id, and a pipe is made with the
     function that wakes its waiters, so the host tests wake no process
     table. A pipe's lock is held only inside its own calls, and an end is
     never dropped while the process table is locked (a debug assertion).
   - **Joining a group** (§6.4, §7.3, §7.4). `SpawnArgs` gains `pgid` and
     a reserved word that must be 0 (128 bytes): without `NEW_GROUP`, a
     `pgid` other than 0 is a group the child joins instead of its
     parent's. It must be the group of one of the caller's children, an
     ended one not yet collected included (`EPERM` otherwise, Linux's
     `setpgid` answer), so a pipeline's later stage can join a first stage
     that has ended already (`true | cat`); with `NEW_GROUP` it is
     `EINVAL`. The changed layout makes `relay_abi::VERSION` 3: `system: 40
     programs, ABI 3`, `ABI 99, kernel wants 3`, and `t-abi` built for ABI
     2 (`STALE_ABI` is the ABI before). The recorded NUC transcripts get
     `ABI 3` by hand until plan 4's NUC checks record real ones.
   - **`relay-rt`** (§8.1). `sys::pipe`, and `sys::spawn` takes the group
     to join. A write to fd 1 that fails with `EPIPE` ends the program
     with 141 (`BROKEN_PIPE`) and no message, in `sys::write` itself, so
     every way to fd 1 does so; a pipe of the program's own still sees
     `EPIPE`. `SysStdin` reads fd 0.
   - **Standard input** (§9.1). A command gets standard input
     (`shell::Stdin`) as it gets standard output: a program's fd 0, which
     is the console in line mode (a line a read, Ctrl-D its end, Ctrl-C
     for the whole group) or a pipe; or bytes in memory (`shell::Bytes`)
     in the in-process runner, whose first input is what a test gives
     (`Shell::with_input`) and in `host-shell` nothing, so `cat` alone
     ends at once there. `run_command` takes what a program works with as
     one `CommandIo`. `cat`, `wc`, `head` and `tail` without a file read
     it, and so does a file named `-` (`cat header - footer`), as GNU's do;
     `cat` no longer says `missing operand`. A command writes out what
     waits for its standard output before each read of its input, so a
     line typed into `cat | cat` reaches the second at Enter (the
     prototype's review found it held in 4 KiB pieces until Ctrl-D).
     `head` stops reading once
     its lines are out, so a writer before it gets `EPIPE`; `tail` keeps
     the last lines as they come, a line cut across reads joined. GNU's
     names for standard input in messages are kept (`cat: -: …`,
     `error reading 'standard input'`).
   - **`wc`** (§9.1, §12.3). `wc` gains `-c`, `-l` and `-w` (§12.3's
     `cat big | wc -c` needs `-c`), and its widths are GNU wc's: one count
     of one input is not padded; otherwise the columns fit the regular
     files' total size, with at least 7 digits when an input is something
     else (standard input is never a regular file here). A test compares
     it with the host's `wc`, which showed that a file that cannot be
     found takes no part in the widths: milestone 1's `wc` padded to 7
     for one, and now does not.
   - **Pipelines in the shell** (§8.2, §9.1). An unquoted `|` joins
     commands; bash's syntax errors name a `|` with nothing before it
     (``syntax error near unexpected token `|'``) or after it (`syntax
     error: unexpected end of file`); `||` is refused as unsupported
     syntax, and so is a redirection on a command before the last
     (`unsupported syntax: > before |`), where bash would send that
     command's output into the file. `cd`, `exit` and `help` cannot be in
     a pipeline: `relay-sh: cd: cannot be used in a pipeline`, status 1
     (§9.1 said `sh:`; the shell's own messages say `relay-sh:`, §16 item
     5), and nothing of the line runs. Every command of a pipeline has a
     name: a redirection alone, which bash runs as a command, is refused
     (`> f | b`: `unsupported syntax: > before |`; `a | > f`:
     `unsupported syntax: | >`; the review found `/bin/sh` panicking on
     the second). `/bin/sh` starts the stages left to
     right in one group: the first that starts gets a new group and the
     console, the others join it (`Group::Join`), and a script's stay in
     the script's group. Each pipe is made just before the stage that
     writes it starts, and the shell closes its copies of the ends as soon
     as the stages have them, so a reader sees its end once its writer has
     ended. A stage that cannot start says so at once, its neighbours see
     an end, and the others run, as in bash; a pipe that cannot be made
     (`relay-sh: pipe error: …`, status 1) stops the starting. The shell
     waits for every stage, says how any killed one ended, a Ctrl-C once
     (`^C`), and takes the last one's status; a stage that ended with 141
     says nothing. The in-process runner runs the stages one after
     another, each one's output kept in memory as the next one's input,
     and refuses `sh` in a pipeline too (it would run its script in that
     shell); a stage that is not found gives the next nothing, and Ctrl-C
     stops the rest.
   - **`X | sh`** (§8.3). `/bin/sh` without arguments whose fd 0 is no
     console runs the lines it reads there, as they come, as bash does:
     without a prompt, a trace or the line editor, so it never takes the
     console (the review found `cat | sh` leaving it in raw mode, where
     Ctrl-C could no longer end the pipeline). It ends at the input's end
     or `exit`; a line over 64 KiB or not UTF-8 is skipped with an `sh:`
     message.
   - **`grep`** (§9.1; `crates/shell/src/pattern.rs`,
     `commands/grep.rs`). `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]`
     matches bytes as GNU grep does with `LC_ALL=C`: literal bytes, `.`,
     `*` (literal at the start), `^` at the start and `$` at the end
     (literal elsewhere), `[...]` with ranges, `^` and `]` first, `\`
     before a byte for that byte; `-i` folds ASCII letters only. The
     escapes that mean something else in GNU's basic expressions (groups,
     intervals, `\|`, `\+`, `\?`, word anchors, back-references) and a
     set's classes are refused (`grep: \( \) is not supported`, status 2)
     rather than matched differently; a malformed pattern gets GNU's
     message (`Unmatched [, [^, [:, [., or [=`, `Trailing backslash`,
     `Invalid range end`, `Invalid regular expression`). Matching runs
     every item at once over the line (an NFA), in time proportional to
     the line times the pattern, whatever the pattern. Several files put
     each one's name first; an input with a NUL byte is binary, and
     `grep: f: binary file matches` replaces its lines; a file that is
     also its output is refused (`grep: f: input file is also the
     output`), as GNU's does, since it would read its own output for ever;
     the status is 0 if a line was selected, 1 if none, 2 after an error,
     a write error included, as GNU's (`Ctx::set_write_error_status`; the
     other commands keep 1). A range cannot start where one ended
     (`[a-z-9]`: `Invalid range end`). A test
     runs every pattern of up to three symbols of a small alphabet (1885)
     and the `-i` ones through the host's GNU grep: the same lines, or the
     same refusal.
   - **`seq`, `sleep`, `true`, `false`** (§9.1). `seq [FIRST [INCREMENT]]
     LAST` prints whole numbers as GNU seq does (it takes GNU's INCREMENT
     too, rather than calling a third operand extra; negative numbers are
     operands, not options); GNU's also takes decimals, exponents, `inf`,
     hexadecimal and numbers beyond 64 bits, of which this one says `seq:
     not a whole number: '1.5'`, and the options `-f`, `-s` and `-w`,
     which are unknown here; what GNU's refuses gets GNU's first line
     (`invalid floating point argument`, `invalid option`). `sleep` waits for the sum of its
     times, in seconds or with GNU's `s`, `m`, `h` and `d`, a fraction
     counting to the millisecond, through the shell's `System`, which
     gains `sleep`. `true` and `false` ignore their arguments. With them
     and `t-pipe`, `/bin` holds 40 programs.
   - **Messages.** The new commands print the first line of GNU's message
     for what they refuse, without GNU's `Try '… --help'` line, as
     milestone 1's commands do; `grep` without a pattern prints GNU's two
     usage lines, which are its message.
   - **Tests** (§8.5, §12). `t-pipe` (`basic`, `room`, `child`, `eof`,
     `epipe`, `killed`, `many`) and the scenario `pipe_calls`; `t-spawn
     join` in `spawn`; `cat` reading the console in `console`; the
     scenario `pipes`. Host tests compare `wc`, `head`, `tail`, `grep` and
     `seq` with the host's own (`testing::host_tool`, run in a directory
     under `target/` with `LC_ALL=C`, its input written from a thread; a
     missing tool fails the test).
   - **Plan 5's deferred minors.** The roadmap's `verify-usb` wording is
     corrected in this plan's first pull request; `t-spawn fill` is
     bounded at the table's 64; the other five go to plan 4.
9. **Decisions made while planning milestone 3's plan 2** (jobs, `ps` and
   `kill`):
   - **Plan 2 is one plan** (§13) in four pull requests: this plan; the
     kernel's `proc_list`, the console calls' refusal, `wait`'s Ctrl-C and
     the error screen's test; the shell's background jobs, `jobs`, `wait`
     and `kill`; `ps`. It has no NUC check: nothing it changes is
     particular to the NUC (QEMU's USB keyboard types the Ctrl-C a `wait`
     needs), the stick keeps 0.3.0 until plan 4's `flash --full`, and plan
     4's `check5.sh` runs a background job and `kill` there.
   - **`proc_list`** (§7.3, §9.3; `relay_abi::proc`). It fills a buffer
     with a `ProcInfo` per process, by pid (96 bytes: pid, ppid, pgid and
     state as `u32`, frames and CPU ticks as `u64`, the name in 64 bytes
     padded with NULs), as many whole entries as fit, and returns how many
     processes there are, so a short buffer is no error and says what it
     missed (`PROC_MAX`, 64, always fits); nothing is written unless all
     that fits can be (`EFAULT`). A new struct changes no layout, and
     `VERSION` stays 3. The state says what a blocked process waits for:
     `STATE_RUN`, `READY`, `WAIT` (a child), `READ` (the console),
     `SLEEP`, `PIPE`, `ZOMBIE`. The frames are its address space's (its
     pages, their tables and its PML4, counted by walking the tables as
     they are given back), 0 for a zombie and for process 1, which has
     none; the name is the path `spawn` was given, cut at 64 bytes.
   - **Who may change the console** (§6.4, §7.3; `kernel/src/proc/holders.rs`).
     The process table keeps the chain of groups that handed the console
     on, from process 1's at the bottom to the foreground group at the
     top. `spawn`'s `FOREGROUND` and `console_foreground` put a group on
     top, cutting the chain back to the giver's group first, and a group
     taking the console back (its own, or one below it) is cut back to;
     the error screen gives it to process 1 alone; groups with no process
     left leave it, so it holds at most one entry per group. `console_mode`,
     `console_foreground` and a `spawn` with `FOREGROUND` are `EPERM` for a
     caller whose group is not in it (`console_foreground` says `ESRCH`
     first for a group with no live process). So a shell takes the
     console back after its command's group has ended and been
     collected (it switches the mode while that group is still on top),
     nested shells and scripts each take it from theirs, and a background
     job, never given it, can neither take it nor change its mode: `sh
     script &` and `sh &` leave the console alone. A spike ran every
     scenario under this rule first: all passed, and the one refusal was
     `t-read leave`'s orphan, which now shows it. The tee calls stay open
     to any process: a tee changes nothing a process reads, and refusing
     them would stop `sh s &` before its first line; so a background
     script's transcript also gets what the screen shows meanwhile, the
     prompt's typing included (ruled).
   - **`wait` and Ctrl-C** (§7.3, §9.2). At its prompt the shell has the
     console in raw mode, where a Ctrl-C is only a byte, so nothing could
     stop `wait` while a job ran on. `wait` gains `WAIT_CTRL_C`: a Ctrl-C
     typed while the caller's group has the console in raw mode ends the
     wait with `EINTR` and is taken from the input, and that group's
     waiters are woken when one comes. Without the flag a raw Ctrl-C stays
     input. A wait with the flag blocks in a state of its own, the only
     one a Ctrl-C wakes (the review found a plain waiter woken on every
     pass of the idle task while a raw Ctrl-C waited unread). A flag no
     program passed before (it was `EINVAL`) adds to the ABI and changes
     no meaning, so `VERSION` stays 3.
   - **Tees** (§6.5, corrected in its body; §16 items 4 and 5). A process
     pushes at most 4 tees, and the stack holds 64, one for each process
     the table can hold: with 4 for the whole machine, the review found
     four background scripts, a transcript's tee each, kept any further
     script from starting (`Device or resource busy`). So nested scripts
     are no longer stopped at four deep (§16 item 5).
   - **Background jobs** (§9.2; `crates/shell/src/jobs.rs`). An unquoted
     `&` at the end of a line (a comment may follow) runs it in the
     background; elsewhere it gets bash's syntax error (`&` alone, `a |
     &`, `echo > &`, `a & &`) or is refused as unsupported (`a & b`, `&&`,
     `> f &`, which bash runs). A job's commands start as a pipeline's do,
     the first that starts in a new group without the console
     (`Group::Background`), at the prompt and in a script alike, so a
     script's Ctrl-C does not reach them (bash's jobs ignore SIGINT
     there); the others join it, and nothing waits for them. At the
     prompt the shell says `[n] <pid of the last command>`; it collects
     what ended with `wait(-1, NOHANG)` before each prompt and before
     running each line typed (so a job that ended meanwhile leaves its
     slot in the process table to that line, which the review found
     refused at a full table), and says how each finished job ended
     before the next prompt (after a foreground command too); a
     script and `X | sh` collect before each line and say neither, as
     bash's non-interactive shells do. A job none of whose commands
     started is no job (the status the last one's, 127); once one
     started the status is 0. `cd`, `exit`, `help`, `jobs`, `wait` and
     `kill` cannot run in the background (`relay-sh: cd: cannot be used in
     the background`, status 1). The in-process runner, which has no
     programs, keeps refusing `&` (`unsupported syntax: &`). `>&2` and `>&
     f`, which bash runs, are `unsupported syntax: >&`, `>>&` and `> &`
     bash's syntax errors. `exit` leaves running jobs to process 1, as
     bash's does without `huponexit`. An interactive `/bin/sh` whose group
     was never given the console (`sh &`) ends at once with 0, before its
     first prompt (its leader test says `EPERM` exactly then); bash stops
     such a shell, which needs job control this gate leaves out.
   - **The job table** (§9.2). A new job is numbered one past the highest
     in the table, so numbers start again only once it is empty: bash 5.2
     numbers 4 and 5 after jobs 1 and 2 ended, not the lowest free one.
     The newest job is `+`, the one before it `-`. A job has ended when
     every process it started has, and its status is its last one's. Its
     text is the line as typed, without the `&`. Its lines are bash's,
     the state padded to 24 columns (`[1]+  Done                    sleep
     5`, `Exit 1`, `Running` and the text with ` &`), rather than §9.2's
     shorter form (corrected in its body). A job a signal ended says
     bash's words for it, which the statuses already follow: `Killed`
     for `kill`, `Segmentation fault` for a page fault, a protection fault
     or a stack overflow, `Illegal instruction`, `Floating point
     exception`, `Interrupt`; the kernel log keeps the fault's detail.
     bash pads to 24 and no further, so its `Floating point exception`
     meets the text; a blank stays between here (ruled). Job specs are
     bash's: `%n`, and `%%`, `%+` or `%` for the current job, `%-` for the
     previous one (the current one when it is alone).
   - **`jobs`, `wait` and `kill`** (§8.3, §9.2, §9.3). Built-ins, since
     they need the job table: a built-in's `Ctx` gets the shell's job
     control (its jobs and, under `/bin/sh`, its programs), and `help`
     lists them. `jobs [%n | n]...` lists every job or those named, in the
     order named (one named twice twice), and a job that has ended says
     how there once and leaves the table. `wait` waits for every job,
     which then leave the table, at the prompt each saying how it ended,
     as bash's do; with nothing running as it began, only those a signal
     ended say so (the final review found that bare `wait` silent, from
     notes that had seen only that case); `wait %n` or `wait PID` waits
     for one, its status that job's last process's or that
     process's own, and at the prompt a job that ends there says how at
     once. These notices go to the screen, never into a redirection. The
     table keeps the statuses of the processes of jobs that left it (the
     table's 64 at most), so `wait PID` of one answers once, as bash's
     does. A Ctrl-C at the prompt ends a `wait` (`^C`, 130), the jobs
     running on. `kill PID...` and `kill
     %n...` kill processes or a job's whole group (`kill(-pgid)`), the
     status 1 if any was not killed; killing is the only signal (§15):
     `-9`, `-KILL`, `-SIGKILL` and `-s KILL` are taken, another signal
     Linux has (and `-0`, which tests that a process exists, and `-l`,
     which lists the signals) is `not supported`, and a name it has not
     is an `invalid signal specification`. Their messages are bash's with `relay-sh:`:
     `kill: (1) - Operation not permitted`, `kill: (999) - No such
     process`, `kill: %3: no such job`, `kill: abc: arguments must be
     process or job IDs`, `wait: pid 9 is not a child of this shell` and
     `wait: %3: no such job` (127), ``wait: `abc': not a pid or valid job
     spec`` (1), and an option `invalid option` (2). Until plan 3's `$?`,
     their status is the shell's last status, which `exit` and a script's
     end take.
   - **`ps`** (§9.3; `commands/system.rs`, `/bin/ps`). A command function
     over the shell's `System`, which gains `processes` (from `proc_list`;
     none on the host, where `ps` says `processes are not available` as
     `free` says of memory figures), and a program like the others: `/bin`
     holds 42. Its columns are procps's, right-aligned numbers:
     `  PID  PPID STATE      MEM  TIME CMD`; STATE `run`, `ready`, `wait`,
     `read`, `sleep`, `pipe` or `zombie`; MEM the KiB of the frames its
     address space holds; TIME its CPU time as `m:ss`; CMD the path it was
     started from. Process 0, the idle task, is no process.
   - **The error screen takes the console back** (§11.2; milestone 2's
     leftover). The scenario `screen_console` ends the shell a third time
     within 10 s while its command, `t-proc end-shell`, reads the console
     in line mode and its child kills the shell; the screen ends every
     other process, gives the console to process 1 in raw mode, and a key
     restarts the machine, which it would not in line mode, where the key
     waits in the line discipline.
   - **Plan 1's deferred minors.** `pipe()` puts copies of its ends in the
     fd table, both or neither, so the last reference to an end that did
     not go in is dropped after the table is unlocked, by construction
     rather than because two free fds were checked first. `X | sh`
     reading its input 4 KiB at a time goes to plan 3, which reads
     scripts; `seq`'s zero increment and `[a-a-`'s message go to plan 4.
   - **Tests** (§8.5, §12). `t-proc` (`list`, `short`, `long`,
     `end-shell`) and the scenarios `proc_calls`, `screen_console` and
     `jobs`; `t-tee owners` in `tees`; `t-spawn ctrl-c` and `ctrl-c-apart`
     (a wait apart from the console never takes its Ctrl-C) in `ctrlc`,
     which now waits for the prompt
     before typing into the next command (a spike found the line could
     reach a command's group that had just ended); `t-read refused` and
     `leave` show the refusals in `console` (`t-read apart`, which
     `check3-a.sh` runs on the NUC, prints what it printed). Host tests: the chain of
     holders, the job table against bash 5.2's own lines, and the shell's
     jobs over `FakePrograms`, whose children can outlive a prompt.
10. **Decisions made while planning milestone 3's plan 3** (script
    arguments and variables):
    - **Plan 3 is one plan** (§13) in three pull requests: this plan; the
      parser's parameters and the shell's variables and `$?`; script
      arguments and the scenario `script_vars`. It has no NUC check: no
      check script holds a `$`, so their transcripts do not change, and
      plan 4's `check5.sh` runs variables and script arguments there. A
      spike first expanded every parameter to nothing and ran every
      scenario and check script: all passed, and only the two host tests
      that pinned `$`'s refusal failed.
    - **What expands** (§9.4). Outside single quotes, `$NAME` and `${NAME}`
      (a name is `[A-Za-z_][A-Za-z0-9_]*`, the longest that follows), `$0`
      to `$9`, `${N}` for any argument (`$10` is `$1` and a `0`, as in
      bash), `$#`, `$@` and `$?`. A `$` alone, or before a character no
      parameter starts with, is a `$`. bash's other parameters, `$*`, `$$`,
      `$!`, `$-` and `$_`, are unsupported (`unsupported syntax: $*`), as
      are `$(`, `$((` and `` ` ``, and bash's other quotes `$'…'` and
      `$"…"`; so are `${NAME:-x}` and bash's other operators, `${#NAME}`
      and `${!NAME}` (`unsupported syntax: ${NAME:-x}`); a
      `${…}` that holds no name, number or special parameter is bash's
      `relay-sh: ${1A}: bad substitution` (status 1), and one without its
      `}` a syntax error (``syntax error: unexpected EOF while looking for
      matching `}'``, status 2). These end only their line: bash ends a
      non-interactive shell at them, but a failing line does not stop a
      script here.
    - **No word splitting** (§9.4). An expansion is never split: a value
      with spaces stays one word, where bash splits an unquoted one. An
      unquoted expansion that leaves a word empty removes it; a quoted one
      leaves an empty word. `$@` and `"$@"` give each argument as one word,
      none with no arguments; an empty argument is kept only quoted; text
      joined to them goes with the first and the last argument (`"a$@b"`),
      as bash's `"$@"` does. No variable is set when a shell starts: bash
      sets `HOME`, `PWD`, `PATH` and others, here they read as empty (`~`
      is `/root`). A command whose words expand to nothing runs nothing,
      with status 0, and a redirection after it still makes its file, as
      bash's do; in a pipeline it reads nothing and gives the next command
      an end, and in the background it starts no job.
    - **Where it expands** (§9.4). In command names, arguments and
      redirection targets, so `$C` may name a built-in; an unquoted target
      that expands to nothing is bash's `relay-sh: $f: ambiguous redirect`
      (status 1). Not in comments, nor in a background job's text, which
      stays as typed (as bash's `jobs` shows it), nor in a script's trace,
      `+ <line>`, which shows the line as written (bash's `set -x` shows
      the expanded words): the transcripts keep milestone 1's form. Every
      command of a pipeline is expanded before any starts. A `~` is
      `/root` only alone or before a `/` in the same unquoted piece, as
      bash's is, so `~"/x"`, `~\/x` and `~''` keep it, where milestone 1
      made them `/root`, and so does `~$A`; a `~` that a variable holds
      never expands.
    - **Assignments** (§9.4). A line of only `NAME=value` words sets each
      in turn (`A=1 B=$A`), with status 0. The value expands as a word does
      but always makes one, empty or not (`A=`, `A="a b"`, `A=$B`); an
      unquoted `~` at its start or after a `:` is `/root`, as in bash. A
      word is an assignment only if its name and `=` are unquoted; `1A=x`,
      `A-B=x` and `"A"=x` are command names, as in bash. A redirection
      after the assignments makes its file, and they are made even when it
      cannot be (status 1), as in bash. `A=1 cmd`, which gives bash's
      command an environment, is unsupported (programs get none:
      `unsupported syntax: A=1 before a command`), and so is bash's
      `NAME+=value`, which appends (`unsupported syntax: A+=2`; the review
      found it run as a command, and the user chose the refusal); in a
      pipeline or with
      `&`, where bash's assignment changes nothing in the shell, it is
      refused as a built-in is (`relay-sh: A=1: cannot be used in a
      pipeline`, `… in the background`, status 1).
    - **`$?`** (§9.4). The status of the last line that ran: a pipeline's
      last command's, a background job's start (0, or 127 when nothing of
      it started), `wait`'s, `kill`'s and `jobs`', 130 after Ctrl-C, 2
      after a syntax error, 126 and 127; a blank or comment line keeps it.
      A script starts with 0, and the shell that ran it gets the script's
      status.
    - **Whose variables** (§8.3, §9.4). Variables and arguments belong to
      one shell: a script, `X | sh` and a nested `sh` start with none, and
      nothing passes in or out (bash's, without `export`); the in-process
      runner keeps its own and a script's apart, as `/bin/sh` does. `$0` is
      a script's path as `sh` was given it, and outside a script the
      shell's argument 0, as bash's (`/bin/sh` for the one init starts,
      `sh` for one typed), and `relay-sh` in the in-process runner.
    - **Script arguments** (§8.3). `sh FILE [ARG]...` gives the script its
      arguments under both runners; `/bin/sh` passes them to the child
      shell, which reads them as `$1` on, where milestone 1's `sh` said
      `extra operand`. Options come only before the file, as bash's do, so
      what follows it is the script's (`sh f -x`), and `--` lets a file's
      name start with `-`.
    - **Limits.** A shell's variables, names and values together, hold at
      most 64 KiB, and a line expands to at most 64 KiB, counting one for
      each word as well as their bytes (`spawn` takes at most 64 KiB of
      arguments, §11.1). Beyond either the line stops there, with status 1:
      `relay-sh: B: the variables would hold more than 64 KiB`, `relay-sh:
      the line would expand to more than 64 KiB`. `/bin/sh`'s heap ends the
      program when it runs out, so nothing a person types may grow it
      without bound (`"$@"` many times over many arguments could).
    - **`X | sh` reads a byte at a time** (plan 1's deferred minor). It
      reads a pipe one byte at a time, as bash does, so a command it runs
      reads the rest of the input after its line, not what the shell took
      ahead of it.
    - **Plan 2's deferred minors.** `|&` (bash: both outputs into the pipe)
      is `unsupported syntax: |&` again, rather than bash's syntax error at
      `&`. The stray prompt of an interactive `sh` in a background script
      and bash's `(wd: …)` note go to plan 4, with plan 1's and plan 5's.
    - **Tests** (§12). The parser's parameters, refusals and assignments;
      the expansion against bash 5.2's words for each case, the departures
      named in each test; the shell's variables, `$?` and scripts' scope
      under both runners; the scenario `script_vars` (a script with
      arguments, the prompt, a nested `sh` and `X | sh`).
