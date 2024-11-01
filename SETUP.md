# Chip Setup

## Install the bootloader

1. Clone [`embassy-rs`]
1. `$ cd examples/boot/bootloader/rp`
1. Apply `embassy-boot.patch`
1. `$ cargo run --release`

## Run self-debug

`$ cargo run --release`

or for dry-run (no actual flashing) and more logging output:

`$ DEFMT_LOG=info cargo build --release --features flash-dry-run`

## Attach probe-rs OTA

```bash
$ cd probe-rs

$ ip=pico0

$ cargo run \
	run \
	--probe "0:0:$ip:1234" \
	--chip RP2040_SELFDEBUG_TARGET_SELECT \
	../embassy-net-rp-self-debug/target/thumbv6m-none-eabi/release/embassy-net-rp-self-debug
```
