.PHONY: run install test check bundle clean

# Build the bundle and launch it (kills any running copy first).
TARGET_DIR := $(shell cargo metadata --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')

run: bundle
	@pkill -x claudebar 2>/dev/null || true
	open "$(TARGET_DIR)/Claudebar.app"

# Put the app somewhere permanent and run it from there. Launch-at-login
# registers whatever path the app was launched from, so a copy that lives in
# /Applications is the one worth registering.
install: bundle
	@pkill -x claudebar 2>/dev/null || true
	rm -rf "/Applications/Claudebar.app"
	cp -R "$(TARGET_DIR)/Claudebar.app" "/Applications/Claudebar.app"
	open "/Applications/Claudebar.app"

test:
	cargo test

check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

bundle:
	./scripts/bundle.sh

clean:
	cargo clean
