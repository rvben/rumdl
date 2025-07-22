.PHONY: build test clean fmt check doc version-major version-minor version-patch build-python build-wheel dev-install setup-mise dev-setup dev-verify update-dependencies update-rust-version pre-release

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

test:
	cargo test

test-nextest:
	cargo nextest run

test-quick:
	cargo nextest run --profile quick

test-ci:
	cargo nextest run --profile ci

clean:
	cargo clean

fmt:
	cargo fmt
	cargo clippy --fix --allow-dirty --allow-staged
	cargo fix --allow-dirty --allow-staged

lint:
	cargo clippy --all-targets --all-features -- -D warnings

check:
	cargo check --all-targets --all-features

doc:
	cargo doc --no-deps

watch-test:
	cargo watch -x test

all: fmt check test build

# Python-specific targets
build-python:
	maturin build --release

build-wheel:
	maturin build --release --strip --interpreter python3

dev-install:
	maturin develop --release

# Version tagging targets
version-get:
	@echo "Current version: $$(git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0)"
	@echo "Cargo.toml version: $$(grep '^version' Cargo.toml | sed -E 's/version = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/')"

# Helper function to update Cargo.toml version
update-cargo-version:
	@echo "Updating Cargo.toml version to $(VERSION_NO_V)..."
	@sed -i.bak -E 's/^version = "[0-9]+\.[0-9]+\.[0-9]+"/version = "$(VERSION_NO_V)"/' Cargo.toml
	@rm -f Cargo.toml.bak
	@echo "Cargo.toml updated to version $(VERSION_NO_V)"
	@echo "Updating Cargo.lock..."
	@cargo update

update-readme-version:
	@echo "Updating README.md pre-commit rev to $(NEW_TAG)..."
	@perl -i.bak -0777 -pe 's{(repo: https://github.com/rvben/rumdl\s+rev: )v\d+\.\d+\.\d+}{$$1$(NEW_TAG)}g' README.md
	@rm -f README.md.bak
	@echo "README.md updated to rev $(NEW_TAG)"

update-all-docs-version:
	@echo "Updating all documentation to use $(NEW_TAG)..."
	@perl -i.bak -E 's/rev: v[0-9]+\.[0-9]+\.[0-9]+/rev: $(NEW_TAG)/g' README.md docs/global-settings.md ../rumdl-pre-commit/README.md 2>/dev/null || true
	@rm -f README.md.bak docs/global-settings.md.bak ../rumdl-pre-commit/README.md.bak 2>/dev/null || true
	@echo "All documentation updated to rev $(NEW_TAG)"

update-changelog:
	@echo "Updating CHANGELOG.md for $(NEW_TAG)..."
	@if [ -f CHANGELOG.md ]; then \
		DATE=$$(date +%Y-%m-%d); \
		perl -i.bak -pe 's/## \[Unreleased\]/## [Unreleased]\n\n## [$(VERSION_NO_V)] - '"$$DATE"'/' CHANGELOG.md; \
		perl -i.bak -0777 -pe 's/(\[Unreleased\]: .*\/compare\/)v[0-9]+\.[0-9]+\.[0-9]+(\.\.\.HEAD)/$$1$(NEW_TAG)$$2\n[$(VERSION_NO_V)]: https:\/\/github.com\/rvben\/rumdl\/compare\/$(CURRENT)...$(NEW_TAG)/' CHANGELOG.md; \
		rm -f CHANGELOG.md.bak; \
		echo "CHANGELOG.md updated for version $(NEW_TAG)"; \
	else \
		echo "Warning: CHANGELOG.md not found"; \
	fi

