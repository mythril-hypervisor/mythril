CARGO?=cargo
MULTIBOOT2_TARGET?=multiboot2_target

mkfile_dir := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

multiboot2_binary = target/$(MULTIBOOT2_TARGET)/debug/mythril_multiboot2
mythril_src = $(shell find . -type f -name '*.rs' -or -name '*.S' -or -name '*.ld')

ifneq (,$(filter qemu%, $(firstword $(MAKECMDGOALS))))
    QEMU_EXTRA := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
    $(eval $(QEMU_EXTRA):;@:)
endif

.PHONY: fmt
fmt:
	$(CARGO) fmt --all -- --check

multiboot2: $(multiboot2_binary)

.PHONY: qemu
qemu: $(multiboot2_binary)
	./scripts/mythril-run.sh $(multiboot2_binary) $(QEMU_EXTRA)

.PHONY: qemu-debug
qemu-debug: $(multiboot2_binary)
	./scripts/mythril-run.sh $(multiboot2_binary) \
	    -gdb tcp::1234 -S $(QEMU_EXTRA)

$(multiboot2_binary): $(mythril_src)
	$(CARGO) xbuild \
	    --target mythril_multiboot2/$(MULTIBOOT2_TARGET).json \
	    --manifest-path mythril_multiboot2/Cargo.toml

.PHONY: test_core
test_core:
	$(CARGO) test \
	    --manifest-path mythril_core/Cargo.toml \
	    --lib

.PHONY: test
test: test_core

.PHONY: clean
clean:
	$(CARGO) clean

.PHONY: help
help:
	@echo " Make Targets:"
	@echo "   fmt            run rustfmt"
	@echo "   qemu           run mythril in a VM"
	@echo "   qemu-debug     run mythril in a VM, but halt for a debugger connection"
	@echo "   test           run the mythril tests"
	@echo "   clean          clean the build state"
	@echo "   help           this"
	@echo ""
	@echo " Examples:"
	@echo ""
	@echo "   make qemu"
	@echo "   make qemu -- -serial pty -monitor stdio"
