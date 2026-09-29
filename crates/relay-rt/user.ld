/* Relay OS user programs (spec §5.1, §5.2): static, linked at 0x400000,
 * one PT_LOAD per permission set, and the ABI note in a PT_NOTE. */
ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);   /* R-X */
    rodata PT_LOAD FLAGS(4);   /* R-- */
    data   PT_LOAD FLAGS(6);   /* RW- */
    note   PT_NOTE FLAGS(4);
}

SECTIONS
{
    . = 0x400000;

    .text : ALIGN(4K) { *(.text .text.*) } :text

    . = ALIGN(4K);
    .rodata : ALIGN(4K) { *(.rodata .rodata.*) } :rodata
    .note.relay : ALIGN(4) { KEEP(*(.note.relay)) } :rodata :note

    . = ALIGN(4K);
    .data : ALIGN(4K) { *(.data .data.*) *(.got .got.*) } :data
    .bss : ALIGN(16) { *(.bss .bss.*) *(COMMON) } :data

    /DISCARD/ : { *(.comment) *(.note.gnu.* .note.GNU-stack) *(.eh_frame .eh_frame_hdr) }
}
