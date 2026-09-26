/* Relay OS kernel: higher-half, one PT_LOAD per permission set. */
ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);   /* R-X */
    rodata PT_LOAD FLAGS(4);   /* R-- */
    data   PT_LOAD FLAGS(6);   /* RW- */
}

SECTIONS
{
    . = 0xFFFFFFFF80000000;

    .text : ALIGN(4K) { *(.text .text.*) } :text

    . = ALIGN(4K);
    .rodata : ALIGN(4K) { *(.rodata .rodata.*) } :rodata

    . = ALIGN(4K);
    .data : ALIGN(4K) { *(.data .data.*) *(.got .got.*) } :data
    .bss : ALIGN(16) { *(.bss .bss.*) *(COMMON) } :data

    /DISCARD/ : { *(.comment) *(.note .note.*) *(.eh_frame .eh_frame_hdr) }
}
