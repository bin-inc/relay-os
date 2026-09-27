# Relay OS — Milestone 1 Design: USB boot, HDMI terminal, persistent file I/O

- **Date:** 2026-09-26
- **Status:** Approved 2026-09-26; revised during planning (see §15)
- **Branch:** `milestone-1`

## 1. Intent

Relay OS is a new operating system written from scratch in Rust. Milestone 1 is
the first end-to-end result: the machine boots from a USB stick, shows a
terminal on the HDMI monitor, and the user types Linux-style commands on a USB
keyboard to create, read, change and delete files. Those files are kept on the
stick across reboots.

### 1.1 What the owner specified

- Written in Rust wherever possible. Another language only where the
  architecture makes it unavoidable.
- Open-source Rust components are allowed. We rewrite or replace them when they
  don't fit our use case.
- Boots from USB on the target NUC and shows a terminal over HDMI.
- The user can do simple file reads and writes. Command names follow Linux.
- Filesystem looks like Linux: `/` is the root and `/` is the path separator.
- Automated testing in QEMU. Manual testing on the physical NUC.

### 1.2 Decisions made during brainstorming

| Topic | Decision |
|---|---|
| Earlier attempt (`images/12-…` to `images/19-…`) | Ignored. This is a clean slate. |
| Persistence | Files written in Relay OS are stored on the USB stick and survive reboot. |
| Root filesystem format | ext2, written by us, mounted as `/`. |
| Boot architecture | Our own UEFI loader (`relay-boot`) hands off to a separate higher-half kernel ELF (`relay-kernel`). Both are Rust. |

### 1.3 Assumptions (correct these in review if they're wrong)

- Only x86_64. Only one CPU core is used.
- No user mode and no processes. The shell runs inside the kernel, but reaches
  files only through a VFS trait, so it can move to user space later without
  changes.
- No networking, no audio, no USB hubs, no mouse.
- Single user, `root`. Permission bits are stored and displayed but not
  enforced.
- US keyboard layout. The host's `/etc/default/keyboard` says `us`.
- The CMOS real-time clock holds UTC.

### 1.4 Success criteria (definition of done)

1. `cargo xtask test` passes: host unit tests, the QEMU end-to-end scenarios
   (§9.3), and `e2fsck -fn` on every disk image those scenarios leave behind.
2. On the NUC, the `docs/hardware-test.md` checklist passes using a stick
   written by `cargo xtask flash --full`:
   - F10 → "UEFI: Kingston" boots to the shell with every startup line `[ ok ]`.
   - The listed commands behave as described.
   - A file written on the NUC is still there after `reboot`.
   - Back in Linux Mint, `cargo xtask verify-usb` reports a clean filesystem
     containing that file.
3. Apart from the bitmap font, every OS-side component comes from this
   repository or from the approved crates listed in §3.2.

## 2. Target hardware and test environment

Facts gathered from the host (the NUC runs Linux Mint and also hosts
development):

| Item | Value | Design consequence |
|---|---|---|
| Machine | Intel NUC 12 Pro `NUC12WSHi7`, board `NUC12WSBi7`, Core i7-1260P, 16 GB RAM | x86_64, Alder Lake. TSC frequency is available from CPUID 0x15/0x16. |
| Firmware | `WSADL357.0090.2023.0821.1714`, UEFI, Secure Boot **disabled**, no legacy BIOS/CSM | The OS must boot as a UEFI application. The loader doesn't need to be signed. |
| Boot menu | F10 at power-on (F2 opens firmware setup) | Boot the stick through the removable-media path, leaving the Mint boot order untouched. |
| GPU / display | Iris Xe `8086:46a6`, monitor on `HDMI-A-1` | Use the UEFI GOP framebuffer. We don't write an Intel GPU driver. |
| USB controller | PCH xHCI `00:14.0` `8086:51ed`, BAR0 at physical `0x603D180000` (above RAM) | One xHCI driver serves both the keyboard and the stick. MMIO must be mapped on demand (§5.3). |
| Keyboard | Logitech K120 `046d:c31c`, USB 2 bus 3 port 3, low-speed. There is also a Logitech Unifying receiver `046d:c534` on port 1. | Needs a USB HID boot-keyboard driver, because there is no PS/2. |
| USB stick | Kingston DataTraveler 3.0 `0951:1666`, 14.4 GB, USB 3 bus 4 port 3 (SuperSpeed), `/dev/sda` | Needs USB Mass Storage (BOT + SCSI). The driver must handle both USB 3 and USB 2 port reset. |
| Stick serial | Reported serial is **`08606E6D413FB27127135F8E`**, not the `0B82670162E7` in the original request | The flash tooling matches on the reported serial (§9.2). |
| Internal disk | Samsung 980 NVMe, running Mint | Must never be written by our tooling. |
| Host tools | Rust 1.98.1 stable, QEMU 8.2.2, OVMF (`/usr/share/OVMF/OVMF_CODE_4M.fd`, `OVMF_VARS_4M.fd`), mtools, e2fsprogs | Enough for building, making images and running end-to-end tests. |

## 3. Repository layout and components

```
relay-os/
├─ rust-toolchain.toml     pinned stable; targets x86_64-unknown-uefi, x86_64-unknown-none
├─ Cargo.toml              workspace
├─ boot/                   relay-boot   – UEFI loader, output is BOOTX64.EFI
├─ kernel/                 relay-kernel – freestanding higher-half kernel ELF
├─ crates/
│  ├─ boot-info/           #[repr(C)] BootInfo shared by loader and kernel (no dependencies)
│  ├─ ext2/                ext2 read/write over a BlockDevice trait     (no_std + alloc)
│  ├─ vfs/                 paths, mount table, inode/file traits, errno (no_std + alloc)
│  ├─ shell/               line editor, parser, built-in commands       (no_std + alloc)
│  ├─ term/                text grid, ANSI subset, glyph rendering      (no_std + alloc)
│  └─ usb/                 xHCI, HID boot keyboard, BOT/SCSI over a Hal trait (no_std + alloc)
├─ rootfs/                 source files for the initial / tree
├─ tests/e2e/              QEMU scenario scripts
├─ xtask/                  host tool: build, image, qemu, test, flash, verify-usb
└─ docs/
   ├─ hardware-test.md
   └─ superpowers/specs/
```

### 3.1 Principles

- **Pure logic is in crates that also build for the host.** `ext2`, `vfs`,
  `shell`, `term` and `usb` are `no_std + alloc` and are tested with
  `cargo test` on Linux. They reach hardware only through traits
  (`BlockDevice`, `Hal`, `Vfs`, pixel-buffer and console traits).
- **The kernel is a thin layer that wires hardware to those crates.** It holds
  the architecture code, memory management, ACPI, PCI, the timer, the concrete
  `Hal`, GPT parsing and the startup sequence.
- **One job per unit.** Each crate can be understood through its public trait
  surface without reading its internals.
- **Stable Rust only.** Interrupt and exception entry stubs are written as
  `#[unsafe(naked)]` functions with `naked_asm!`, which are stable, instead of
  the nightly-only `x86-interrupt` ABI. All assembly is inline (`asm!`,
  `naked_asm!`, `global_asm!`) in Rust files. There are no separate `.S`
  files.
- **Kernel link settings:** a linker script places it at
  `0xFFFF_FFFF_8000_0000`, built with `-C code-model=kernel` and
  `-C relocation-model=static`.

