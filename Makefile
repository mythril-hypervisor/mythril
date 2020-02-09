CARGO?=cargo
CARGO_TOOLCHAIN?=nightly-2020-02-14-x86_64-unknown-linux-gnu
MULTIBOOT2_TARGET?=multiboot2_target

mkfile_dir := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

multiboot2_binary = target/$(MULTIBOOT2_TARGET)/debug/mythril_multiboot2
mythril_src = $(shell find . -type f -name '*.rs' -or -name '*.S')

.PHONY: fmt
fmt:
	$(CARGO) +$(CARGO_TOOLCHAIN) fmt --all -- --check

multiboot2: $(multiboot2_binary)

qemu: $(multiboot2_binary)
	./scripts/mythril-run.sh $(multiboot2_binary)

$(multiboot2_binary): $(mythril_src)
	$(CARGO) +$(CARGO_TOOLCHAIN) xbuild \
		--target mythril_multiboot2/$(MULTIBOOT2_TARGET).json \
	        --manifest-path mythril_multiboot2/Cargo.toml

.PHONY: test_core
test_core:
	cd mythril_core; $(CARGO) +$(CARGO_TOOLCHAIN) test --lib

.PHONY: test
test: test_core

.PHONY: clean
clean:
	$(CARGO) +$(CARGO_TOOLCHAIN) clean
