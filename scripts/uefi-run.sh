#!/bin/bash

# A very hacky script to just run the compiled hypervisor

set -e

if [ $# -eq 0 ]; then
    echo "Usage: $0 <path to mythril.efi> [<other file to include>]..."
    exit 1
fi

rm -r -f _boot.img _mnt
mkfs.fat -C _boot.img 51200

mkdir _mnt
mount _boot.img _mnt

cat > _mnt/startup.nsh <<EOF
echo Starting UEFI application...
fs0:
run.efi
EOF

cp "$1" _mnt/run.efi

if [ $# -gt 1 ]; then
    cp -r "${@:2}" _mnt/
fi

umount _mnt
rm -rf _mnt

qemu-system-x86_64 -bios ./scripts/OVMF.fd \
                   -enable-kvm \
                   -cpu host \
                   -nographic \
                   -drive file=_boot.img,index=0,media=disk,format=raw \
                   -net none \
                   -gdb tcp::1234 \
                   -debugcon file:debug.log \
                   -no-reboot \
                   -global isa-debugcon.iobase=0x402 \
                   -m 1G
