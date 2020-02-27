CARGO?=cargo
MULTIBOOT2_TARGET?=multiboot2_target

multiboot2_binary = target/$(MULTIBOOT2_TARGET)/debug/mythril_multiboot2
mythril_src = $(shell find . -type f -name '*.rs' -or -name '*.S' -or -name '*.ld')
seabios = seabios/out/bios.bin

ifneq (,$(filter qemu%, $(firstword $(MAKECMDGOALS))))
    QEMU_EXTRA := $(subst :,\:, $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS)))
    $(eval $(QEMU_EXTRA):;@:)
endif

.PHONY: all
all: multiboot2 $(seabios)

.PHONY: multiboot2
multiboot2: $(multiboot2_binary)

$(seabios):
	cp scripts/seabios.config seabios/.config
	make -C seabios

.PHONY: qemu
qemu: all
	./scripts/mythril-run.sh $(multiboot2_binary) $(QEMU_EXTRA)

.PHONY: qemu-debug
qemu-debug: all
	./scripts/mythril-run.sh $(multiboot2_binary) \
	    -gdb tcp::1234 -S $(QEMU_EXTRA)

$(multiboot2_binary): $(mythril_src)
	$(CARGO) xbuild \
	    --target mythril_multiboot2/$(MULTIBOOT2_TARGET).json \
	    --manifest-path mythril_multiboot2/Cargo.toml

.PHONY: fmt
fmt:
	$(CARGO) fmt --all -- --check

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
	make -C seabios clean

.PHONY: help
help:
	@echo " Make Targets:"
	@echo "   all            build everything to run mythril, but do not start qemu"
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
