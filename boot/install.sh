#!/usr/bin/env bash
sudo mount /dev/mmcblk0p1 /mnt/flashdrive
make
sudo ../bin/install-kernel.py build/boot.elf
