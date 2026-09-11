# Relay OS Milestone One Design

## Status

Approved in design review on 2026-09-07.

## Objective

Milestone one delivers a bootable, independent x86_64 operating system whose
project-owned implementation is written in Rust except where architecture
requirements make another language unavoidable. It boots in QEMU and on an
Intel NUC 12 Pro RNUC12WSHI70000 with a Core i7-1260P, presents a framebuffer
command line controlled by a USB keyboard, and persistently reads and modifies
an ext2 filesystem on the same USB drive from which it booted.

The milestone demonstrates a complete native path from the kernel through an
xHCI USB controller to keyboard and storage devices. UEFI is used to start the
OS, not to provide keyboard or storage services after kernel entry.

## Success Criteria

The deliverable is a reproducible GPT disk image that can be flashed to a USB
drive. It contains:

- A FAT32 EFI System Partition with the boot artifact and kernel.
- A 128 MiB writable ext2 root partition.

The same image must boot under the supported QEMU configuration and on the
target NUC. On both targets, a user must be able to navigate the filesystem,
create and read files and directories, modify file contents, remove files and
empty directories, flush writes, shut down cleanly, reboot, and observe that
the data persisted.

## Architecture

Relay OS uses a modular monolithic architecture for milestone one. The kernel
is a single-core, single-address-space, `no_std` Rust program. Drivers, the
filesystem, and the shell have separate internal interfaces but execute with
kernel privilege. There are no processes or user-space programs in this
milestone.

The implementation is divided into the following responsibilities:

- `arch`: x86_64 startup, exceptions, memory mapping, timing, and safe wrappers
  around unavoidable unsafe operations.
- `pci`: PCI enumeration and discovery of the supported xHCI controller.
- `xhci`: xHCI controller ownership, DMA structures, command and transfer
  rings, ports, and event processing.
- `usb`: device enumeration and the supported HID and mass-storage class
  protocols.
- `block`: fixed-sector block reads, writes, and flushes over USB storage.
- `partition`: GPT validation and root-partition selection.
- `vfs`: Unix-like pathname traversal and common file/directory operations.
- `ext2`: the supported on-disk ext2 profile and its VFS implementation.
- `console`: framebuffer text output, line editing, and diagnostic mirroring.
- `shell`: command parsing, current-directory state, and command dispatch.
- Host tooling: reproducible creation of the partitioned bootable image.

Interfaces between these responsibilities must not depend on concrete lower
layers unnecessarily. In particular, ext2 consumes a block-device interface,
VFS consumes a filesystem implementation, and shell commands consume VFS
operations. This allows host tests to substitute memory- or file-backed block
devices and allows later milestones to put commands behind system calls
without replacing the filesystem and driver layers.

## Boot And Initialization

The host image tool creates a GPT disk, formats and populates both partitions,
installs the EFI boot artifact, and records the root partition's GPT unique
GUID in boot configuration.

At startup, the UEFI boot layer loads the kernel and builds a read-only boot
information structure containing the firmware memory map, graphics framebuffer,
and configured root-partition GUID. Before constructing replacement page tables,
it reads `CR4.LA57` and builds a four- or five-level hierarchy matching the
active firmware paging depth without changing that bit in long mode. It changes
`CR3` only from a loader-owned, position-independent transition page whose
physical address is known and identity-mapped in the replacement hierarchy; it
does not assume the UEFI loaded-image base is physical or identity-mapped. The
loader obtains the final UEFI memory map in preallocated storage, performs the
real `ExitBootServices` transition, sorts and normalizes the returned map
without allocation, and publishes it in `BootInfo` before permanently
transferring control to the kernel. The kernel then:

1. Establishes exception handling, memory mapping, physical-page allocation,
   heap allocation, and the framebuffer console.
2. Enumerates PCI and locates a supported xHCI controller.
3. Takes ownership of and resets the controller after firmware handoff.
4. Enumerates supported devices connected directly to xHCI root ports.
5. Finds a supported USB keyboard and USB mass-storage device.
6. Reads GPT from mass storage and selects the partition whose unique GUID
   matches the boot configuration.
7. Validates and mounts the ext2 root filesystem.
8. Starts the interactive shell.

Initialization failures produce a specific framebuffer and serial diagnostic
and halt. The kernel never fallbacks to UEFI keyboard or disk access.

## Supported Hardware Profile

Milestone one deliberately supports a narrow hardware profile:

- x86_64 systems booted through UEFI with Secure Boot disabled.
- The xHCI implementation required by QEMU's emulated controller and the
  target Intel NUC 12 Pro.