version-major:
	@echo "Creating new major version tag..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval NEW_MAJOR := $(shell echo $$(( $(MAJOR) + 1 ))))
	$(eval NEW_TAG := v$(NEW_MAJOR).0.0)
	$(eval VERSION_NO_V := $(NEW_MAJOR).0.0)
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@$(MAKE) update-cargo-version VERSION_NO_V=$(VERSION_NO_V)
	@$(MAKE) update-all-docs-version NEW_TAG=$(NEW_TAG)
	@$(MAKE) update-changelog NEW_TAG=$(NEW_TAG) VERSION_NO_V=$(VERSION_NO_V) CURRENT=$(CURRENT)
	@git add Cargo.toml Cargo.lock README.md docs/global-settings.md CHANGELOG.md
	@git commit -m "Bump version to $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Version $(NEW_TAG) created and committed. Run 'git push && git push origin $(NEW_TAG)' to trigger release workflow."

version-minor:
	@echo "Creating new minor version tag..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval MINOR := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.([0-9]+)\.[0-9]+/\1/'))
	$(eval NEW_MINOR := $(shell echo $$(( $(MINOR) + 1 ))))
	$(eval NEW_TAG := v$(MAJOR).$(NEW_MINOR).0)
	$(eval VERSION_NO_V := $(MAJOR).$(NEW_MINOR).0)
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@$(MAKE) update-cargo-version VERSION_NO_V=$(VERSION_NO_V)
	@$(MAKE) update-all-docs-version NEW_TAG=$(NEW_TAG)
	@$(MAKE) update-changelog NEW_TAG=$(NEW_TAG) VERSION_NO_V=$(VERSION_NO_V) CURRENT=$(CURRENT)
	@git add Cargo.toml Cargo.lock README.md docs/global-settings.md CHANGELOG.md
	@git commit -m "Bump version to $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Version $(NEW_TAG) created and committed. Run 'git push && git push origin $(NEW_TAG)' to trigger release workflow."

version-patch:
	@echo "Creating new patch version tag..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval MINOR := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.([0-9]+)\.[0-9]+/\1/'))
	$(eval PATCH := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.[0-9]+\.([0-9]+)/\1/'))
	$(eval NEW_PATCH := $(shell echo $$(( $(PATCH) + 1 ))))
	$(eval NEW_TAG := v$(MAJOR).$(MINOR).$(NEW_PATCH))
	$(eval VERSION_NO_V := $(MAJOR).$(MINOR).$(NEW_PATCH))
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@$(MAKE) update-cargo-version VERSION_NO_V=$(VERSION_NO_V)
	@$(MAKE) update-all-docs-version NEW_TAG=$(NEW_TAG)
	@$(MAKE) update-changelog NEW_TAG=$(NEW_TAG) VERSION_NO_V=$(VERSION_NO_V) CURRENT=$(CURRENT)
	@git add Cargo.toml Cargo.lock README.md docs/global-settings.md CHANGELOG.md
	@git commit -m "Bump version to $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Version $(NEW_TAG) created and committed. Run 'git push && git push origin $(NEW_TAG)' to trigger release workflow."

# Target to push the new tag and changes automatically
version-push:
	$(eval LATEST_TAG := $(shell git describe --tags --abbrev=0))
	@echo "Pushing latest commit and tag $(LATEST_TAG) to origin..."
	@git push
	@git push origin $(LATEST_TAG)
	@echo "Release workflow triggered for $(LATEST_TAG)"

# Combined targets for one-step release
release-major: version-major version-push
release-minor: version-minor version-push
release-patch: version-patch version-push

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

pre-release:
	@echo "Preparing for release..."
	@echo "===================="
	@$(MAKE) update-rust-version
	@echo ""
	@$(MAKE) update-dependencies
	@echo ""
	@echo "Running tests to verify everything works..."
	@$(MAKE) test-quick
	@echo ""
	@echo "Pre-release preparation complete!"
	@echo "===================="
	@echo ""
	@echo "Summary of changes:"
	@echo "- Rust version: $$(grep '^rust-version' Cargo.toml | sed -E 's/rust-version = "([0-9]+\.[0-9]+\.[0-9]+)"/\1/')"
	@echo "- Dependencies: Updated to latest compatible versions"
	@echo ""
	@echo "Please review changes and commit if satisfied."

# Full release targets that include pre-release preparation
release-major-full: pre-release version-major version-push
release-minor-full: pre-release version-minor version-push
release-patch-full: pre-release version-patch version-push
