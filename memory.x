MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    /* FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100 */
    RAM   : ORIGIN = 0x20000000, LENGTH = 263K /* RAM end: 0x2003FC00 */
    /* FLASH_ALGO: ORIGIN = 0x2003FC00, LENGTH = 1K */

    /* copied from embassy/examples/boot/bootloader/rp/memory.x */
    /*BOOTLOADER       : ORIGIN = 0x10000100, LENGTH = 24K /_* bootloader lives here, can drop to 8k *_/*/
    BOOTLOADER_STATE : ORIGIN = 0x10006000, LENGTH = 4K
    FLASH /*ACTIVE*/ : ORIGIN = 0x10007000, LENGTH = 512K
    DFU              : ORIGIN = 0x10087000, LENGTH = 516K
}

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - ORIGIN(BOOT2);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(BOOT2);

__bootloader_active_start = ORIGIN(FLASH) - ORIGIN(BOOT2);
__bootloader_active_end = ORIGIN(FLASH) + LENGTH(FLASH) - ORIGIN(BOOT2);

__bootloader_dfu_start = ORIGIN(DFU) - ORIGIN(BOOT2);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(BOOT2);
