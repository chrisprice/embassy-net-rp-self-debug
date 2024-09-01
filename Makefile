SYSROOT = $(shell rustc --print sysroot)
SYSROOT_BIN = ${SYSROOT}/lib/rustlib/x86_64-apple-darwin/bin

OBJCOPY = ${SYSROOT_BIN}/llvm-objcopy
LD = ${SYSROOT_BIN}/gcc-ld/ld.lld

OPT = -C opt-level=3
RUSTC_FLAGS = --crate-type=lib -C codegen-units=1 ${OPT}

.PHONY: all

all: algo.yaml flash.s

algo.yaml: flash.o flash.base64
	sh gen_yaml.sh flash.base64 flash.o $@

flash-objdump: flash.o
	objdump -dr $<

flash.o: src/flash_standalone.rs
	rustc --target=thumbv6m-none-eabi ${RUSTC_FLAGS} --emit=obj $< -o $@

flash.s: src/flash_standalone.rs
	rustc --target=thumbv6m-none-eabi ${RUSTC_FLAGS} --emit=asm $< -o $@
	rustfilt <"$@" | sponge "$@"

disassemble: flash.o
	objdump -dr $<

extract-pcs: flash.o
	objdump -d $< | awk '/^[0-9a-f]+ <[^$$].*>:/ { print $$2 " " $$1 " (+1)" }'

extract-isn-bytes: flash.text
	xxd $<

flash.linked: flash.o
	@# needs all code in .text:
	@# not needed - branches are relative anyway
	${LD} -r -Ttext 0x20000000 -o $@ $<

flash.text: flash.o #linked
	@# needs all code in .text:
	${OBJCOPY} -j .text -O binary $< $@

flash.base64: flash.text
	base64 < $< >$@.tmp
	mv $@.tmp $@
