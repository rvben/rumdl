.PHONY: build test clean fmt check doc

build:
	cargo build --release

test:
	cargo test

clean:
	cargo clean

fmt:
	cargo fmt

check:
	cargo check
	cargo clippy

doc:
	cargo doc --no-deps

watch-test:
	cargo watch -x test

all: fmt check test build 