# mythril

A rust-based hypervisor currently requiring multiboot2 boot (e.g. grub), and VT-x/EPT.

## Building and Testing

`mythril` should be built and tested using the provided docker image
`adamschwalm/hypervisor-build:6`. For example, to build the
multiboot application, run:

```
docker run -v $(pwd):/src adamschwalm/hypervisor-build:6 make
```

This will create the multiboot2 application in `target/multiboot2_target/debug/mythril_multiboot2`.
Similarly, unittests can be executed like:

```
docker run -v $(pwd):/src adamschwalm/hypervisor-build:6 make test
```

## Running the Hypervisor

After building the multiboot2 application as described above, a linux kernel and initramfs
must be added to the `scripts/` directory. The kernel must be named `vmlinuz` and the
initramfs must be named `initramfs`. _Note that currently, copying these images in to the
guest is very slow, so avoid using a large initramfs_. Once in place, the hypervisor
can be executed with:

```
make qemu
```

Note that this has only been tested on relatively recent versions of QEMU (v4.1.0+).
Older versions may contain bugs that could cause issues running the image.

## Debugging

To debug mythril, first build the multiboot application as described above. Then
run `make qemu-debug`. This will start start QEMU but not launch mythril. You can
then run `gdb target/multiboot2_target/debug/mythril_multiboot2` to launch gdb with
the debug info from the application. You can then attach to the qemu instance with
`target remote localhost:1234`.

Because the virtualization is hardware accelerated, remember to use `hbreak` instead
of `break` in gdb. For example, to put a breakpoint at the start of `kmain` and start
mythril, run:

```
(gdb) target remote localhost:1234
Remote debugging using localhost:1234
0x000000000000fff0 in ?? ()
(gdb) hbreak kmain
Hardware assisted breakpoint 1 at 0x110d54: file mythril_multiboot2/src/main.rs, line 151.
(gdb) continue
Continuing.

Breakpoint 1, kmain (multiboot_info_addr=10993664) at mythril_multiboot2/src/main.rs:151
151	   unsafe { interrupt::idt::init() };
```

You can then use `step` and other debugging functions as usual.