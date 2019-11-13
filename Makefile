CARGO?=cargo
CARGO_TOOLCHAIN?=nightly
UEFI_TARGET?=x86_64-unknown-uefi

efi_binary = target/$(UEFI_TARGET)/debug/mythril_efi.efi
rs_src = $(shell find . -type f -name '*.rs')

.PHONY: fmt
fmt:
	$(CARGO) +$(CARGO_TOOLCHAIN) fmt -- --check

efi: $(efi_binary)

qemu: $(efi_binary)
	sudo ./scripts/uefi-run.sh $(efi_binary) scripts/OVMF.fd

$(efi_binary): $(rs_src)
	$(CARGO) +$(CARGO_TOOLCHAIN) xbuild \
		--target $(UEFI_TARGET) --manifest-path mythril_efi/Cargo.toml

.PHONY: test_core
test_core:
	cd mythril_core; $(CARGO) +$(CARGO_TOOLCHAIN) test --lib

.PHONY: test
test: test_core

.PHONY: clean
clean:
	$(CARGO) +$(CARGO_TOOLCHAIN) clean
