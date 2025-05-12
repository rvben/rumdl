.PHONY: build test clean fmt check doc version-major version-minor version-patch build-python build-wheel dev-install

build:
	cargo build --release

test:
	cargo test

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

version-major:
	@echo "Creating new major version tag..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval NEW_MAJOR := $(shell echo $$(( $(MAJOR) + 1 ))))
	$(eval NEW_TAG := v$(NEW_MAJOR).0.0)
	$(eval VERSION_NO_V := $(NEW_MAJOR).0.0)
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@$(MAKE) update-cargo-version VERSION_NO_V=$(VERSION_NO_V)
	@$(MAKE) update-readme-version NEW_TAG=$(NEW_TAG)
	@git add Cargo.toml Cargo.lock README.md
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
	@$(MAKE) update-readme-version NEW_TAG=$(NEW_TAG)
	@git add Cargo.toml Cargo.lock README.md
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
	@$(MAKE) update-readme-version NEW_TAG=$(NEW_TAG)
	@git add Cargo.toml Cargo.lock README.md
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
