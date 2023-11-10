INSTALL_DIR ?= /usr/local

.PHONY: install
install:
	cargo build --release
	cp target/release/sermaid $(INSTALL_DIR)