### 3.2 Crate policy

| Scope | Allowed third-party crates |
|---|---|
| `boot/` | `uefi`, `x86_64`, `bitflags` |
| `kernel/`, `crates/*` | `x86_64` (register and page-table types), `bitflags`, `spin` |
| Font data | Spleen 8×16 bitmap font (BSD-2-Clause), embedded as a generated Rust array. The license is kept in `crates/term/fonts/LICENSE`. |
| `xtask/` and dev-dependencies | Anything useful (for example `anyhow`, `clap`, `serde_json`, `tempfile`, `proptest`/`rand`). |

Everything else on the OS side is written in this repository. That includes
the heap allocator, the frame allocator, ACPI table parsing, PCI, xHCI, HID,
BOT/SCSI, GPT, ext2, VFS, the terminal and the shell.

## 4. Boot flow

### 4.1 Disk layout

GPT with two partitions:

| # | Type | Size | Contents |
|---|---|---|---|
| 1 | EFI System (FAT32, label `RELAYESP`) | 64 MiB | `\EFI\BOOT\BOOTX64.EFI`, `\EFI\RELAY\kernel.elf`, optional `\EFI\RELAY\cmdline` |
| 2 | Linux filesystem (ext2, label `relayroot`) | rest of the disk (image: about 190 MiB; stick: about 14.3 GB) | the `/` tree |

### 4.2 `relay-boot` (UEFI application)

1. **Read files.** Read `\EFI\RELAY\kernel.elf` and, if present,
   `\EFI\RELAY\cmdline` (one line, at most 255 bytes) from the ESP it was
   loaded from.
2. **Parse the kernel.** Load the ELF64 `PT_LOAD` segments into freshly
   allocated pages, zeroing `.bss`. Reject anything that isn't an x86_64 ELF64
   executable linked at the expected base.
3. **Choose a display mode (GOP).**
   - Prefer the mode the firmware selected when it matches the monitor's
     native resolution.
   - Otherwise take the largest 32-bpp mode no bigger than 1920×1080.
   - If neither exists, keep the current mode.
   - A `video=WxH` entry in `cmdline` overrides these rules when that mode
     exists.
   - Refuse to continue (and print a message on the UEFI console) if no
     linear 32-bpp framebuffer is available.
4. **Collect information for the kernel.**
   - The ACPI 2.0 RSDP from the configuration table.
   - The ESP **partition GUID**, taken from the HardDrive media node of the
     loaded image's device path.
5. **Build page tables.**
   - Kernel segments at `0xFFFF_FFFF_8000_0000`, with permissions taken from
     the ELF flags (NX on data).
   - All RAM in the memory map, plus the framebuffer, mapped linearly at
     `PHYS_OFFSET = 0xFFFF_8000_0000_0000` using 2 MiB pages.
   - A 64 KiB kernel stack with an unmapped guard page below it.
   - An identity mapping of the trampoline page.
6. **Hand off.**
   - Allocate `BootInfo` and the memory-map buffer as `LOADER_DATA`.
   - Fetch the final memory map and call `ExitBootServices`.
   - Translate the memory map into `BootInfo`.
   - Jump to the trampoline, which loads CR3 and RSP and calls
     `kernel_main(&'static BootInfo)`.

### 4.3 `BootInfo` (crate `boot-info`)

`#[repr(C)]`, starting with `magic: u64` and `version: u32`. The kernel checks
both and halts with a message if they don't match.

| Field | Contents |
|---|---|
| `framebuffer` | Physical address, byte size, width, height, stride (in pixels), pixel format (RGB or BGR). |
| `memory_map` | Pointer and length of `MemoryRegion { start, len, kind }`, where `kind` is `Usable`, `Bootloader`, `AcpiReclaimable`, `AcpiNvs`, `Reserved`, `Mmio` or `Kernel`. UEFI boot-services code and data become `Usable`. Loader data becomes `Bootloader`. |
| `rsdp_addr` | Physical address of the RSDP. |
| `phys_offset` | Virtual base of the linear physical-memory map. |
| `kernel_phys` | Start and length of the kernel image in physical memory. |
| `cmdline` | `[u8; 256]` plus a length. |
| `boot_partition_guid` | `[u8; 16]` |

### 4.4 Kernel startup sequence

Each numbered step prints `[ ok ] <step>` or `[FAIL] <step>: <reason>`. The
last line on screen therefore shows where a hang happened, which matters
because the NUC has no serial port.

1. **Early console:** framebuffer text rendering (§7.1), mirrored to COM1
   (0x3F8) when a UART is detected by probing its scratch register (so it
   works in QEMU and is harmless on the NUC).
2. **CPU tables:** GDT with TSS (an IST stack for double faults), then an IDT
   whose exception handlers draw the panic screen (§10).
3. **Memory:** frame allocator (§5.1), kernel heap (§5.2), then kernel-owned
   page tables that replace the loader's (§5.3). Set up PAT so the framebuffer
   is mapped write-combining.
4. **ACPI** (§5.4).
5. **Timer:** find the TSC frequency from CPUID 0x15/0x16, or measure it
   against the HPET if those leaves are missing (as in QEMU). The LAPIC timer
   in periodic mode provides a 1 kHz tick, used for the uptime clock,
   timeouts and key repeat.
6. **RTC:** read the CMOS clock for wall-clock time.
7. **PCI:** enumerate devices through ECAM (§5.5).
8. **USB:** bring up the xHCI controller and attach devices (§6).
9. **Storage:** read the GPT and select the root partition (§6.5), then mount
   ext2 at `/` (§8).
10. **Shell:** print `/etc/motd` and start the shell in `/root` (§7.3).

If a device-level step fails, the kernel keeps going (see §10).

## 5. Kernel core

### 5.1 Frame allocator

A bitmap over every `Usable` region, at 4 KiB granularity, stored in frames
taken from the first sufficiently large `Usable` region.
- `alloc(count, align)` returns physically contiguous frames. DMA buffers and
  the xHCI scratchpad need this.
- `Bootloader`, `AcpiReclaimable` and `Kernel` regions stay reserved for all
  of milestone 1.

### 5.2 Kernel heap

A fixed 32 MiB virtual region at `0xFFFF_C000_0000_0000`, backed by frames when
the heap is set up.

The allocator is our own, registered as `#[global_allocator]`:
- **Small blocks:** size classes 16 B to 2 KiB, each served from a free list
  carved out of 4 KiB slabs.
- **Larger blocks:** an address-ordered first-fit free list that merges
  adjacent free blocks.
- Running out of heap memory panics with a clear message.

### 5.3 Paging and MMIO

The kernel builds its own PML4. It copies the kernel mappings and the RAM linear
map, and removes the identity-mapped trampoline.

- **`map_mmio(phys, len, cache)`** maps device memory into the linear region at
  `PHYS_OFFSET + phys`, marked uncached or write-combining. This is required
  because the NUC's xHCI BAR (`0x603D180000`) is above the top of RAM, so the
  loader's RAM map doesn't cover it.
- **Supported cache types:** write-back (RAM), uncached (MMIO) and
  write-combining (framebuffer, through PAT entry 1).

### 5.4 ACPI (tables only, no AML interpreter)

