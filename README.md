# mythril

[![Actions Status](https://github.com/ALSchwalm/mythril/workflows/Mythril/badge.svg)](https://github.com/ALSchwalm/mythril/actions)

A rust-based hypervisor currently requiring EFI boot, and VT-x/EPT.

## Building and Testing

`mythril` should be built and tested using the provided docker image
`adamschwalm/hypervisor-build:docker-2`. For example, to build the
EFI application, run:

```
docker run -v $(pwd):/src adamschwalm/hypervisor-build:docker-2 make efi
```

This will create the EFI application in `target/x86_64-unknown-uefi/debug/mythril_efi.efi`.
Similarly, unittests can be executed like:

```
docker run -v $(pwd):/src adamschwalm/hypervisor-build:docker-2 make test
```

## Running the Hypervisor

After building the EFI application as described above, the image can be executed
with:

```
make qemu
```

Note that this has only been tested on relatively recent versions of QEMU (v4.1.0+).
Older versions may contain bugs that could cause issues running the image.