- One directly connected USB HID boot-protocol keyboard using a US layout.
- One directly connected USB mass-storage flash drive using Bulk-Only
  Transport and the SCSI transparent command set.
- The graphics framebuffer supplied during UEFI startup.

The USB stack does not promise external hub support, hot-plug recovery,
multiple keyboards, multiple candidate root drives, non-boot HID protocols,
UAS storage, or arbitrary xHCI controller compatibility. Hardware acceptance
uses a specific keyboard and BOT-compatible flash drive known to fit this
profile.

The xHCI driver polls command, transfer, and event rings. The shell event loop
services controller events, converts HID reports into input, and waits for
storage completion when executing filesystem operations. USB interrupts,
asynchronous I/O, and scheduling are deferred.

## Storage Data Flow

A filesystem request follows this path:

`shell -> VFS -> ext2 -> GPT-selected block device -> SCSI -> USB mass storage -> xHCI`

The block interface uses explicit sector ranges and reports short, timed-out,
or failed transfers as errors. Mutating commands return success only after all
dirty filesystem blocks for that operation have been submitted successfully.
`sync` flushes filesystem and device caches. `shutdown` performs the same
flush, marks the filesystem clean, and unmounts it before halting.

USB-drive removal while mounted is unsupported. If storage or ext2 metadata
writing fails, the mount is marked unsafe for further mutations. The shell
reports the failure and rejects later mutating commands so it does not compound
possible damage. An unexpected reset or power loss can require host-side
`e2fsck`, because ext2 has no journal and an in-OS repair utility is outside
this milestone.

## Ext2 Profile

The root partition uses standard ext2 revision 1 with this controlled profile:

- 4 KiB filesystem blocks.
- 256-byte inodes.
- One block group in a 128 MiB partition.
- Directory entries with file-type fields.
- Regular files and directories only.
- Direct and singly indirect inode data blocks.
- Compatible feature bits set to zero, the incompatible `FILETYPE` bit set,
  and read-only-compatible feature bits set to zero.
- No journal.

The maximum supported regular-file size is 4,243,456 bytes: twelve direct
blocks plus 1,024 block references in one singly indirect block. Filesystem
images outside this profile are rejected with a diagnostic rather than mounted
partially. Images written by Relay OS must remain readable and repairable by
standard Linux ext2 tools.

VFS supports `/`-rooted absolute paths, paths relative to the shell's current
directory, repeated separators, `.`, `..`, and ext2 component-length limits.
Interactive input, displayed glyphs, and names are restricted to printable
ASCII in milestone one. File contents are arbitrary bytes when read from disk;
the shell can enter printable ASCII text.

New regular files use mode `0644`, new directories use `0755`, and ownership is
root. Modes and ownership are stored but not enforced because there are no
users or process credentials. Symbolic links, hard links, additional mounts,
sparse files, pipes, redirection, globbing, environment variables, and
executable loading are excluded.

## Shell Contract

The shell provides basic line editing with printable US-layout keys,
Backspace, Enter, and Left/Right movement. Tokens are separated by ASCII
whitespace. Single and double quotes preserve whitespace within one argument,
and backslash escapes the following printable character. There are no shell
expansions or redirections.

Supported commands are:

- `help`: list commands and their usage.
- `pwd`: print the current absolute path.
- `cd PATH`: change the current directory.
- `ls [PATH]`: list names, types, and sizes; default to the current directory.
- `cat PATH`: render printable ASCII plus line-feed, carriage-return, and tab
  bytes; render each other byte as a replacement marker.
- `touch PATH`: create an empty file if absent; an existing regular file is
  left unchanged because wall-clock timestamp management is out of scope.
- `write PATH [TEXT ...]`: create or truncate a regular file and write the
  remaining arguments joined by one ASCII space.
- `append PATH [TEXT ...]`: create a regular file if absent and append the
  remaining arguments joined by one ASCII space.
- `mkdir PATH`: create one directory whose parent already exists.
- `rm PATH`: remove one regular file.
- `rmdir PATH`: remove one empty directory other than `/`.
- `echo [TEXT ...]`: print arguments joined by one ASCII space.
- `sync`: flush filesystem and device caches.
- `shutdown`: flush, cleanly unmount, and halt.

Commands do not recursively create or remove entries. They return concise
errors for invalid syntax, missing entries, wrong entry types, non-empty
directories, invalid names, full filesystems, unsupported file sizes, and I/O
failures. An error returns control to the prompt unless continued operation
would be unsafe.

## Error Handling And Diagnostics

Subsystems expose typed errors and do not panic for expected device,
filesystem, input, or resource failures. USB operations have bounded timeouts
and limited retries. Parsers treat USB descriptors, GPT data, and ext2 metadata
as untrusted and bounds-check offsets, lengths, counts, and arithmetic.

