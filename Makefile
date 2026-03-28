.PHONY: build test clean fmt check doc build-python build-wheel dev-install setup-mise dev-setup dev-verify update-dependencies update-rust-version build-static-linux-x64 build-static-linux-arm64 build-static-all schema check-schema benchmark benchmark-run benchmark-chart lint-actions fuzz fuzz-long check-links

# Development environment setup
setup-mise:
	@echo "Checking if mise is installed..."
	@command -v mise >/dev/null 2>&1 || { \
		echo "mise is not installed. Installing mise..."; \
		curl https://mise.run | sh; \
		echo 'eval "$$(~/.local/bin/mise activate bash)"' >> ~/.bashrc; \
		echo 'eval "$$(~/.local/bin/mise activate zsh)"' >> ~/.zshrc; \
		echo ""; \
		echo "mise installed! Please run:"; \
		echo "  source ~/.bashrc  # or source ~/.zshrc"; \
		echo "Then run 'make dev-setup' to continue"; \
		exit 1; \
	}
	@echo "mise is installed at: $$(which mise)"

dev-setup: setup-mise
	@echo "Installing development environment with mise..."
	mise install
	@echo ""
	@echo "Development environment setup complete!"
	@echo "Run 'make dev-verify' to verify the installation"

dev-verify:
	@echo "Verifying development environment..."
	@echo "===================="
	@echo "Rust version: $$(rustc --version)"
	@echo "Cargo version: $$(cargo --version)"
	@echo "Python version: $$(python --version)"
	@echo "cargo-nextest: $$(cargo nextest --version 2>/dev/null || echo 'not installed')"
	@echo "maturin: $$(maturin --version 2>/dev/null || echo 'not installed')"
	@echo "cargo-binstall: $$(cargo binstall --version 2>/dev/null || echo 'not installed')"
	@echo "===================="

# CI-specific setup (uses mise if available, falls back to direct installation)
ci-setup:
	@if command -v mise >/dev/null 2>&1; then \
		echo "Using mise for CI setup..."; \
		mise install; \
	else \
		echo "mise not found, using direct installation..."; \
		if ! command -v cargo-nextest >/dev/null 2>&1; then \
			echo "Installing cargo-nextest..."; \
			curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C $${CARGO_HOME:-~/.cargo}/bin; \
		fi; \
		if ! command -v cargo-binstall >/dev/null 2>&1; then \
			echo "Installing cargo-binstall..."; \
			curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash; \
		fi; \
	fi

# Install mise in CI environment
ci-install-mise:
	@echo "Installing mise for CI..."
	@curl https://mise.run | MISE_INSTALL_PATH=/usr/local/bin/mise sh
	@echo "mise installed at: $$(which mise)"

build:
	cargo build --release

# Static binary builds for Linux (musl)
build-static-linux-x64:
	@echo "Building static Linux x86_64 binary..."
	rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
	mise exec -- cargo zigbuild --release --target x86_64-unknown-linux-musl
	@echo "Static binary built at: target/x86_64-unknown-linux-musl/release/rumdl"

build-static-linux-arm64:
	@echo "Building static Linux ARM64 binary..."
	rustup target add aarch64-unknown-linux-musl 2>/dev/null || true
	mise exec -- cargo zigbuild --release --target aarch64-unknown-linux-musl
	@echo "Static binary built at: target/aarch64-unknown-linux-musl/release/rumdl"

build-static-all: build-static-linux-x64 build-static-linux-arm64
	@echo "All static Linux binaries built successfully"

test:
	cargo nextest run --profile dev

test-legacy:
	cargo test

test-nextest:
	cargo nextest run

test-dev:
	cargo nextest run --profile dev

test-quick:
	cargo nextest run --profile quick

test-pre-commit:
	cargo nextest run --profile pre-commit

test-smoke:
	cargo nextest run --profile smoke

test-push:
	@echo "Running CI test suite (excludes performance tests)..."
	cargo nextest run --profile ci

test-ci:
	cargo nextest run --profile ci

test-performance:
	@echo "Running performance tests (this may take a few minutes)..."
	@echo "Tests run serially to reduce noise - be patient!"
	cargo nextest run --profile performance

test-complexity:
	@echo "Running O(n²) complexity regression tests..."
	@echo "These tests verify all rules maintain linear O(n) complexity."
	cargo nextest run --profile performance -E 'test(linear_complexity)'

# Fuzz testing (requires nightly Rust)
fuzz:
	@echo "Running fuzz test for fix idempotency (30 seconds)..."
	cargo +nightly fuzz run fuzz_fix_idempotency -- -max_total_time=30

fuzz-long:
	@echo "Running extended fuzz test (5 minutes)..."
	cargo +nightly fuzz run fuzz_fix_idempotency -- -max_total_time=300

clean:
	cargo clean

fmt:
	cargo fmt
	cargo clippy --fix --allow-dirty --allow-staged -- -D clippy::uninlined_format_args
	cargo fix --allow-dirty --allow-staged

lint-actions:
	actionlint

lint:
	CARGO_INCREMENTAL=1 cargo clippy --workspace --lib --bins --tests -- -D warnings -D clippy::uninlined_format_args
	$(MAKE) lint-actions

