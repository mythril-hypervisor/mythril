#!/bin/bash

# A very hacky script to just run the compiled hypervisor

set -e

if [ $# -eq 0 ]; then
    echo "Usage: $0 <path to mythril binary> [<other args passed to qemu>]..."
    exit 1
fi

rm -rf _isofiles
mkdir -p _isofiles/boot/grub

cp scripts/grub.cfg _isofiles/boot/grub/
cp seabios/out/bios.bin _isofiles/boot/seabios.bin
cp scripts/linuxboot_dma.bin _isofiles/boot/linuxboot_dma.bin
cp scripts/vmlinuz _isofiles/boot/vmlinuz
cp scripts/initramfs _isofiles/boot/initramfs
cp "$1" _isofiles/boot/mythril.bin

# Explicitly avoid using grub efi for now
grub-mkrescue -d /usr/lib/grub/i386-pc -o os.iso _isofiles

qemu-system-x86_64 -enable-kvm \
                   -cpu host \
                   -smp cores=2,threads=1,sockets=1 \
                   -nographic \
                   -cdrom os.iso \
                   -net none \
                   -debugcon file:debug.log \
                   -no-reboot \
                   -global isa-debugcon.iobase=0x402 \
                   -m 1G "${@:2}"
