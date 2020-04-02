#!/usr/bin/env bash
make
sudo ttywrite -i build/kernel.bin /dev/ttyUSB0 && sudo screen /dev/ttyUSB0 115200