lint-all:
	CARGO_INCREMENTAL=1 cargo clippy --all-targets --all-features -- -D warnings -D clippy::uninlined_format_args
	$(MAKE) lint-actions

lint-fast:
	CARGO_INCREMENTAL=1 cargo clippy --workspace --lib --bins -- -D warnings -D clippy::uninlined_format_args

check:
	cargo check --all-targets --all-features

# Generate JSON schema for rumdl.toml
schema:
	cargo run --bin rumdl -- schema generate

# Check if JSON schema is up-to-date
check-schema:
	cargo run --bin rumdl -- schema check

doc:
	cargo doc --no-deps

watch-test:
	cargo watch -x "nextest run --profile quick"

all: fmt check test build

# Python-specific targets
build-python:
	maturin build --release

build-wheel:
	maturin build --release --strip --interpreter python3

dev-install:
	maturin develop --release

maturin-build:
	uv run --with pip,maturin[zig],cffi maturin build --release

maturin-sdist:
	uv run --with pip,maturin[zig],cffi maturin sdist

run:
	cargo run --release --bin rumdl check .

run-readme:
	cargo run --release --bin rumdl check README.md

run-small:
	cargo run --release --bin rumdl check benchmark/test-data/small

run-medium:
	cargo run --release --bin rumdl check benchmark/test-data/medium

run-large:
	cargo run --release --bin rumdl check benchmark/test-data/large

run-rule:
	cargo run --release --bin rumdl -- rule MD001

run-config:
	cargo run --release --bin rumdl -- config

run-config-defaults:
	cargo run --release --bin rumdl -- config --defaults

run-config-toml:
	cargo run --release --bin rumdl -- config --output toml

run-config-defaults-toml:
	cargo run --release --bin rumdl -- config --defaults --output toml

run-config-defaults-smart:
	cargo run --release --bin rumdl -- config --defaults --output smart

run-help:
	cargo run --release --bin rumdl -- help

trigger-pre-commit:
	curl -X POST \
	-H "Accept: application/vnd.github+json" \
	-H "Authorization: Bearer $(PRECOMMIT_DISPATCH_TOKEN)" \
	https://api.github.com/repos/rvben/rumdl-pre-commit/dispatches \
	-d '{"event_type": "pypi_release"}'

# Dependency and version update targets
update-dependencies:
	@echo "Updating Cargo dependencies to latest compatible versions..."
	@cargo update
	@echo "Dependencies updated in Cargo.lock"
	@echo ""
	@if command -v cargo-outdated >/dev/null 2>&1; then \
		echo "Checking for available updates beyond current constraints:"; \
		cargo outdated; \
	else \
		echo "Install cargo-outdated for more detailed update information:"; \
		echo "  cargo install cargo-outdated"; \
	fi

update-rust-version:
	@echo "Checking for latest stable Rust version..."
	$(eval LATEST_RUST := $(shell curl -s https://api.github.com/repos/rust-lang/rust/releases/latest | grep '"tag_name":' | sed -E 's/.*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/' | head -1))
	$(eval CURRENT_RUST := $(shell grep '^rust-version' Cargo.toml | sed -E 's/rust-version = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/'))
	@if [ -z "$(LATEST_RUST)" ]; then \
		echo "Failed to fetch latest Rust version"; \
		exit 1; \
	fi
	@echo "Current Rust version: $(CURRENT_RUST)"
	@echo "Latest Rust version: $(LATEST_RUST)"
	@if [ "$(CURRENT_RUST)" = "$(LATEST_RUST)" ]; then \
		echo "Already using the latest Rust version"; \
	else \
		echo "Updating Rust version to $(LATEST_RUST)..."; \
		sed -i.bak -E 's/^rust-version = "[0-9]+\.[0-9]+\.[0-9]+"/rust-version = "$(LATEST_RUST)"/' Cargo.toml; \
		sed -i.bak -E 's/^rust = "[0-9]+\.[0-9]+\.[0-9]+"/rust = "$(LATEST_RUST)"/' .mise.toml; \
		sed -i.bak -E 's/^channel = "[0-9]+\.[0-9]+\.[0-9]+"/channel = "$(LATEST_RUST)"/' rust-toolchain.toml; \
		rm -f Cargo.toml.bak .mise.toml.bak rust-toolchain.toml.bak; \
		echo "Updated Rust version in Cargo.toml, .mise.toml, and rust-toolchain.toml"; \
		echo "Running 'cargo check' to verify compatibility..."; \
		cargo check || (echo "Warning: cargo check failed. You may need to fix compatibility issues."; exit 1); \
	fi

# Benchmark targets
benchmark: benchmark-run benchmark-chart

benchmark-run:
	@echo "Running cold start benchmarks..."
	@python3 scripts/benchmark_cold_start.py

benchmark-chart:
	@echo "Generating benchmark chart..."
	@uv run --with matplotlib python3 scripts/generate_benchmark_chart.py

LYCHEE := $(shell command -v lychee 2>/dev/null || echo "mise exec -- lychee")

check-links:
	@echo "Checking links in markdown files..."
	$(LYCHEE) --no-progress --config .lychee.toml --remap 'https://rumdl.dev/([^/]+)/? file://$(CURDIR)/docs/$$1.md' 'README.md' 'docs/**/*.md'

# Documentation validation
test-doc-completeness:
	cargo test --test config_documentation_completeness -- --nocapture
