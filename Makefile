.PHONY: build test clean fmt check doc build-python build-wheel dev-install setup-mise dev-setup dev-verify update-dependencies update-rust-version build-static-linux-x64 build-static-linux-arm64 build-static-all docker-binaries docker-binaries-release docker-binfmt docker-builder docker-build docker-verify docker-push schema check-schema sync-code-block-tools check-code-block-tools test-code-block-tools check-versions benchmark benchmark-run benchmark-chart lint-actions lint-actions-all fuzz fuzz-long check-links docs-check docs-smoke sync-rule-docs check-rule-docs release-patch release-minor release-major test-idempotency test-doc fuzz-all audit msrv-check smoke-wasi parity

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

# Build the CLI for WASI (wasm32-wasip1-threads). The `wasi` feature drops the
# native LSP/tokio/jemalloc stack. Run in CI so wasm support can't silently regress.
build-wasi:
	rustup target add wasm32-wasip1-threads 2>/dev/null || true
	cargo build --target wasm32-wasip1-threads --no-default-features --features wasi

# Build the browser/npm wasm package (wasm32-unknown-unknown via wasm-pack).
# Identical command to the release pipeline so local == CI == release; requires
# wasm-pack on PATH. Run in CI so the browser build can't silently regress.
build-wasm:
	rustup target add wasm32-unknown-unknown 2>/dev/null || true
	wasm-pack build --target web --no-default-features --features wasm

# Run the wasm binding host tests (native target, `wasm` feature). These exercise
# the JS-facing column/offset conversion and config parsing in src/wasm.rs, which
# the default test suite skips because wasm.rs is feature-gated. Run in CI so a
# regression in the browser/npm surface can't slip past the build-only check.
test-wasm:
	cargo test --features wasm --lib wasm::

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

# Container image (ghcr.io). The Docker build context is a staging directory
# holding prebuilt static musl binaries at binaries/<arch>/rumdl. CI populates
# it from release artifacts; `make docker-binaries` populates it from local
# cargo-zigbuild builds. VERSION defaults to the Cargo.toml version; CI passes
# the release tag explicitly.
#
# Two image flavours, both Dockerfile targets: scratch (binary only) owns
# the bare tags (:VERSION, :latest); alpine (binary on an Alpine base, with
# a shell for CI runners such as GitLab) gets suffixed tags
# (:VERSION-alpine, :alpine).
DOCKER_IMAGE ?= ghcr.io/rvben/rumdl
DOCKER_CONTEXT ?= target/docker
DOCKER_PLATFORMS ?= linux/amd64,linux/arm64
DOCKER_FLAVORS ?= scratch alpine
ALPINE_VERSION ?= 3.24
VERSION ?= $(shell awk -F '"' '/^version/ { print $$2; exit }' Cargo.toml)
REVISION ?= $(shell git rev-parse HEAD)

docker-binaries: build-static-all
	mkdir -p $(DOCKER_CONTEXT)/binaries/amd64 $(DOCKER_CONTEXT)/binaries/arm64
	cp target/x86_64-unknown-linux-musl/release/rumdl $(DOCKER_CONTEXT)/binaries/amd64/rumdl
	cp target/aarch64-unknown-linux-musl/release/rumdl $(DOCKER_CONTEXT)/binaries/arm64/rumdl
	@echo "Docker build context staged at: $(DOCKER_CONTEXT)"

# Stage the latest *released* musl binaries into the build context, without
# compiling anything. Lets CI (and a workstation without a musl toolchain)
# validate the Dockerfile and make wiring in seconds on every push, so image
# regressions surface before release time. `gh` needs GH_TOKEN in CI.
docker-binaries-release:
	rm -rf $(DOCKER_CONTEXT)/dl
	mkdir -p $(DOCKER_CONTEXT)/dl $(DOCKER_CONTEXT)/binaries/amd64 $(DOCKER_CONTEXT)/binaries/arm64
	gh release download --repo rvben/rumdl --pattern 'rumdl-*-unknown-linux-musl.tar.gz' --dir $(DOCKER_CONTEXT)/dl --clobber
	tar -xzf $(DOCKER_CONTEXT)/dl/rumdl-*-x86_64-unknown-linux-musl.tar.gz -C $(DOCKER_CONTEXT)/binaries/amd64
	tar -xzf $(DOCKER_CONTEXT)/dl/rumdl-*-aarch64-unknown-linux-musl.tar.gz -C $(DOCKER_CONTEXT)/binaries/arm64
	@echo "Docker build context staged at: $(DOCKER_CONTEXT) (latest release binaries)"

