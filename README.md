# mythril

A rust-based hypervisor currently requiring multiboot2 boot (e.g. grub), and VT-x/EPT.

## Building and Testing

`mythril` should be built and tested using the provided docker image
`adamschwalm/hypervisor-build:docker-4`. For example, to build the
multiboot application, run:

```
docker run -v $(pwd):/src adamschwalm/hypervisor-build:docker-4 make multiboot2
```

This will create the multiboot2 application in `target/multiboot2_target/debug/mythril_multiboot2`.
Similarly, unittests can be executed like:

```
docker run -v $(pwd):/src adamschwalm/hypervisor-build:docker-4 make test
```

## Running the Hypervisor

After building the multiboot2 application as described above, the image can be executed
with:

```
make qemu
```

Note that this has only been tested on relatively recent versions of QEMU (v4.1.0+).
Older versions may contain bugs that could cause issues running the image.
