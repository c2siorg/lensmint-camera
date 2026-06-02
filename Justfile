TARGET := "aarch64-unknown-linux-gnu"
PI_USER := "tenerife"
PI_IP := "192.168.210.101"
APP_DIR := "rust-camera-daemon/lensmint-daemon"

# 强制 Cargo 使用 AArch64 链接器
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER := "aarch64-linux-gnu-gcc"
export CC_aarch64_unknown_linux_gnu := "aarch64-linux-gnu-gcc"
export CXX_aarch64_unknown_linux_gnu := "aarch64-linux-gnu-g++"

build:
	cd {{APP_DIR}} && cargo build --target {{TARGET}} --release

deploy: build
	ssh {{PI_USER}}@{{PI_IP}} "killall -9 lensmint-daemon || true"
	scp {{APP_DIR}}/target/{{TARGET}}/release/lensmint-daemon {{PI_USER}}@{{PI_IP}}:/tmp/lensmint-daemon

run: deploy
	ssh {{PI_USER}}@{{PI_IP}} "libcamerify /tmp/lensmint-daemon"

dev: run