# Register QEMU binfmt handlers so non-native image platforms can run.
# Needed on bare-Linux hosts (CI runners); Docker Desktop ships emulation
# already, where this is a harmless no-op.
docker-binfmt:
	docker run --privileged --rm tonistiigi/binfmt --install all

# Multi-platform builds need a docker-container buildx builder; the default
# `docker` driver cannot assemble a multi-arch manifest. Created on demand,
# identically on a workstation and in CI. DOCKER_BUILDER is overridable so a
# pre-provisioned builder (e.g. one allowed to push to a local registry for
# an end-to-end docker-push test) can be substituted.
DOCKER_BUILDER ?= rumdl-builder

docker-builder:
	docker buildx inspect $(DOCKER_BUILDER) >/dev/null 2>&1 || \
		docker buildx create --name $(DOCKER_BUILDER) --driver docker-container

# Build the multi-arch image for every flavour (stays in the buildx cache;
# use docker-push to publish).
docker-build: docker-builder
	for flavor in $(DOCKER_FLAVORS); do \
		case $$flavor in \
			scratch) tags="-t $(DOCKER_IMAGE):$(VERSION) -t $(DOCKER_IMAGE):latest" ;; \
			*) tags="-t $(DOCKER_IMAGE):$(VERSION)-$$flavor -t $(DOCKER_IMAGE):$$flavor" ;; \
		esac && \
		echo "==> Building flavour $$flavor" && \
		docker buildx build \
			--builder $(DOCKER_BUILDER) \
			--platform $(DOCKER_PLATFORMS) \
			--build-arg VERSION=$(VERSION) \
			--build-arg REVISION=$(REVISION) \
			--build-arg ALPINE_VERSION=$(ALPINE_VERSION) \
			--target $$flavor-image \
			$$tags \
			-f Dockerfile $(DOCKER_CONTEXT) \
		|| exit 1; \
	done

# Build every flavour for every target platform and actually run each one:
# --version must work and a check on a known-clean file must exit 0. The
# non-native platform runs under QEMU emulation (see docker-binfmt), so a
# broken ENTRYPOINT, a non-static binary, or a wrong-arch COPY is caught for
# both architectures before anything is published. The scratch flavour has
# rumdl as ENTRYPOINT; the alpine flavour invokes it by name and must also
# start a shell, since a shell in the image is its reason to exist.
docker-verify:
	mkdir -p $(DOCKER_CONTEXT)/verify
	printf '# Sample\n\nA known-clean document.\n' > $(DOCKER_CONTEXT)/verify/sample.md
	for flavor in $(DOCKER_FLAVORS); do \
		case $$flavor in scratch) run="" ;; *) run="rumdl" ;; esac && \
		for platform in $$(echo "$(DOCKER_PLATFORMS)" | tr ',' ' '); do \
			echo "==> Verifying flavour $$flavor on $$platform" && \
			docker buildx build \
				--load \
				--platform "$$platform" \
				--build-arg VERSION=$(VERSION) \
				--build-arg REVISION=$(REVISION) \
				--build-arg ALPINE_VERSION=$(ALPINE_VERSION) \
				--target $$flavor-image \
				-t $(DOCKER_IMAGE):verify-$$flavor \
				-f Dockerfile $(DOCKER_CONTEXT) && \
			docker run --rm --platform "$$platform" $(DOCKER_IMAGE):verify-$$flavor $$run --version && \
			docker run --rm --platform "$$platform" \
				-v "$$(pwd)/$(DOCKER_CONTEXT)/verify:/data" \
				$(DOCKER_IMAGE):verify-$$flavor $$run check --no-cache --no-config sample.md \
			|| exit 1; \
			if [ "$$flavor" != "scratch" ]; then \
				docker run --rm --platform "$$platform" \
					$(DOCKER_IMAGE):verify-$$flavor /bin/sh -c 'rumdl --version' \
				|| exit 1; \
			fi; \
		done; \
	done

