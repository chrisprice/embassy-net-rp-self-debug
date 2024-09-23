MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100
    RAM   : ORIGIN = 0x20000000, LENGTH = 256K /* RAM end: 0x20040000 */
}

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* ### Boot loader */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;

SECTIONS {
    /* ensure probe_rs_scratch section is at a fixed address */
    .probe_rs_scratch 0x2000e000 (NOLOAD) : {
        KEEP(*(.probe_rs_scratch));
        . = ALIGN(4);
        __escratch = .;
    } > RAM
} INSERT BEFORE .uninit;
