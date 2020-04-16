#!/bin/sh

TOP=$(git rev-parse --show-toplevel)
qemu-system-aarch64 \
    -nographic \
    -M raspi3 \
    -serial null -serial mon:stdio \
    -kernel \
    "$@"