# Build and publish the multi-arch images for every flavour, with BuildKit
# SBOM and provenance attestations attached to the manifests, then assert
# the pushed manifests really contain every target platform.
docker-push: docker-builder
	for flavor in $(DOCKER_FLAVORS); do \
		case $$flavor in \
			scratch) tags="-t $(DOCKER_IMAGE):$(VERSION) -t $(DOCKER_IMAGE):latest" ;; \
			*) tags="-t $(DOCKER_IMAGE):$(VERSION)-$$flavor -t $(DOCKER_IMAGE):$$flavor" ;; \
		esac && \
		echo "==> Pushing flavour $$flavor" && \
		docker buildx build \
			--builder $(DOCKER_BUILDER) \
			--platform $(DOCKER_PLATFORMS) \
			--build-arg VERSION=$(VERSION) \
			--build-arg REVISION=$(REVISION) \
			--build-arg ALPINE_VERSION=$(ALPINE_VERSION) \
			--sbom=true \
			--provenance=mode=max \
			--target $$flavor-image \
			$$tags \
			--push \
			-f Dockerfile $(DOCKER_CONTEXT) \
		|| exit 1; \
	done
	for flavor in $(DOCKER_FLAVORS); do \
		case $$flavor in \
			scratch) tags="$(VERSION) latest" ;; \
			*) tags="$(VERSION)-$$flavor $$flavor" ;; \
		esac && \
		for tag in $$tags; do \
			for platform in $$(echo "$(DOCKER_PLATFORMS)" | tr ',' ' '); do \
				docker buildx imagetools inspect $(DOCKER_IMAGE):$$tag | grep -q "$$platform" \
					|| { echo "ERROR: $(DOCKER_IMAGE):$$tag is missing $$platform"; exit 1; }; \
			done; \
		done; \
	done

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

test-prek:
	cargo nextest run --profile prek

test-smoke:
	cargo nextest run --profile smoke

test-push:
	@echo "Running CI test suite (excludes performance tests)..."
	cargo nextest archive --archive-file /tmp/rumdl-nextest-archive.tar.zst --profile ci
	cargo nextest run --archive-file /tmp/rumdl-nextest-archive.tar.zst --profile ci
	@rm -f /tmp/rumdl-nextest-archive.tar.zst

test-ci:
	cargo nextest run --profile ci

# Run documentation tests. nextest cannot run doctests, so they are a separate
# gate; without this they compile-rot silently (the crate has runnable `///`
# examples with assertions).
test-doc:
	cargo test --doc

# Like test-ci but reports every failure instead of stopping at the first.
# Used by the non-blocking Windows canary so one run enumerates all
# platform-specific failures.
test-ci-no-fail-fast:
	cargo nextest run --profile ci --no-fail-fast

test-performance:
	@echo "Running performance tests (this may take a few minutes)..."
	@echo "Tests run serially to reduce noise - be patient!"
	cargo nextest run --profile performance

test-complexity:
	@echo "Running O(n²) complexity regression tests..."
	@echo "These tests verify all rules maintain linear O(n) complexity."
	cargo nextest run --profile performance -E 'test(linear_complexity)'

# Run idempotency property tests with elevated PROPTEST_CASES.
# This is on-demand because 2000 cases per rule is slow.
test-idempotency:
	@echo "Running idempotency property tests (2000 cases each, may take several minutes)..."
	PROPTEST_CASES=2000 cargo nextest run \
		--test lib \
		--run-ignored all \
		-E 'test(/rules::(formatter_proptest|idempotency_pipeline|idempotency_corpus)::/)'

