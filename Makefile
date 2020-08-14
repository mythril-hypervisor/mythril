CARGO?=cargo
MULTIBOOT2_TARGET?=multiboot2_target
BUILD_TYPE?=release
DOCKER_IMAGE=adamschwalm/hypervisor-build:9

multiboot2_binary = target/$(MULTIBOOT2_TARGET)/$(BUILD_TYPE)/mythril_multiboot2
mythril_src = $(shell find . -type f -name '*.rs' -or -name '*.S' -or -name '*.ld' \
	                   -name 'Cargo.toml')
kernel = linux/arch/x86_64/boot/bzImage
seabios = seabios/out/bios.bin
git_hooks_src = $(wildcard .mythril_githooks/*)
git_hooks = $(subst .mythril_githooks,.git/hooks,$(git_hooks_src))

ifneq (,$(filter qemu%, $(firstword $(MAKECMDGOALS))))
    QEMU_EXTRA := $(subst :,\:, $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS)))
    $(eval $(QEMU_EXTRA):;@:)
endif

ifeq ($(BUILD_TYPE), release)
    CARGO_FLAGS := --release
endif

.PHONY: all
all: multiboot2 $(seabios) $(kernel)

.PHONY: multiboot2
multiboot2: $(multiboot2_binary)

.PHONY: multiboot2-debug
multiboot2-debug: BUILD_TYPE=debug
multiboot2-debug: $(multiboot2_binary)

docker-%:
	docker run --rm -w $(CURDIR) -v $(CURDIR):$(CURDIR) \
	   -u $(shell id -u):$(shell id -g) $(DOCKER_IMAGE) \
	   /bin/bash -c '$(MAKE) $*'

$(seabios):
	cp scripts/seabios.config seabios/.config
	make -C seabios

$(kernel):
	cp scripts/kernel.config linux/.config
	make -C linux bzImage

.PHONY: qemu
qemu: multiboot2 $(seabios) $(kernel)
	./scripts/mythril-run.sh $(multiboot2_binary) $(QEMU_EXTRA)

.PHONY: qemu-debug
qemu-debug: multiboot2-debug $(seabios) $(kernel)
	./scripts/mythril-run.sh $(multiboot2_binary) \
	    -gdb tcp::1234 -S $(QEMU_EXTRA)

$(multiboot2_binary): $(mythril_src)
	$(CARGO) xbuild $(CARGO_FLAGS) \
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
	make -C linux clean

.PHONY: dev-init
dev-init: install-git-hooks

.PHONY: install-git-hooks
install-git-hooks: $(git_hooks)

$(git_hooks): $(git_hooks_src)
	ln -s $(shell realpath --relative-to=.git/hooks $<) $@

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
