.PHONY: build test release clean install

# Default target
build:
	cargo build

# Run tests
test:
	cargo test

# Release - auto-increments patch version, or pass VERSION=x.y.z
release:
ifdef VERSION
	./scripts/release.sh $(VERSION)
else
	./scripts/release.sh
endif

# Build release binary
build-release:
	cargo build --release

# Install locally
install:
	cargo install --path .

# Clean build artifacts
clean:
	cargo clean

# Run clippy
lint:
	cargo clippy

# Check hooks are up to date
check:
	cargo run -- check