An unrecoverable kernel invariant failure displays a panic diagnostic on the
framebuffer, mirrors it to the QEMU serial log when available, and halts. The
console normally mirrors output to serial in QEMU so automated tests can
observe the same results as the framebuffer. Serial is diagnostic only and is
not required for physical use.

## Rust And Dependency Policy

Project-owned logic is implemented in Rust. Non-Rust code is allowed only when
an architecture or toolchain requirement makes it unavoidable, and each such
case must be documented. Unsafe Rust is expected around startup, privileged CPU
instructions, MMIO, DMA, and page tables. It is isolated in small modules, and
each unsafe block documents the invariant that makes it valid. Path,
filesystem, shell, parsing, and most protocol logic remain safe Rust.

DMA allocation enforces xHCI alignment, physical-contiguity, addressability,
lifetime, and memory-visibility requirements in one audited boundary.

Open-source Rust crates may be used where they reduce risk or substantial
effort. Each runtime dependency must be:

- Maintained and compatible with `no_std` where its target requires it.
- License-compatible and narrowly feature-configured.
- Pinned through the repository lockfile.
- Checked for relevant RustSec advisories.
- Reviewed for its unsafe-code surface and fit with the supported hardware.

A dependency with a relevant known vulnerability is rejected, patched, or
replaced. If no suitable crate exists, Relay OS implements the smallest needed
component locally rather than adopting an unsuitable high-level dependency.
CI runs dependency and advisory checks.

Milestone one does not claim hostile-user isolation: shell commands and all
drivers run with kernel privilege. Memory-safe parsing and narrow unsafe
boundaries protect kernel correctness, while privilege separation is deferred
to a later process milestone.

## Testing Strategy

Logic that does not inherently require privileged hardware access is separated
for host testing. Unit tests cover:

- Shell tokenization and command validation.
- Path normalization and VFS traversal rules.
- GPT parsing and partition selection.
- Ext2 allocation, file growth, directory insertion/removal, and error cases.
- USB descriptor and HID report parsing.
- SCSI command and response handling.
- xHCI ring and context bookkeeping.

Filesystem integration tests use disposable file-backed block devices. They
create, mutate, flush, reopen, and verify images, then run standard `e2fsck`
against the result. Fault-injecting block devices exercise short I/O, timeouts,
capacity exhaustion, and write failures.

Loader host tests cover `CR4.LA57` paging-depth selection, four- and five-level
index traversal, canonical transition-page addresses, executable
identity-mapping of the transition page, final-map sorting and overlap
rejection, and conditional NX enablement. The production QEMU boot gate requires
the kernel banner after real `ExitBootServices`; it does not accept a UEFI
fallback path.

Automated QEMU tests use a `q35` machine with UEFI, an emulated xHCI
controller, a USB keyboard, and the boot image attached as USB mass storage.
The harness boots the production image, injects keyboard commands, observes
mirrored console output over serial, shuts down cleanly, reboots the same
image, and confirms persisted content. It also covers malformed or missing
root metadata and unavailable supported devices where QEMU can model them.

CI quality gates include formatting, linting, host unit and integration tests,
dependency advisories, image creation, and the QEMU smoke and persistence
paths.

## Physical Acceptance Test

With Secure Boot disabled, one supported keyboard and one supported flash drive
are connected directly to the Intel NUC 12 Pro. The flashed milestone image
must:

1. Complete the real `ExitBootServices` handoff and display the post-UEFI kernel
   banner without a serial terminal; record the NUC firmware version, tested
   image SHA-256, and a banner photo.
2. Boot from USB to a visible prompt without a serial terminal.
3. Successfully exercise every documented shell command.
4. Create, list, navigate, read, append, and remove representative files and
   directories.
5. Run `sync` and `shutdown` without an I/O or filesystem error.
6. Boot again from the same USB drive and read content retained from the prior
   session.
7. Leave an ext2 partition that passes host `e2fsck` after clean shutdown.

Milestone one is complete only when automated QEMU verification and this NUC
acceptance test both pass.

## Explicit Non-Goals

The milestone does not include processes, user mode, privilege separation,
multitasking, executable loading, multiple CPU cores, networking, audio, a GUI,
USB hubs, broad USB-device compatibility, NVMe, SATA/AHCI, Secure Boot,
filesystem journaling or repair, ext2 features outside the fixed profile, or
support guarantees for machines other than the specified QEMU configuration
and NUC acceptance target.

Later milestones can introduce processes and syscalls, then expand storage to
NVMe and SATA behind the existing block-device interface without replacing VFS
or ext2.