- **RSDP → XSDT:** validate checksums.
- **MCFG:** ECAM base address and bus range.
- **HPET:** base address, used only to measure the TSC.
- **FADT:**
  - the reset register (for `reboot`);
  - the PM1a/PM1b control block addresses (for `poweroff`);
  - the DSDT address.
- **DSDT `\_S5`:** found by scanning the raw bytes for the `_S5_` package and
  decoding its `SLP_TYPa` and `SLP_TYPb` values. If it isn't found, `poweroff`
  falls back as described in §7.4.

### 5.5 PCI

Enumerate every bus in the MCFG range through ECAM. Handle multi-function
devices, and decode 32-bit and 64-bit BARs (sizing them by writing all ones
and reading back).

The result is a device list that `dmesg` prints and the xHCI driver uses to
find its controller (class `0x0C`, subclass `0x03`, prog-if `0x30`). Enable
memory-space decoding and bus mastering on that controller.

## 6. USB stack (`crates/usb`)

### 6.1 `Hal` trait

The only way the crate reaches hardware. The kernel implements it, and host
tests use a fake.

```rust
pub trait Hal {
    fn map_mmio(&self, phys: u64, len: usize) -> NonNull<u8>;
    fn alloc_dma(&self, size: usize, align: usize) -> DmaBuf; // virt + phys, zeroed
    fn free_dma(&self, buf: DmaBuf);
    fn now(&self) -> Duration;      // monotonic time since boot
    fn sleep(&self, d: Duration);   // busy-wait on the timer
}
```

### 6.2 xHCI controller driver

**Hardware quirks.** Each of these is its own function with a unit test
against a fake register file:

- **BIOS handoff:** find the USB Legacy Support extended capability (ID 1).
  Set the OS-owned semaphore, wait up to 1 s for the BIOS semaphore to clear,
  then turn off the SMI enables. Without this, the NUC firmware's SMM keyboard
  emulation keeps interfering with the controller.
- **Reset:** clear Run/Stop, wait for HCHalted, set HCRST, then wait for both
  HCRST and CNR to clear. Each wait times out after 1 s.
- **Scratchpad buffers:** read `Max Scratchpad Buffers` from HCSPARAMS2 and
  allocate the scratchpad array and pages. Intel requires them; QEMU requires
  none.
- **Context size:** `HCCPARAMS1.CSZ` chooses 32-byte (QEMU) or 64-byte (Intel)
  contexts. Every context accessor takes that stride as a parameter.
- **Supported Protocol capabilities:** tell us which ports are USB 2 and which
  are USB 3, so the right reset sequence runs. USB 3 ports train on their own;
  USB 2 ports need a PORTSC reset followed by waiting for Port Enabled.

**Data structures:**
- the DCBAA;
- one command ring;
- one event ring (primary interrupter, a single-segment ERST);
- one transfer ring per endpoint.

Rings hold 256 TRBs and use a Link TRB with toggle-cycle. Cycle-bit handling is
covered by host unit tests.

**Device setup sequence:**
1. Detect the connection change on the port and reset the port.
2. Enable Slot.
3. Address Device, with a max packet size of 8, 64 or 512 depending on port
   speed.
4. `GET_DESCRIPTOR(Device)`, correcting EP0's max packet size if needed.
5. `GET_DESCRIPTOR(Configuration)` (full length).
6. Hand the parsed interfaces to class drivers (§6.3, §6.4).
7. For each claimed interface, run Configure Endpoint and
   `SET_CONFIGURATION`.

**Event handling:** `poll()` processes events from the event ring. It updates
the Event Ring Dequeue Pointer and sends completions to the waiting transfer
by matching the TRB pointer. It runs:
- from the shell's idle loop;
- from every blocking wait (control and bulk transfers).

We use no interrupts in milestone 1.

**Timeouts:** commands 500 ms; control transfers 1 s; bulk transfers 5 s.
When a timeout expires, the driver aborts the ring (Stop Endpoint, then Set TR
Dequeue Pointer) and returns `UsbError::Timeout`.

**Scope limits:** devices on root ports only (no hubs), and only the first
configuration of each device. Unclaimed devices and interfaces are logged and
ignored.

### 6.3 HID boot keyboard driver

- **Claiming:** claim every interface with class 3, subclass 1, protocol 1.
  That covers both the K120 and the Unifying receiver's keyboard interface.
  Send `SET_PROTOCOL(0 = boot)` and `SET_IDLE(0)`.
- **Reading reports:** keep one 8-byte interrupt-IN transfer queued at all
  times.
- **Converting reports to events:**
  - Compare each report with the previous one to produce `KeyDown` and
    `KeyUp` events.
  - Ignore phantom (rollover-error) reports where every key slot is `0x01`.
- **Keymap:** HID usage codes are translated with a US layout table.
  - Modifiers: Shift, Ctrl and Caps Lock (Alt is recognised but has no
    function yet).
  - Caps Lock state is shown on the keyboard LED with `SET_REPORT(Output)`.
- **Auto-repeat:** done in software, 500 ms delay then 30 per second, driven by
  the timer.
- **Output:** a queue of `KeyEvent { key, modifiers, pressed }` that the
  console input layer (§7.2) reads.

### 6.4 Mass Storage driver (Bulk-Only Transport + SCSI)

- **Claiming:** interfaces with class 8, subclass 6, protocol `0x50`. Use one
  bulk-IN and one bulk-OUT endpoint. Read `GET_MAX_LUN` (default 0) and use LUN
  0 only.
- **Transport:** CBW, then data, then CSW, with tag checks and residue
  handling.
- **Setup commands, in order:**
  1. `INQUIRY`, logged as vendor and product.
  2. `TEST UNIT READY`, retried for up to 5 s while the stick powers up. A
     `REQUEST SENSE` after each failure clears the unit-attention condition.
  3. `READ CAPACITY(10)`, switching to `READ CAPACITY(16)` if the result is
     `0xFFFFFFFF`.
- **I/O commands:** `READ(10)` and `WRITE(10)`, split into chunks of at most
  64 KiB; `SYNCHRONIZE CACHE(10)` for `flush()`.
- **Error recovery:**
  - A failed CSW status triggers `REQUEST SENSE`, and the sense key is logged.
  - A stall on a bulk endpoint triggers `CLEAR_FEATURE(ENDPOINT_HALT)`.
  - A phase error or an invalid CSW triggers BOT reset recovery: Bulk-Only
    Mass Storage Reset, then clear both halts.
  - Each operation is retried at most twice, then returns `EIO`.

### 6.5 `BlockDevice` trait and GPT

Shared trait (defined in `vfs`, implemented by the USB storage driver and by
file-backed devices in host tests):

```rust
pub trait BlockDevice {
    fn block_size(&self) -> usize;              // 512 for the stick
    fn block_count(&self) -> u64;
    fn read(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), IoError>;
    fn write(&mut self, lba: u64, buf: &[u8]) -> Result<(), IoError>;
    fn flush(&mut self) -> Result<(), IoError>;
}
```