# Fuzz testing (requires nightly Rust)
#
# cargo-fuzz defaults --target to the triple cargo-fuzz ITSELF was built for,
# not the host being built on. CI installs a prebuilt musl-static cargo-fuzz,
# which made every run default to x86_64-unknown-linux-musl and die with
# "sanitizer is incompatible with statically linked libc" before fuzzing a
# single input. Pinning the target to this host's own triple keeps that
# explicit and works unchanged on macOS and Linux.
FUZZ_TARGET ?= $(shell rustc -vV | sed -n 's/^host: //p')

fuzz:
	@echo "Running fuzz test for fix idempotency (30 seconds)..."
	cargo +nightly fuzz run --target $(FUZZ_TARGET) fuzz_fix_idempotency -- -max_total_time=30

fuzz-long:
	@echo "Running extended fuzz test (5 minutes)..."
	cargo +nightly fuzz run --target $(FUZZ_TARGET) fuzz_fix_idempotency -- -max_total_time=300

# Fuzz every target for a bounded time. Used by the scheduled fuzz workflow so
# the fix/lint/config/context paths get adversarial coverage on a cadence, not
# just fuzz_fix_idempotency on demand.
FUZZ_TIME ?= 120
fuzz-all:
	@echo "Fuzzing all targets ($(FUZZ_TIME)s each) for $(FUZZ_TARGET)..."
	@test -n "$(FUZZ_TARGET)" || { echo "FUZZ_TARGET is empty; could not detect host triple"; exit 1; }
	@for t in $$(cargo +nightly fuzz list); do \
		echo "=== fuzzing $$t ==="; \
		cargo +nightly fuzz run --target $(FUZZ_TARGET) $$t -- -max_total_time=$(FUZZ_TIME) || exit 1; \
	done

# Measure how closely rumdl and markdownlint agree over markdownlint's own test
# corpus (cloned and pinned by the script). Reports agreed / rumdl-only /
# markdownlint-only finding counts for the rules both tools implement, so parity
# is a tracked number rather than something noticed by hand. Pass MIN_AGREEMENT=N
# to fail below a floor, or ARGS='--json' for machine-readable output.
#
# markdownlint-cli2 is installed under target/parity rather than the repo root:
# the repo has no package.json, so a bare `npm install` walks up and installs
# into the parent directory (or $HOME). --prefix keeps it repo-local and
# throwaway. The rule list is read from the markdownlint that cli2 itself
# depends on, so the two always agree on which rules exist.
MIN_AGREEMENT ?=
PARITY_DIR := target/parity
PARITY_MARKDOWNLINT := $(PARITY_DIR)/node_modules/.bin/markdownlint-cli2
parity: build
	@mkdir -p $(PARITY_DIR)
	@test -x $(PARITY_MARKDOWNLINT) \
		|| npm install --no-save --no-audit --no-fund --prefix $(PARITY_DIR) markdownlint-cli2
	@python3 scripts/parity.py \
		--rumdl target/release/rumdl \
		--markdownlint $(PARITY_MARKDOWNLINT) \
		$(if $(MIN_AGREEMENT),--min-agreement $(MIN_AGREEMENT),) \
		$(ARGS)

# Audit dependencies for known vulnerabilities (RUSTSEC advisories). Reads
# Cargo.lock only; installs cargo-audit if missing.
audit:
	@command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit --locked
	cargo audit

# Verify the crate still builds on its declared MSRV (Cargo.toml `rust-version`).
# Uses a clippy/check against the pinned toolchain so a feature stabilized after
# the MSRV floor is caught instead of silently passing CI on a newer toolchain.
msrv-check:
	@msrv=$$(awk -F '"' '/^rust-version/ {print $$2; exit}' Cargo.toml); \
	echo "Checking build on MSRV $$msrv..."; \
	rustup toolchain install "$$msrv" --profile minimal --no-self-update 2>/dev/null || true; \
	cargo "+$$msrv" check --all-features --workspace

