# mythril

A rust-based hypervisor currently requiring multiboot2 boot (e.g. grub), and VT-x/EPT.

## Building and Testing

`mythril` should be built and tested using the provided docker image
`adamschwalm/hypervisor-build`. There are convenience `make` rules for
using this image. For example, to build the multiboot application, run:

```
make docker-all
```

This will create the hypervisor in `mythril/target/mythril_target/release/mythril`.
It will also compile the patched versions for seabios and the linux kernel that
are currently required to use `mythril`. Unittests can be executed like:

```
make docker-test
```

## Running the Hypervisor

After running the build steps as described above, an initramfs must be added to the
`scripts/` directory with the name `initramfs`. Once in place, the hypervisor
can be executed with:

```
make docker-qemu
```

Note that this has only been tested on relatively recent versions of QEMU (v4.1.0+).
Older versions may contain bugs that could cause issues running the image.

## Debugging

To debug mythril, run `BUILD_TYPE=debug make qemu-debug`. This will build a debug version
of the hypervisor then start start QEMU in a paused state. You can then run
`gdb mythril/target/mythril_target/debug/mythril` to launch gdb with the debug info from
the application. You can attach to the qemu instance with `target remote :1234`. Note that
debugging the hypervisor is generally not supported under docker.

Because the virtualization is hardware accelerated, remember to use `hbreak` instead
of `break` in gdb. For example, to put a breakpoint at the start of `kmain` and start
mythril, run:

```
(gdb) target remote :1234
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