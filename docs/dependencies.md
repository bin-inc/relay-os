# Dependency Decisions

Runtime dependencies are pinned with exact workspace versions. Target-side
dependencies disable default features; no crate features are enabled until a
task consumes a documented capability. Evidence was reviewed on 2026-09-07.
The host-only `ovmf-prebuilt` graph requires the reviewed ISC, BSD-3-Clause,
and CDLA-Permissive-2.0 transitive licenses. `deny.toml` records narrow
duplicate-version exceptions required by the reviewed direct versions.

Task 1 does not include an xHCI crate. Task 12 will implement the narrowly
scoped local xHCI definitions needed by Relay OS unless a newly audited safe
replacement is approved.

| Crate | Version | Enabled features | Scope | License | Repository | Maintenance evidence | RustSec result | Unsafe surface | Decision |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `uefi` | 0.40.0 | none | UEFI loader | MIT OR Apache-2.0 | https://github.com/rust-osdev/uefi-rs | 2026-09-07: active upstream release and repository | no relevant advisory | UEFI ABI, firmware tables, and protocol pointers | Accept for the loader boundary. |
| `elf` | 0.8.0 | none | UEFI loader | MIT/Apache-2.0 | https://github.com/cole14/rust-elf | 2026-09-07: published release and maintained repository reviewed | no relevant advisory | ELF byte parsing must remain bounds-checked by the loader | Accept for ELF64 parsing. |
| `x86_64` | 0.15.5 | none | kernel and core | MIT/Apache-2.0 | https://github.com/rust-osdev/x86_64 | 2026-09-07: active rust-osdev release and repository | no relevant advisory | privileged instructions, page tables, and registers | Accept behind narrow architecture wrappers. |
| `uart_16550` | 0.8.0 | none | kernel | MIT OR Apache-2.0 | https://github.com/rust-osdev/uart_16550 | 2026-09-07: maintained rust-osdev repository reviewed | no relevant advisory | port-mapped I/O | Accept for serial diagnostics behind the console boundary. |
| `pci_types` | 0.10.1 | none | core | MIT/Apache-2.0 | https://github.com/rust-osdev/pci_types | 2026-09-07: published release and maintained repository reviewed | no relevant advisory | PCI configuration-space values only | Accept for typed PCI definitions. |
| `crc` | 3.4.0 | none | core | MIT OR Apache-2.0 | https://github.com/mrhooray/crc-rs | 2026-09-07: current release and repository reviewed | no relevant advisory | none beyond pure checksum computation | Accept for checksum validation. |
| `noto-sans-mono-bitmap` | 0.3.2 | `regular`, `size_16`, `unicode-basic-latin` | core | MIT | https://github.com/phip1611/noto-sans-mono-bitmap-rs | 2026-09-07: published font crate and repository reviewed | no relevant advisory | embedded bitmap data only | Accept for framebuffer glyphs. |
| `gpt` | 4.1.0 | default | host xtask only | MIT | https://github.com/Quyzi/gpt | 2026-09-07: published release and maintained repository reviewed | no relevant advisory | host image partition parsing and writes | Accept for host-only GPT image construction. |
| `fatfs` | 0.3.6 | default | host xtask only | MIT | https://github.com/rafalh/rust-fatfs | 2026-09-07: published release and maintained repository reviewed | no relevant advisory | host filesystem image writes | Accept for host-only ESP population. |
| `ovmf-prebuilt` | 0.2.9 | default | host xtask only | MIT OR Apache-2.0 | https://github.com/rust-osdev/ovmf-prebuilt | 2026-09-07: published release and maintained repository reviewed | no relevant advisory | host firmware artifact selection and network download | Accept for host-only QEMU firmware discovery. |
