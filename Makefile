CARGO?=cargo
BUILD_TYPE?=release
BUILD_REPO_TAG=16
DOCKER_IMAGE=adamschwalm/hypervisor-build:$(BUILD_REPO_TAG)

TEST_IMAGE_REPO=https://github.com/mythril-hypervisor/build
TEST_INITRAMFS_URL=$(TEST_IMAGE_REPO)/releases/download/$(BUILD_REPO_TAG)/test-initramfs.img
TEST_KERNEL_URL=$(TEST_IMAGE_REPO)/releases/download/$(BUILD_REPO_TAG)/test-bzImage

CARGO_BUILD_JOBS?=$(shell grep -c '^processor' /proc/cpuinfo)
KVM_GROUP_ID?=$(shell grep kvm /etc/group | cut -f 3 -d:)

mythril_binary = mythril/target/mythril_target/$(BUILD_TYPE)/mythril
mythril_src = $(shell find mythril* -type f -name '*.rs' -or -name '*.S' -or -name '*.ld' \
	                   -name 'Cargo.toml')

seabios = seabios/out/bios.bin
seabios_blob = mythril/src/blob/bios.bin
guest_kernel = scripts/vmlinuz
guest_initramfs = scripts/initramfs
git_hooks_src = $(wildcard .mythril_githooks/*)
git_hooks = $(subst .mythril_githooks,.git/hooks,$(git_hooks_src))

GUEST_ASSETS=$(guest_kernel) $(guest_initramfs)

ifneq (,$(filter qemu%, $(firstword $(MAKECMDGOALS))))
    QEMU_EXTRA := $(subst :,\:, $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS)))
    $(eval $(QEMU_EXTRA):;@:)
endif

CARGO_MANIFEST?=--manifest-path mythril/Cargo.toml

ifeq ($(BUILD_TYPE), release)
    CARGO_BUILD_FLAGS := --release
endif

.PHONY: all
all: mythril $(seabios)

.PHONY: mythril
mythril: $(mythril_binary)

.PHONY: mythril-debug
mythril-debug: BUILD_TYPE=debug
mythril-debug: $(mythril_binary)

$(guest_kernel):
	curl -L $(TEST_KERNEL_URL) -o $(guest_kernel)

$(guest_initramfs):
	curl -L $(TEST_INITRAMFS_URL) -o $(guest_initramfs)

docker-%:
	docker run --privileged -ti --rm -w $(CURDIR) -v $(CURDIR):$(CURDIR) \
	   -u $(shell id -u):$(shell id -g) \
	   --group-add=$(KVM_GROUP_ID) \
	   -e CARGO_HOME=$(CURDIR)/mythril/.cargo \
	   -e CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS) \
	   $(DOCKER_IMAGE) /bin/bash -c '$(MAKE) $*'

$(seabios):
	cp scripts/seabios.config seabios/.config
	make -j $(CARGO_BUILD_JOBS) -C seabios

$(seabios_blob): $(seabios)
	cp $(seabios) $(seabios_blob)

.PHONY: qemu
qemu: mythril $(GUEST_ASSETS)
	./scripts/mythril-run.sh $(mythril_binary) $(QEMU_EXTRA)

.PHONY: qemu-debug
qemu-debug: mythril-debug $(GUEST_ASSETS)
	./scripts/mythril-run.sh $(mythril_binary) \
	    -gdb tcp::1234 -S $(QEMU_EXTRA)

$(mythril_binary): $(mythril_src) $(seabios_blob)
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
test_common: $(seabios_blob)
	$(CARGO) test $(CARGO_MANIFEST) --lib \
	    --features=test \

.PHONY: test
test: test_common

.PHONY: clean
clean:
	$(CARGO) clean $(CARGO_MANIFEST)
	rm $(seabios_blob)
	make -C seabios clean
	rm $(GUEST_ASSETS)

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
