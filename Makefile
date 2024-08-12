show: flash.o
	objdump -d $<

flash.o: src/bin/flash.rs
	rustc --target=thumbv6m-none-eabi -C opt-level=3 --crate-type=lib --emit=obj $< -o $@
