#!/bin/sh

if test $# -ne 1
then
	echo >&2 "Usage: $0 file.o"
	exit 2
fi

if objdump -h "$1" | grep -Ei '\.(bss|(ro)?data)'
then
	echo >&2 "$0: found data sections in flash algo code (see above)"
	exit 1
fi
