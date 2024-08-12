OBJCOPY = $(shell echo $$(rustc --print sysroot)/lib/rustlib/*/bin/llvm-objcopy)

flash.o: src/flash_standalone.rs
	rustc --target=thumbv6m-none-eabi -C opt-level=3 --crate-type=lib --emit=obj $< -o $@

disassemble: flash.o
	objdump -d $<

extract-pcs: flash.o
	objdump -d $< | awk '/^[0-9a-f]+ <[^$$].*>:/ { print $$2 " " $$1 " (+1)" }'

extract-isn-bytes: flash.text
	xxd $<

flash.text: flash.o
	${OBJCOPY} -j .text -O binary $< $@

flash.base64: flash.text
	base64 < $< >$@.tmp
	mv $@.tmp $@