**GPT parsing** (in the kernel's `block` module):
- Read the primary header and check its CRC32. If the check fails, use the
  backup header instead.
- Read the partition entry array.
- Expose each partition as a `BlockDevice` that adds an offset and checks
  bounds.

**Choosing the root filesystem:**
- The root disk is the one whose GPT contains the `boot_partition_guid` from
  `BootInfo`.
- On that disk, the first partition with the Linux filesystem type GUID
  (`0FC63DAF-8483-4772-8E79-3D69D8477DE4`) is mounted as `/`.

**Fallback:** if no disk matches the boot GUID, use the first disk that has
exactly one ESP and one Linux partition, and log a warning.

## 7. Console, terminal and shell

### 7.1 Terminal (`crates/term`)

- **Screen model:** a grid of `Cell { ch, fg, bg }` plus the cursor position.
- **Escape handling:** a small parser for:
  - SGR colours: 16 colours, bold as bright, reset;
  - `ED`/`EL` (clear screen and clear line);
  - `CUP`/`CUF`/`CUB` (cursor positioning);
  - `\r`, `\n`, `\b`, `\t` (tab stops every 8 columns).
- **Rendering:**
  - Glyphs are drawn into a **shadow buffer** in RAM.
  - Rows marked dirty are copied to the framebuffer, which is mapped
    write-combining. We never read from the framebuffer.
  - Scrolling moves rows within the shadow buffer and marks the whole screen
    dirty.
- **Font:** Spleen 8×16, drawn at 2× when the framebuffer is at least 1600 px
  wide. That gives 120×33 cells at 1920×1080. ASCII and Latin-1 are covered;
  other characters show as `?`.
- **Pixel formats:** RGB and BGR (32 bpp) are supported, and stride is
  respected.
- **Host tests:** render into an in-memory pixel buffer, then check the cell
  grid and a few golden rows of pixels.

### 7.2 Console input and output

- **Output:** goes to the terminal and to COM1 when it's present (§4.4).
- **Input sources:**
  - HID key events are converted to bytes or escape sequences, so arrow keys
    arrive as `ESC [ A` and so on.
  - In QEMU, bytes received on COM1 join the same stream, so tests can type
    quickly over serial.
- **Kernel log:** a 64 KiB ring buffer holding every `[ ok ]` / `[FAIL]` line,
  driver messages and warnings. `dmesg` prints it.

### 7.3 Shell (`crates/shell`)

The shell depends only on these traits: `Vfs` (file operations),
`Console` (reading and writing bytes) and `System` (clock, memory statistics,
reboot and power-off, kernel log).

- **Prompt:** `root@relay:<cwd># `, where `<cwd>` is shortened to `~` under
  `/root`, as in Linux.
- **Line editor:**
  - printable characters are inserted at the cursor; Backspace deletes the
    character before it;
  - ←/→, Home/End, and Ctrl-A/Ctrl-E move the cursor;
  - ↑/↓ step through 50 lines of history held in memory;
  - Ctrl-C cancels the line (prints `^C`);
  - Ctrl-L clears the screen and redraws the current line.
- **Parser:** words are split on whitespace. Supported syntax:
  - `'…'` (literal) and `"…"` (supports `\"` and `\\` inside);
  - `\` escapes outside quotes;
  - `> file` (truncate) and `>> file` (append) on any command's standard
    output;
  - error messages always go to the screen, never into a redirect file.

  Anything else is an error: `|`, `;`, `&`, `$`, `*` and `?` (when not
  quoted) print `relay-sh: unsupported syntax: <token>`.
- **Command results:** every command returns an exit status. Error messages
  follow the GNU coreutils format, for example
  `cat: foo: No such file or directory`. Unknown commands print
  `relay-sh: foo: command not found`.
- **Command dispatch:** a table of command name to function, so each command
  is small and tested on its own.

**Built-in commands:**

| Command | Supported syntax |
|---|---|
| `ls` | `ls [-l] [-a] [path…]`; entries sorted; `-l` shows mode, links, uid/gid as names (`root`), size and mtime |
| `cd` | `cd [dir]`; no argument goes to `/root`; `cd -` isn't supported |
| `pwd` | |
| `cat` | `cat file…` |
| `head` / `tail` | `[-n N] file` (default 10) |
| `wc` | `wc file…` (lines, words, bytes) |
| `stat` | `stat path` (type, size, blocks, inode, links, mode, uid/gid, atime/mtime/ctime) |
| `echo` | `echo [-n] args…` |
| `touch` | `touch file…` (creates the file or updates its mtime) |
| `mkdir` | `mkdir [-p] dir…` |
| `rmdir` | `rmdir dir…` |
| `rm` | `rm [-r] [-f] path…`; refuses `/` |
| `cp` | `cp src dst` and `cp src… dir`; files only (no `-r` in milestone 1) |
| `mv` | `mv src dst` and `mv src… dir` (rename within the one filesystem) |
| `clear` | |
| `help` | lists commands with a one-line description |
| `uname` | `uname [-a]` → `Relay relay 0.1.0 x86_64` |
| `date` | wall clock from the RTC, printed in UTC |
| `df` | size, used and available space of `/` |
| `free` | total, used and free frames and heap |
| `dmesg` | prints the kernel log |
| `sync` | flushes the block cache and the device |
| `reboot` / `poweroff` | see §7.4 |

### 7.4 Reboot and power-off

- **Before either:** flush the cache, mark the ext2 superblock clean, and
  flush the device.
- **`reboot`:** write to the FADT reset register if one is described;
  otherwise write `0x06` to port `0xCF9`; then triple-fault as the last
  resort.
- **`poweroff`:** write `SLP_TYPa | SLP_EN` to PM1a_CNT (and PM1b if present)
  using the decoded `\_S5` values. If it can't, print
  `System halted. It is now safe to power off.` and halt.
- **Test mode** (`cmdline` contains `test=1`): `poweroff` writes the exit code
  to the `isa-debug-exit` port `0xF4` before trying ACPI, so QEMU exits
  straight away.

## 8. Filesystem (`crates/ext2`, `crates/vfs`)

### 8.1 VFS

- **Traits:** `FileSystem` (root inode, statfs, sync) and `Inode` (stat,
  lookup, read_at, write_at, truncate, create, mkdir, unlink, rmdir, rename,
  readdir).
- **Mount table:** one entry for now, ext2 at `/`. Path lookup already
  consults the table, so `/dev`, `/proc` or tmpfs mounts can be added later
  without changing callers.
- **Path resolution:**
  - absolute paths, and relative paths from the shell's current directory;
  - `.` and `..` (`..` at `/` stays at `/`);
  - repeated slashes collapse into one;
  - a trailing slash on a non-directory fails with `ENOTDIR`;
  - each name is at most 255 bytes, each path at most 4096 bytes;
  - names are raw bytes, shown as UTF-8 with invalid bytes replaced by `?`.
- **Symlinks:** `ls -l` shows them as type `l` with `-> target`, but
  resolution doesn't follow them in milestone 1. Opening one returns `EINVAL`.
- **Errors:** an `Errno` enum with these values:
  - `ENOENT`, `EEXIST`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`;
  - `ENOSPC`, `EIO`, `EROFS`, `EINVAL`, `ENAMETOOLONG`, `EXDEV`, `EBUSY`.

  Each has a Linux-style `strerror` message.

### 8.2 ext2 driver

**Mount checks:**
- The magic number must be `0xEF53` and the revision must be 1 (dynamic).
- Block size may be 1, 2 or 4 KiB. Inode size is taken from the superblock.
- Incompat features: `filetype` is supported. Any other incompat feature is
  refused with `EINVAL` and a logged reason.
- Ro_compat features: `sparse_super` and `large_file` are supported. Any other
  ro_compat feature means we mount read-only and log a warning.
- Compat features are ignored. We don't touch `dir_index`/htree data; the
  images we create don't enable it (see §9.1).

**Data layout support:**
- Direct blocks plus single, double and triple indirect blocks.
- Sparse holes read back as zeros.
- File sizes use 64 bits (`i_size_high`).

**Allocation:**
- Block and inode bitmaps, per group.
- New blocks are placed near the file's existing blocks, in the same group if
  possible. New directories go in the group with the most free inodes.
- Every allocation updates the free counts in both the group descriptor and
  the superblock.

**Operations:**
- **Directories:** lookup and readdir. Create, mkdir, unlink and rmdir keep
  `rec_len` valid, merge free space when entries are removed, and add
  directory blocks when needed.
- **Files:** `read_at`, and `write_at` (which can extend the file, allocating
  data and indirect blocks as needed); `truncate` to grow or shrink,
  freeing data and indirect blocks.
- **Rename:** works within one filesystem. If the target exists it is replaced
  (it must be a file, or an empty directory). Moving a directory updates its
  `..` entry and the parents' link counts. Moving a directory into its own
  descendant fails with `EINVAL`.

**Metadata:**
- Link counts are updated for `.`, `..` and hard links. `ln` isn't in
  milestone 1, but existing counts are handled correctly.
- Timestamps (atime, mtime, ctime; dtime set on delete) come from the RTC.
  atime is only set when an inode is created, a lightweight form of
  `noatime`.
- New inodes get mode `0644` (files) or `0755` (directories), uid and gid 0.

**Consistency:**
- On a read-write mount, clear `EXT2_VALID_FS` in `s_state` (marking the
  filesystem not clean), increment `s_mnt_count` and write the superblock.
- A clean unmount (from `reboot` or `poweroff`) sets `EXT2_VALID_FS` again.
- Every superblock write also updates the backup copies in groups that hold
  them (group 0, 1 and powers of 3, 5 and 7).

### 8.3 Block cache and durability

A write-back LRU block cache (8 MiB, keyed by filesystem block number) sits
between ext2 and the `BlockDevice`.

- **Automatic flush:** after each shell command finishes, the shell calls
  `sync()`. That writes every dirty block in ascending block order, then calls
  `BlockDevice::flush()`, which sends `SYNCHRONIZE CACHE`. So once the prompt
  comes back, the command's changes are on the stick.
- **Early flush:** if dirty blocks would take up more than half the cache,
  they are written out before the command finishes.
- **Write errors:** a write error during sync is reported through `dmesg` and
  returned as `EIO`. The block stays dirty so a later `sync` can retry it.

### 8.4 Initial `/` tree

`xtask image` builds this from `rootfs/` plus a list of directories kept in
`xtask` (git doesn't track empty directories):

```
/bin  /dev  /etc  /home  /root  /tmp  /usr  /var
/etc/hostname   relay
/etc/motd       welcome banner shown at shell start
/root/README    short guide to the available commands
```

All inodes are owned by 0:0 (§9.1). `/tmp` has mode `1777`; the other
directories have `0755`.

## 9. Tooling (`xtask`)

### 9.1 Commands

| Command | What it does |
|---|---|
| `cargo xtask build [--release]` | Builds `relay-boot` (`x86_64-unknown-uefi`) and `relay-kernel` (`x86_64-unknown-none`). Output goes to `target/relay/`. |
| `cargo xtask image` | Builds a 256 MiB image at `target/relay/relay-os.img`, without needing root: `sfdisk` writes the GPT (with fixed GUIDs); `mformat` and `mcopy` create and fill the ESP at its offset; `mke2fs -t ext2 -b 4096 -I 256 -O ^dir_index,^resize_inode,^ext_attr -L relayroot -d <staging>` creates and fills ext2; `debugfs -w` sets uid/gid 0 on every populated inode. The result is written into the image at the partition offset. |
| `cargo xtask qemu [--serial-only]` | Interactive run: q35 machine, OVMF, `qemu-xhci`, `usb-kbd` and `usb-storage` backed by a copy of the image, 1 GiB RAM, a GTK window for the framebuffer, and serial on stdio. |
| `cargo xtask host-shell <img>` | Runs the `shell` crate on the host over the ext2 partition of an image file, through a file-backed `BlockDevice`. Used to develop the filesystem and shell before the hardware drivers exist. |
| `cargo xtask test` | `cargo test` for all host-testable crates, then `build` and `image`, then every QEMU scenario (§9.3). Exits non-zero on the first failure and prints the serial log and screenshot path. |
| `cargo xtask flash --kernel` | Replaces `BOOTX64.EFI`, `kernel.elf` and `cmdline` on the stick's ESP using `mcopy -o`, addressing the ESP by its byte offset on the whole-disk device (§15). The stick's `cmdline` is empty by default; `test=1` goes only into QEMU test images. The ext2 root isn't touched, so files created on the NUC are kept. |
| `cargo xtask flash --full` | After a typed confirmation: writes a new GPT, formats the ESP, runs `mke2fs` over the rest of the stick (same options as `image`), fills it, sets ownership, then writes the ESP files. |
| `cargo xtask verify-usb` | Runs `e2fsck -fn` on the stick's ext2 partition and prints its file tree using `debugfs`. |
| `cargo xtask setup-udev` | Writes a udev rule that gives the invoking user read/write access to the disk and partitions **with this stick's serial only**, and prints the three `sudo` commands that install it (§15). |

### 9.2 Flash safety

- **Identifying the stick:** it is resolved only through
  `/dev/disk/by-id/usb-Kingston_DataTraveler_3.0_08606E6D413FB27127135F8E-0:0`
  (and its `-partN` links). This path is set in `xtask/src/config.rs`.
- **Checks before any write:**
  - the resolved device's `removable` attribute is `1`;
  - its transport is `usb`;
  - its size is under 64 GiB;
  - it isn't mounted at `/`, `/boot` or `/boot/efi`;
  - its parent disk isn't `nvme*`.
- **Unmounting:** any auto-mounted partitions of the stick are unmounted with
  `udisksctl unmount` before writing.
- **No hidden privileges:** the commands never call `sudo` by themselves.
  Without the udev rule they fail with a message telling the user to run
  `setup-udev`.

### 9.3 QEMU end-to-end tests

**Machine setup:**
```
qemu-system-x86_64 -machine q35 -accel kvm:tcg -m 1G -cpu max -smp 1 -no-reboot
  -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE_4M.fd
  -drive if=pflash,format=raw,file=<per-run copy of OVMF_VARS_4M.fd>
  -device qemu-xhci,id=xhci
  -device usb-kbd,bus=xhci.0
  -drive if=none,id=stick,format=raw,file=<per-run copy of relay-os.img>
  -device usb-storage,bus=xhci.0,drive=stick
  -device isa-debug-exit,iobase=0xf4,iosize=0x04
  -serial <pipe> -qmp unix:<sock>,server,nowait -display none
```

The ESP `cmdline` in the test image contains `test=1`.

**How scenarios run:**
- Each scenario in `tests/e2e/*.txt` is a list of steps:
  - `send <text>`: typed over serial;
  - `key <text>`: typed with QMP `send-key`, which goes through xHCI → HID;
  - `expect <regex>`: waits for matching serial output, with a timeout;
  - `screenshot`: saves a QMP `screendump` and checks it isn't a single
    colour;
  - `reboot`: sends `reboot`, waits for QEMU to exit (`-no-reboot`), then
    starts it again on the same disk;
  - `poweroff`.
- After every scenario, `e2fsck -fn` must pass on the ext2 partition extracted
  from the disk image.

**Scenarios:**

| # | Name | Checks |
|---|---|---|
| 1 | `boot` | The prompt appears within 20 s. Every startup line is `[ ok ]`. The motd is shown. |
| 2 | `keyboard` | `echo hello` typed with `key` produces `hello`. Shift, Caps Lock and Backspace work. Arrow-key history recall works. |
| 3 | `fileops` | `mkdir -p`, `echo >`/`>>`, `cat`, `ls -l`, `cp`, `mv`, `rm -r`, `rmdir`, `touch`, `stat`, `head`/`tail`/`wc`. Error messages for missing paths, non-empty directories and `rm /`. |
| 4 | `persist` | Write a file, `reboot`, `cat` it after the second boot. `ls` of the directory matches. |
| 5 | `bigfile` | Build an 8 MiB file by repeated `>>` of a 4 KiB line file, then `cp` it (exercises double-indirect blocks). The host checks the file's size and content with `debugfs dump`. |
| 6 | `display` | A screenshot shows non-uniform content in the rows where the prompt is expected. |
| 7 | `diskfull` | Fill a small test image (32 MiB ext2) until `ENOSPC`. The shell reports `No space left on device` and `e2fsck` is still clean. |

**Known gaps in QEMU** (covered by the hardware checklist instead):
- 64-byte contexts, scratchpad buffers and BIOS handoff. QEMU's xHCI needs
  none of them, so those code paths only get host unit tests.
- SuperSpeed port reset. QEMU's `usb-storage` connects at high speed.
- Real GOP modes on HDMI.
- A slow-to-power-up flash device.

### 9.4 Hardware checklist (`docs/hardware-test.md`)

1. In Mint: `cargo xtask flash --full` (first time) or `flash --kernel`.
2. Reboot the NUC, press F10, and choose the UEFI entry for the Kingston stick.
3. Check the screen: native resolution, every startup line `[ ok ]`, the xHCI
   log line shows 64-byte contexts and the scratchpad count.
4. Run the scripted command list in the document (the same operations as the
   `fileops` scenario), typed on the K120.
5. `reboot`, choose the stick again with F10, and check the files are still
   there.
6. `poweroff`. The machine should turn off, or show the safe-to-power-off
   message.
7. Boot into Mint and run `cargo xtask verify-usb`. It must be clean and list
   the files from step 4.

### 9.5 CI gate

Every pull request to `main` must pass `.github/workflows/ci.yml` on
GitHub-hosted `ubuntu-24.04` runners (the same Ubuntu release the NUC's Linux
Mint is based on, so QEMU, OVMF and e2fsprogs match):

| Job | Runs | Checks |
|---|---|---|
| `lint` | `cargo xtask lint` | `cargo fmt --check`; clippy with warnings as errors on the host crates, the loader and kernel libraries, and both bare-metal targets |
| `unit` | `cargo xtask unit` | Host unit tests of every crate that can build for the host |
| `e2e` | `cargo xtask test --e2e-only` | Every scenario in `tests/e2e/` in QEMU, with KVM if the runner provides it and TCG otherwise; serial logs and screenshots are uploaded when it fails |

- The workflow only installs tools and calls `xtask`. The list of checks
  lives in `xtask/src/ci.rs`, so `cargo xtask ci` locally checks exactly
  what CI checks, and new crates and scenarios are covered without editing
  the YAML. A unit test keeps the workflow's toolchain version equal to
  `rust-toolchain.toml`.
- Hardware checks cannot run in CI. The pull-request template asks whether
  a change needs a NUC check and where its result is recorded.
- Work lands in small pull requests, each green on its own; plan 1 is split
  into seven.

## 10. Error handling

- **Device errors are values, not panics.** Drivers, the block cache, ext2 and
  the VFS return `Result`. USB errors become `IoError`, which the VFS turns
  into `EIO`. The shell prints them in Linux style.
- **A panic means a kernel bug.** The panic handler and exception handlers:
  1. switch to a dedicated emergency stack if needed;
  2. draw a red panic screen showing the message, source location, exception
     vector, error code, the general registers plus CR2/CR3, and the last 20
     lines of `dmesg`;
  3. halt the CPU.
  They don't allocate from the heap.
- **Boot keeps going when a device fails:**

  | Failure | Behaviour |
  |---|---|
  | No xHCI found | `[FAIL] usb`. The screen stays up with a message; input is available from serial only. |
  | No keyboard found | `[FAIL] keyboard`. The boot log stays on screen. |
  | No root disk, or the ext2 mount fails | `[FAIL] mount /`. The shell starts with an empty read-only `/`, and filesystem commands report `EIO` or `EROFS`. |

- **Nothing waits forever.** Every hardware wait has a timeout (§6.2, §6.4).
  The xHCI `poll()` never blocks.

## 11. Build order and milestones inside milestone 1

Each step ends with something that can be tested.

| Step | Contents | Check |
|---|---|---|
| 1 | Workspace, toolchain, `xtask build/image/qemu`; loader prints on the UEFI console. | Manual run on OVMF. |
| 2 | Loader → kernel hand-off, framebuffer console, GDT/IDT, panic screen. | QEMU `boot` scenario (without a prompt yet). **NUC check 1:** text on HDMI at native resolution. |
| 3 | Frame allocator, heap, kernel page tables, `map_mmio`, ACPI, timer, RTC, PCI. | Unit tests; startup lines `[ ok ]` in QEMU. |
| 4 | `vfs`, `ext2` (with block cache), `shell`, `term` host crates; `xtask host-shell`. Can run in parallel with step 3. | Host tests including randomized model checking plus `e2fsck`. |
| 5 | `usb`: xHCI core plus HID keyboard; interactive shell without disk. | QEMU `keyboard` scenario. **NUC check 2:** typing on the K120. |
| 6 | `usb`: BOT/SCSI, GPT, mount `/`, reboot/poweroff. | QEMU scenarios 1–7. **NUC check 3:** full hardware checklist. |
| 7 | Hardening: fixes found on the NUC, docs, `README.md` with quick-start. | `cargo xtask test` green; checklist green. |

## 12. Testing strategy summary

- **Host unit tests** cover:
  - TRB rings, context layout (both strides), BIOS handoff and reset
    sequences against a fake register file;
  - descriptor parsing, HID report diffing and key repeat;
  - CBW/CSW encoding and sense decoding;
  - GPT parsing including CRC checks and the backup-header fallback;
  - path resolution and the parser;
  - each shell command against an in-memory `Vfs`;
  - terminal escape parsing and rendering.
- **Filesystem tests against real tools:**
  - `mke2fs` creates a scratch image (1 KiB and 4 KiB block sizes);
  - our crate changes it;
  - `e2fsck -fn` must report it clean;
  - `debugfs` must see the same tree and contents.
- **Randomized model test:** thousands of seeded operations (create, write at
  random offsets, truncate, mkdir, rename, unlink, rmdir, remount) run both on
  ext2 and on an in-memory model. After each batch the two must match and
  `e2fsck` must stay clean. A failing seed is printed so it can be replayed.
- **QEMU end-to-end scenarios** (§9.3) run on the full system image.
- **Hardware checklist** (§9.4) is run by hand at NUC checks 1–3.

## 13. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Intel xHCI behaves differently from QEMU (handoff, scratchpad, CSZ, SuperSpeed) | These paths are explicit in the design and tested on the host. The NUC checks come early (steps 2 and 5). Detailed `dmesg` logging covers every xHCI step, because the screen is the only debug output on the NUC. |
| The firmware picks a GOP mode the monitor shows badly | The mode-choice rule is in §4.2. The chosen mode is logged. The `cmdline` can force a mode with `video=WxH`. |
| Unexpected storage behaviour on the Kingston stick (slow to power up, strict transfer lengths) | `TEST UNIT READY` retries, 64 KiB transfer chunks, and BOT reset recovery. |
| Power cut leaves ext2 inconsistent (no journal) | The cache is flushed after every command, the superblock clean flag is used, and `verify-usb` runs `e2fsck`. |
| Writing to the wrong disk | Serial-pinned by-id path, multiple checks, no automatic `sudo` (§9.2). |
| Debugging on the NUC without serial | Startup step lines, `dmesg`, and a panic screen that includes the log tail. |

## 14. Out of scope for milestone 1

- User-mode processes, ELF program loading, system calls, multitasking, SMP.
- Interrupt-driven I/O (MSI/MSI-X). Milestone 1 polls.
- USB hubs, mice, other USB classes, USB 3 streams/UAS.
- A text editor, pipes, environment variables, globbing, scripts.
- Symlink following, hard-link creation, permission enforcement, multiple
  users.
- ext3/ext4 features (journal, extents, htree).
- Networking, audio, GPU acceleration, NVMe/AHCI drivers.
- Keyboard layouts other than US.

## 15. Revisions made during planning

1. **Flashing does not use partition device nodes.** Only root can make
   Linux re-read a partition table, so after `flash --full` repartitions the
   stick, `/dev/sdXN` can be stale until the stick is replugged. All flash
   and verify commands address both filesystems by byte offset on the
   whole-disk device (mtools `-i dev@@offset`, `mke2fs -E offset=`,
   e2fsprogs `dev?offset=`).
2. **`setup-udev` prints the install commands instead of running them**,
   consistent with §9.2 (xtask never calls `sudo`).
3. **The loader and the kernel each have a library target** holding their
   pure logic, unit-tested on the host; the binaries only build for bare
   metal.
4. **Two additions to the e2e runner:** the scenario step `esp-write`
   (overwrite an ESP file before boot, used to test a corrupt `kernel.elf`),
   and the environment variable `RELAY_QEMU_ACCEL` (for example `tcg` on
   machines without KVM).
5. **The loader's linear map covers RAM-type regions only** (usable,
   loader, kernel, ACPI). OVMF reports a Reserved window near 1 TiB; mapping
   up to it is slow and pointless. The kernel maps anything else on demand
   (§5.3).
6. **CI gate added** (§9.5). It was not part of the original design; the
   user asked for it during planning.
7. **Findings from NUC check 1** (real firmware behaved differently from
   QEMU's OVMF):
   - The loader opens firmware protocols only with `GET_PROTOCOL` and never
     closes them, as the Linux EFI stub does. Exclusive opens made the NUC's
     firmware stop its own text console.
   - Kernel pages use the standard `LOADER_DATA` memory type. With an
     OS-defined type the NUC's `ExitBootServices` never returned. The kernel
     image is therefore reported as `Bootloader` memory, and
     `BootInfo::kernel_phys_*` gives its range; `MemoryKind::Kernel` is not
     produced.
   - The loader and early kernel paint **boot-progress squares** on the
     framebuffer (legend in `docs/hardware-test.md`), because the NUC has no
     serial port and the firmware console is unusable once the loader holds
     the display.
8. **Decisions made while planning plan 2** (kernel core):
   - **The kernel's linear map covers RAM only.** The kernel's own page
     tables map RAM-type regions (usable, loader, ACPI) write-back and the
     framebuffer write-combining. Everything else goes through `map_mmio`
     (§5.3): device registers uncached, and ACPI tables that lie in reserved
     memory write-back. Every physical page has at most one virtual address,
     so two cache types can never alias.
   - **The GDT and IDT load before the console** (§4.4 steps 1–2 swap
     internally; the status lines keep their order). A fault before the
     console starts still reaches the panic screen, which starts the console
     itself.
   - **Frames below 1 MiB are never allocated** (§5.1).
   - **TSC frequency** (§4.4 step 5): CPUID leaf 0x15 when it reports the
     crystal frequency. When leaf 0x15 has the ratio but no crystal, leaf
     0x16's base frequency, as in Linux. Otherwise a measurement against the
     HPET. The NUC reports 2496 MHz through leaf 0x15 (leaf 0x16 says 2500).
     QEMU with KVM and `-cpu max` has no leaf 0x15, so QEMU measures. The
     LAPIC timer is calibrated against the TSC.
   - **The NUC has two xHCI controllers** (§2, §5.5). `00:0d.0`
     (`8086:461e`, Thunderbolt 4, BAR `0x603D190000`) enumerates before
     `00:14.0` (`8086:51ed`, BAR `0x603D180000`), which is the one with the
     keyboard and the stick. PCI reports every xHCI controller. Plan 4
     chooses the controller and enables memory decoding and bus mastering
     on it.
   - **The PCI device list goes to the kernel log** and serial. The screen
     shows only the xHCI lines and a summary, because the NUC's terminal has
     33 rows.
   - **Every interrupt vector from 32 up has a gate.** An interrupt the
     firmware left pending on an unused vector is counted and ended with an
     EOI. One that fires 1000 times panics as an interrupt storm, naming the
     vector. The LAPIC's other interrupt sources are masked, and interrupts
     left in service are ended before interrupts are enabled.
   - **New cmdline words:** `panic=early` (test), `tsc=hpet` (measure the
     TSC against the HPET even when CPUID knows it) and `check=timer` (count
     timer ticks over three RTC seconds at boot).
9. **Decisions made while planning plan 3** (filesystem and shell):
   - **One filesystem trait keyed by inode number.** §8.1's `FileSystem`
     and `Inode` traits are one `vfs::FileSystem` trait whose operations
     name inodes by number; an inode object would have to borrow the
     filesystem, its block cache and its device at once. Its documentation
     is a contract (which error wins when several apply, modes and times of
     new inodes, a full disk as a short write, the rename rules), and it
     adds `read_link`, `touch` and `shutdown`. `vfs::MemFs` keeps the same
     contract: it is the shell's test filesystem, the model the ext2 driver
     is checked against, and the empty read-only `/` of §10. The shell works
     through a path-level `Vfs` trait that the mount table implements; the
     mount table can already mount further filesystems.
   - **Path details follow Linux.** A symbolic link used as a directory is
     `ENOTDIR`; an inode number no longer in use is `ENOENT`; `rmdir .` is
     `EINVAL`, `rmdir ..` `ENOTEMPTY`, `rmdir /` `EBUSY`; a removed current
     directory makes relative paths `ENOENT` until the next `cd`.
   - **Errors.** `Errno` gains `EFBIG` ("File too large") for writes past
     the largest file ext2 can map (Linux's `ext2_max_size`). There is no
     `EMLINK`, so a directory already holding 32,000 links is `ENOSPC`.
   - **Shell.** An unquoted `~` alone or before `/` at the start of a word
     means `/root`. Besides §7.3's list, an unquoted `<`, `` ` ``, `(`, `)`
     and `2>` are unsupported syntax, and a command has at most one
     redirection. The line editor takes printable ASCII. `head` and `tail`
     take one file and plain-digit counts (also as `-N`). `cat f >> f` is
     refused, and commands that read to the end of a file stop at the size
     it had when they started. There is no `--help`, so GNU's "Try" line is
     left out. Exit statuses follow bash and GNU (127, 2, 130; `ls` 2).
   - **`reboot` and `poweroff` keep the machine up if the clean shutdown
     fails** (§7.4); `-f` goes ahead anyway. `System::reboot`/`poweroff`
     return only where they cannot act (on the host, in tests), and the
     shell then stops.
   - **ext2 treats every inconsistency as corruption, never as a panic.**
     Beyond §8.2's checks, mount refuses group metadata that leaves its
     group or overlaps other metadata, a first data block that does not
     match the block size, fewer than 8 blocks per group and more than
     65,536 groups. Later, a block pointer into metadata, an impossible file
     size or entry name, and a directory whose size does not match its
     blocks are `EIO` with an `ext2:` log line.
   - **The clean flag follows Linux** (refines §8.2): `shutdown` writes back
     the state the filesystem had when it was mounted, so one that was not
     clean then stays marked not clean until `e2fsck` has checked it. If the
     final writes fail, a later `sync` or `shutdown` keeps trying.
   - **ext2 writes what Linux and e2fsprogs write.** Root may use the
     reserved blocks (`df` leaves them out of "available"). Writing past
     2 GiB sets `large_file`; releasing an inode drops its extended
     attribute block's reference count; a change to an htree directory
     clears its index flag (htree directories are read linearly); new
     256-byte inodes get `i_extra_isize` 32 and a creation time; times are
     32-bit seconds; `sparse_super2` backup locations are honoured.
   - **e2fsprogs is already on the CI runner** (ubuntu-24.04 ships 1.47.0
     and fdisk), so the `unit` job runs the real-tool tests without an
     install step. Those tests fail, never skip, when the tools are missing.
   - **`xtask host-shell`** accepts only image files, puts the terminal in
     raw mode with `stty` (restored at the end and after a panic), shows
     ext2's mount warnings, and ends on `reboot`, `poweroff` or the end of
     input, always with a clean shutdown. `free` has no figures on the host.
10. **Decisions made while planning plan 4** (USB keyboard):
   - **The `Hal` also carries register access and the log** (refines
     §6.1). `map_mmio` returns the registers' virtual address, and
     `read32`/`write32` read and write them, so host tests run the driver
     against a fake controller that sees every access with its real
     semantics (write-one-to-clear bits, doorbells). `map_mmio` and
     `alloc_dma` return `None` on failure instead of panicking; `log` adds a
     line to `dmesg`. `DmaBuf` is reached only through bounds-checked
     volatile accessors, and `alloc_dma` returns page-aligned memory.
   - **Class drivers see a `Bus`, not the xHCI driver:** control requests,
     IN transfers that complete later, and halt recovery. `poll()` never
     waits; what needs a control transfer (the Caps Lock LED, recovering a
     failed endpoint) runs in a separate `service` step from the console's
     idle loop, where blocking is allowed.
   - **Every xHCI controller is started** (§5.5), not just the first
     (the NUC's Thunderbolt xHCI enumerates before the PCH's). Each PCI
     function is put into power state D0 first (firmware may leave an
     unused controller in D3hot), with its BARs written back if leaving
     D3hot reset it, then memory decoding and bus mastering are enabled. `[ ok ] usb` needs one working controller; one that fails
     gets a line of its own and is skipped. Controllers without 64-bit DMA
     or with a page size other than 4 KiB are refused.
   - **Hot-plug on root ports.** Devices are set up when they appear, at
     boot and later (after a 100 ms debounce), and dropped when they are
     unplugged, also when unplugged and replugged between two looks; this
     runs from the console's idle loop.
   - **Waits and resets the spec leaves open.** After start the driver
     waits until 100 ms after the ports were reset or powered (USB 2.0's
     TSIGATT) and up to 1 s while a USB 3 link is still training, so
     devices present at boot are on the boot screen. A port reset waits
     500 ms. A USB 3 port that trains gets a hot reset before enumeration,
     as in Linux; one that does not train gets one warm reset instead.
     Devices get 10 ms after `SET_ADDRESS`. A command that times out
     aborts the command ring (all 64 bits of CRCR written), the driver
     waits for Command Ring Stopped, and the TRB becomes a No Op; a
     controller whose abort fails, or that reports a host system error, is
     halted and not used again. USB timeouts are measured with the TSC, so
     they expire even if the LAPIC timer did not start; without a TSC
     frequency USB is not started.
   - **Memory the controller may still use is never freed:** a slot's
     memory is freed only after Disable Slot succeeded (otherwise it is
     kept, and logged), and nothing of a dead controller is freed.
   - **Keyboard details.** `SET_PROTOCOL(boot)` must succeed (otherwise
     the device's boot line says `keyboard not started: <reason>`); a
     stalled `SET_IDLE` is only logged. A report with ErrorRollOver, POSTFail or
     ErrorUndefined in any key slot changes nothing, and a usage listed
     twice is one key. Caps Lock and its LED are per keyboard, and the LED
     is sent once per change. Keypad keys always give digits (Num Lock is
     not tracked); lock keys do not repeat; the newest key repeats, and
     after a long pause it repeats once, not in a burst. When a keyboard's
     endpoint fails, every held key is released, and recovery is tried
     once a second, three times.
   - **Console input** (§7.2): keyboard and COM1 bytes join one queue with
     4 KiB of type-ahead. A Ctrl-C typed while a command runs drops what
     was typed before it, as a terminal does. Keys send what a Linux
     terminal sends: Ctrl with a letter its control code, Enter CR,
     Backspace DEL, Insert/Delete/Page Up/Page Down `ESC [ 2~/3~/5~/6~`.
     Between polls the console sleeps until the next timer tick.
   - **The shell's output goes to the screen and serial, not into the
     kernel log**, so `dmesg` shows what the kernel reported.
   - **Until plan 5** the boot shows `[FAIL] mount /: no storage driver
     yet`, the shell runs on the empty read-only `/` of §10, and `reboot`
     and `poweroff` print the safe-to-power-off message and halt. The
     `relay: early boot complete` line is gone: the prompt is the last
     line.
   - **New cmdline word `debug=usb`** also shows the USB log on screen, for
     a NUC whose keyboard does not work (no serial port, no `dmesg`).
   - **The e2e `key <text>` step** types over QMP `send-key` and then
     presses Enter, like `send`; `{up}`, `{backspace}`, `{caps_lock}`,
     `{ctrl-c}` and similar name keys without a character. Each press is
     held 30 ms, and the step returns once QEMU has played them all.
   - **What QEMU 8.2 really does** (corrects §9.3's known gaps):
     `qemu-xhci` numbers its USB 3 ports 1-4 and its USB 2 ports 5-8, and
     `usb-storage` connects at SuperSpeed, so a SuperSpeed port is
     exercised in QEMU too; `usb-kbd` is high-speed. 64-byte contexts,
     scratchpad buffers, the BIOS handoff, port power control, low- and
     full-speed devices and a USB 3 port that needs a warm reset remain
     covered only by host tests against a fake controller and by the NUC.
