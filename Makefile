CARGO?=cargo
BUILD_TYPE?=release
DOCKER_IMAGE=adamschwalm/hypervisor-build:12

mythril_binary = mythril/target/mythril_target/$(BUILD_TYPE)/mythril
mythril_src = $(shell find mythril* -type f -name '*.rs' -or -name '*.S' -or -name '*.ld' \
	                   -name 'Cargo.toml')
kernel = linux/arch/x86_64/boot/bzImage
seabios = seabios/out/bios.bin
git_hooks_src = $(wildcard .mythril_githooks/*)
git_hooks = $(subst .mythril_githooks,.git/hooks,$(git_hooks_src))

ifneq (,$(filter qemu%, $(firstword $(MAKECMDGOALS))))
    QEMU_EXTRA := $(subst :,\:, $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS)))
    $(eval $(QEMU_EXTRA):;@:)
endif

CARGO_MANIFEST?=--manifest-path mythril/Cargo.toml

ifeq ($(BUILD_TYPE), release)
    CARGO_BUILD_FLAGS := --release
endif

.PHONY: all
all: mythril $(seabios) $(kernel)

.PHONY: mythril
mythril: $(mythril_binary)

.PHONY: mythril-debug
mythril-debug: BUILD_TYPE=debug
mythril-debug: $(mythril_binary)

docker-%:
	docker run --privileged -it --rm -w $(CURDIR) -v $(CURDIR):$(CURDIR) \
	   -u $(shell id -u):$(shell id -g) $(DOCKER_IMAGE) \
	   /bin/bash -c '$(MAKE) $*'

$(seabios):
	cp scripts/seabios.config seabios/.config
	make -C seabios

$(kernel):
	cp scripts/kernel.config linux/.config
	make -C linux bzImage

.PHONY: qemu
qemu: mythril $(seabios) $(kernel)
	./scripts/mythril-run.sh $(mythril_binary) $(QEMU_EXTRA)

.PHONY: qemu-debug
qemu-debug: mythril-debug $(seabios) $(kernel)
	./scripts/mythril-run.sh $(mythril_binary) \
	    -gdb tcp::1234 -S $(QEMU_EXTRA)

$(mythril_binary): $(mythril_src)
	$(CARGO) build $(CARGO_BUILD_FLAGS) $(CARGO_MANIFEST) \
	    -Z build-std=core,alloc \
	    --target mythril/mythril_target.json \

.PHONY: check-fmt
check-fmt:
	$(CARGO) fmt $(CARGO_MANIFEST) --all -- --check

.PHONY: fmt
fmt:
	$(CARGO) fmt $(CARGO_MANIFEST) --all

.PHONY: test_core
test_common:
	$(CARGO) test $(CARGO_MANIFEST) --lib \
	    --features=test \

.PHONY: test
test: test_common

.PHONY: clean
clean:
	$(CARGO) clean $(CARGO_MANIFEST)
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
	@echo "   check-fmt      run cargo fmt --check"
	@echo "   fmt            run cargo fmt"
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
