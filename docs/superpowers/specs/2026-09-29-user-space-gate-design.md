# Relay OS — User-Space Gate Design: programs, not built-ins (milestones 2 and 3)

- **Date:** 2026-09-29
- **Status:** Approved 2026-09-29; revised while planning milestone 2's plans 1, 2 and 3a (see §16)
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
  `[ ok ] system: 29 programs, ABI 1` or `[FAIL] system: <reason>`
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
  QEMU with KVM passes them through, TCG may not). The kernel touches user
  memory only inside `UserSlice` copies, between `stac` and `clac`.
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
  (milestone 3's background jobs, §9.2).
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
  included) also goes to every file on the tee stack (at most 4). `console_tee_push`
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

The kernel prints `/etc/motd` (M1 §4.4 step 10) and starts `/bin/sh` as
process 1 in `/root`. If process 1 ends, the kernel logs how and
starts it again. If it ended three times within 10 s, or cannot be started
at all, the kernel shows the error screen of §11.2 instead.

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
| `spawn` | `&SpawnArgs` → pid | `SpawnArgs`: path, argument bytes (NUL-separated), working directory, up to 8 `(child_fd, parent_fd)` pairs (unlisted child fds are closed), flags (`NEW_GROUP`) |
| `wait` | pid or −1 for any child, flags (`NOHANG`), `&mut WaitStatus` → pid, or 0 with `NOHANG` and nothing finished | `WaitStatus`: `Exited(code)` or `Killed(reason, fault kind, address, ip)`; `ECHILD` with no such child |
| `kill` | pid, or −pgid for a group → 0 | `ESRCH`; `EPERM` for process 1 |
| `getpid` | → pid | |
| `proc_list` | buffer → count | M3: `ProcInfo` per process: pid, ppid, pgid, state, frames, CPU ticks, name (up to 64 bytes) |
| `mem_map` | length → address | length rounded up to pages; `ENOMEM` (§11.1) |
| `mem_unmap` | address, length → 0 | whole pages of earlier `mem_map`s only; `EINVAL` otherwise |
| `open` | path, flags → fd | flags: `READ`, `WRITE`, `CREATE`, `TRUNCATE`, `APPEND`, `EXCLUSIVE`, `DIRECTORY` |
| `close` | fd → 0 | |
| `read`, `write` | fd, buffer → bytes | |
| `seek` | fd, offset `i64`, whence (start, current, end) → new offset | on the console or a pipe: `EINVAL` |
| `fstat` | fd, `&mut Stat` → 0 | |
| `stat` | path, `NOFOLLOW` flag, `&mut Stat` → 0 | |
| `read_dir` | directory fd, buffer → bytes | packed `DirEntry` records (inode, type, name length, name), continuing where the last call stopped |
| `mkdir`, `rmdir`, `unlink`, `truncate`, `touch`, `readlink` | as the `Vfs` trait | one call per `Vfs` operation |
| `rename` | from, to → 0 | |
| `statfs` | path, `&mut StatFs` → 0 | |
| `sync` | → 0 | syncs every filesystem and every tee |
| `chdir`, `getcwd` | path / buffer | |
| `console_mode` | raw or line → previous mode | |
| `console_size` | → columns and rows | |
| `console_foreground` | pgid → 0 | |
| `console_tee_push`, `console_tee_pop` | fd / none → 0 | §6.5 |
| `time` | `&mut Time` → 0 | Unix seconds and uptime in nanoseconds |
| `sleep` | milliseconds → 0 | |
| `sys_info` | kind, buffer → bytes | kinds: memory (`MemInfo` of M1's `free`), `uname` fields, kernel log (for `dmesg`) |
| `power` | reboot or poweroff → error only | the kernel syncs and shuts the filesystems down first (M1 §7.4); when that fails it returns the error and the machine stays up, as M1's `unplug` scenario requires |
| `pipe` | `&mut [fd; 2]` → 0 | M3 (§9.1) |

`read` and `write` on the console and on pipes may return fewer bytes than
asked. The kernel reaches the files through the same `MountTable` the
in-kernel shell uses today; there is one table, shared by all processes,
with the current directory switched per process (§5.4).

### 7.4 Versioning

`relay_abi::VERSION` (`u32`, 1 for 0.3.0) changes whenever a call's meaning
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
  built-in and not in `/bin` is `sh: <name>: command not found` (status
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
`seq`, `sleep`, `true`, `false` and `ps` (§9). Every command prints exactly
what it prints in milestone 1. They are built with the `relay` profile plus
LTO.

### 8.5 Test programs (`userland/tests`)

Shipped in every `system.img`, because the NUC checks use them too:

| Program | Does |
|---|---|
| `t-fault KIND` | faults on purpose: `null-read`, `null-write`, `write-code`, `exec-data`, `ud`, `div0`, `stack`, `kernel-read` (reads a kernel address), `sse` |
| `t-spin [secs]` | spins without system calls, forever or for `secs` seconds (reading the clock only every 2^20 iterations), then prints how many iterations it made |
| `t-spawn N` | starts N children that exit at once and waits for each; prints the free frames before and after |
| `t-abi` | a program whose ELF note has the wrong ABI version (built by xtask) |
| `t-args` | prints its arguments one per line, as `[n] <arg>` |

## 9. Milestone 3: pipes, jobs, `ps`/`kill`, script variables

### 9.1 Pipes

- `pipe` makes a pipe with a 16 KiB ring buffer. `read` blocks while the
  pipe is empty and a writer is open, and returns 0 once all writers have
  closed. `write` blocks while it is full, and returns `EPIPE` once all
  readers have closed.
- The parser accepts `a | b | c`. The shell makes the pipes, spawns every
  stage in one new process group with the pipe ends mapped to fds 0 and 1,
  closes its own copies, and waits for every stage. `$?` is the last
  stage's status. `>` and `>>` apply to the last stage. A built-in in a
  pipeline is refused with `sh: cd: cannot be used in a pipeline`.
- `cat`, `wc`, `head` and `tail` read standard input when given no file;
  `Ctx` gains an input for that. On the console that input is line mode.
- New programs: `grep [-i] [-v] [-n] [-c] PATTERN [FILE...]` (a pattern is
  literal characters, `.`, `*`, `^`, `$` and `[...]` with ranges and `^`),
  `seq [FIRST] LAST`, `sleep SECONDS`, `true`, `false`.

### 9.2 Background jobs

- `cmd &` or `a | b &` starts a job (its own process group) without making
  it the foreground; the shell prints `[n] <pid of the last stage>`.
- Before each prompt the shell collects finished children with
  `wait(-1, NOHANG)` and reports each finished job as
  `[n]+ Done  <command>` (or `Exit <code>`, `Killed`).
- `jobs` lists the jobs: `[n]+ Running  <command> &`.
- `wait` waits for every job; `wait %n` or `wait PID` for one, and sets `$?`.
- A background process reading the console gets end of input at once; its
  output goes to the screen.
- There is no `fg`, `bg` or Ctrl-Z.

### 9.3 `ps` and `kill`

- `/bin/ps` prints `PID PPID STATE MEM TIME CMD` (MEM in KiB, TIME as
  `m:ss` of CPU time) from `proc_list`.
- `kill PID...` or `kill %n...` (built-in, since `%n` needs the job table)
  kills processes or a whole job. Killing process 1 is refused:
  `kill: (1) - Operation not permitted`.

### 9.4 Script arguments and variables

- `sh FILE a b` sets `$0` (the script's path), `$1`…`$9`, `$#` and `$@`
  (all arguments, each one word). `$?` is the status of the last command or
  pipeline.
- `NAME=value` alone on a line sets a shell variable (a name is
  `[A-Za-z_][A-Za-z0-9_]*`); `$NAME` and `${NAME}` read it; an unset
  variable reads as empty. Variables work at the prompt too.
- Expansion happens outside quotes and inside double quotes, not inside
  single quotes. Its result is never split into words: a value with spaces
  stays one argument (unlike bash). An unquoted expansion that is empty
  produces no word; a quoted one produces an empty word.
- Variables belong to the shell; programs get no environment.
- Still refused: `` ` ``, `$(`, `$((`. Not supported: `&&`, `||`, `if`,
  loops, `export`.

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
  `sh: <command>: killed (page fault at 0x0, read, ip 0x401a2c)`, and
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
`Press any key to reboot.` The kernel then syncs and reboots through the
ACPI path of M1 §7.4. There is no in-kernel fallback shell.

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
| 3 | `pipes` | `cat` of the 8 MiB file through `wc -c` gives the exact size; `cat big \| head -n 1` ends at once; `seq 5 \| grep -c .` prints 5 |
| 3 | `jobs` | `sleep 5 &` and `t-spin &`; `jobs` and `ps` show both; `kill %2`; `wait`; the Done lines |
| 3 | `script_vars` | a script run with arguments prints `$0`, `$1`, `$#`, `$@`, `$?` and variables, with and without quotes |

### 12.4 NUC checks

- `check3-a.sh` and `check3-b.sh` pass as they are, apart from the new
  startup line in their `dmesg` expectations, now with every command in user
  space.
- `check4.sh` (milestone 2): the `t-fault` kinds, `t-spawn`, `free` before
  and after, `t-args`, and a script that runs another. One manual step
  follows: run `t-spin` and press Ctrl-C (a script cannot type).
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
4. **The shell moves out.** `relay-rt`'s trait implementations, the
   `Runner` split, `/bin/sh`, the programs, process 1 and its respawn rule;
   the kernel drops `shell`; `check4.sh`. NUC check 4.
5. **Hardening and 0.3.0.** Fixes from the NUC checks, the spec's §16 up to
   date, version 0.3.0.

**Milestone 3**

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
| Polling input on every ring-3 tick costs too much | Measured on the NUC in check 4 (`t-spin` loop count with and without); if needed, poll every 4th tick |
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
     `t-spin 5`, and Ctrl-C of `t-spin`). The recorded transcripts get
     those lines by hand until plan 3a's NUC check records real ones.