# Smoke-test the WASI CLI build under wasmtime: build for wasm32-wasip1-threads
# and actually run it on a sample file. `build-wasi` only compiles; this
# exercises the WASI-specific fallbacks at runtime (process-id counter, real FS,
# stdio) so a regression in them is caught, not just a type error.
smoke-wasi: build-wasi
	@command -v wasmtime >/dev/null 2>&1 || { echo "wasmtime not found; install it to run the WASI smoke test"; exit 1; }
	@printf '# Title\n\nsome text   \n' > /tmp/rumdl-wasi-smoke.md
	@echo "Running the WASI CLI under wasmtime..."
	@out=$$(wasmtime run -W threads=y,shared-memory=y -S threads=y --dir /tmp \
		target/wasm32-wasip1-threads/debug/rumdl.wasm check --no-cache /tmp/rumdl-wasi-smoke.md 2>&1); \
	code=$$?; \
	echo "$$out"; \
	echo "$$out" | grep -q 'MD009' || { echo "WASI smoke FAILED: expected MD009 in output"; exit 1; }; \
	test $$code -eq 1 || { echo "WASI smoke FAILED: expected exit 1 (violation found), got $$code"; exit 1; }
	@echo "WASI smoke OK: the wasm CLI linted a file, found the trailing-space violation, and exited 1."

clean:
	cargo clean

fmt:
	cargo fmt
	cargo clippy --fix --allow-dirty --allow-staged -- -D clippy::uninlined_format_args
	cargo fix --allow-dirty --allow-staged

# Verify Rust sources are rustfmt-clean (no changes applied). Run in CI so an
# unformatted contribution fails the gate instead of slipping in.
fmt-check:
	cargo fmt --check

lint-actions:
	actionlint
	uvx zizmor --min-severity=medium .github/workflows/

lint-actions-all:
	actionlint
	uvx zizmor .github/workflows/

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

# Sync the built-in code-block-tools table in docs/ from the registry.
sync-code-block-tools:
	cargo run --bin rumdl -- code-block-tools-docs generate

# Verify the built-in code-block-tools docs table is in sync with the registry.
check-code-block-tools:
	cargo run --bin rumdl -- code-block-tools-docs check

# Run the built-in code-block tools through rumdl against the real binaries. Each test
# skips when its tool is absent, so this verifies whatever is installed; install more
# tools to widen coverage (the CI code-block-tools job installs the fast ones).
test-code-block-tools:
	cargo nextest run --profile ci -E 'test(code_block_tools_execution)'

# Verify version references in vership-tracked files are in sync with Cargo.toml.
# Guards against vership's text-mode version_files silently no-op'ing when the
# {prev} pattern drifts out of the file.
check-versions:
	python3 scripts/check-versions.py

# Sync rule-count sentinels in the README and docs/ from the rule registry.
sync-rule-docs:
	python3 scripts/check-rule-docs.py --write

# Verify rule-count claims in docs match the registry and docs/rules.md
# lists every rule. Guards against doc drift when rules are added/removed.
# Runs the guard's own regression suite first: the guard is the thing that
# prevents drift, so its logic must be verified wherever it runs.
check-rule-docs:
	python3 scripts/test_check_rule_docs.py
	python3 scripts/check-rule-docs.py

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

# Assert the `docs/` tree is fmt-clean with the current rumdl code.
# Catches cases where someone hand-edits a docs file without running fmt,
# or where rumdl itself would want to rewrite a committed doc file.
docs-check:
	@echo "Checking docs/ is fmt-clean..."
	cargo run --quiet --release --bin rumdl -- fmt --check docs/

# Smoke-test the built documentation site (site/) for structural invariants
# that would have caught the #583 grid-cards mangling. Runs after `zensical
# build` and before deploy.
docs-smoke:
	@test -d site || { echo "site/ not found; run 'zensical build' first"; exit 1; }
	python3 scripts/docs_smoke_test.py site

release-patch:
	vership bump patch

release-minor:
	vership bump minor

release-major:
	vership bump major
