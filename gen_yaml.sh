#!/bin/sh

if test $# -ne 3
then
  echo >&2 "Usage: $0 flash-base64 flash-obj output.yaml"
  exit 2
fi

algo=$1
obj=$2
out=$3

cat >"$out" <<!
- name: nop_ipc
  description: do nothing, wait for selfdebug to pick things up
  default: true
  stack_overflow_check: false  # TODO: enable this and figure out why it's sometimes triggering
  instructions: $(cat "$algo")
!

objdump -d "$obj" |
  grep -Ei '^[0-9a-f].*<(init|uninit|program_page|erase_sector)>:' |
  sed 's;<\(.*\)>:;\1;' |
  while read addr name
  do
    thumb_addr=$(perl -e "printf '%#x', 0x$addr + 1")
    echo "  pc_$name: $thumb_addr" >>"$out"
  done

cat >>"$out" <<!
  data_section_offset: 0x0
  flash_properties:
    address_range:
      start: 0x10000000 # code loaded here (since there's no load_address)
      end: 0x18000000
    page_size: 0x1000
    erased_byte_value: 0xff
    program_page_timeout: 11000
    erase_sector_timeout: 13000
    sectors:
    - size: 0x1000
      address: 0x0
  cores:
  - core0
!
