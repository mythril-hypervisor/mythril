#!/bin/bash

# A very hacky script to just run the compiled hypervisor

set -e

rm -r -f _boot.img _mnt
mkfs.msdos -C _boot.img 51200

mkdir _mnt
mount _boot.img _mnt

cat > _mnt/startup.nsh <<EOF
echo Starting UEFI application...
fs0:
run.efi
EOF

cp "$1" _mnt/run.efi

umount _mnt
rm -rf _mnt

qemu-system-x86_64 -bios /usr/share/ovmf/OVMF.fd \
                   -enable-kvm \
                   -cpu host \
                   -nographic \
                   -drive file=_boot.img,index=0,media=disk,format=raw \
                   -net none \
                   -gdb tcp::1234 \
                   -debugcon file:debug.log \
                   -global isa-debugcon.iobase=0x402
