MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  BOOT2                             : ORIGIN = 0x10000000, LENGTH = 0x100
  BOOTLOADER_STATE                  : ORIGIN = 0x10006000, LENGTH = 4K
  FLASH                             : ORIGIN = 0x10007000, LENGTH = 996K
  DFU                               : ORIGIN = 0x10107000, LENGTH = 1000K

  /* The first 1k of RAM is reserved for the OTA flash algorithm itself */
  /* (fixed-address trampolines into FLASH and space for its stack)     */
  RAM: ORIGIN = 0x20000400, LENGTH = 256K - 1k
  /* The unstriped areas are used for data by the OTA flash algorithm   */
  SCRATCH_A: ORIGIN = 0x20040000, LENGTH = 4K
  SCRATCH_B: ORIGIN = 0x20041000, LENGTH = 4K
}

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - ORIGIN(BOOT2);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(BOOT2);

__bootloader_dfu_start = ORIGIN(DFU) - ORIGIN(BOOT2);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(BOOT2);
