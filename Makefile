INSTALL_DIR = ~/utils/

.PHONY: install
install:
	cargo build --release
	cp target/release/sermaid $(INSTALL_DIR)