# ACPI test fixtures

Raw ACPI tables used by the `acpi` unit tests. Each file is one table,
byte for byte, except `DSDT.bin` (see below).

| Directory | Source |
|---|---|
| `qemu/` | QEMU 8.2.2 `-machine q35` with OVMF, as `cargo xtask qemu` starts it, dumped by a debug build of the kernel. Physical addresses (used by the tests): RSDP `0x3fb7e014`, XSDT `0x3fb7d0e8`, FACP `0x3fb79000`, DSDT `0x3fb7a000`, APIC `0x3fb78000`, HPET `0x3fb77000`, MCFG `0x3fb76000`, WAET `0x3fb75000`, BGRT `0x3fb74000`. |
| `nuc/` | Intel NUC 12 Pro, firmware `WSADL357.0090.2023.0821.1714`, from `/sys/firmware/acpi/tables/` in Linux. Physical addresses: FACP `0x56d45000`, DSDT `0x56cd2000`, HPET `0x56d46000`, MCFG `0x56cb4000`. Linux does not expose the RSDP or XSDT; the tests build those. |

`DSDT.bin` in both directories is the table's 36-byte header followed by
the 128 bytes of AML around its only `_S5_` name (64 before, 64 after),
with the length and checksum fields fixed up. The scanner only needs that
window, and the NUC's full DSDT is 469 KB of firmware code.
