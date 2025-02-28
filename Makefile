.PHONY: build test clean fmt check doc version-major version-minor version-patch

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

# Version tagging targets
version-get:
	@echo "Current version: $$(git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0)"

version-major:
	@echo "Creating new major version tag..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval NEW_MAJOR := $(shell echo $$(( $(MAJOR) + 1 ))))
	$(eval NEW_TAG := v$(NEW_MAJOR).0.0)
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Tag $(NEW_TAG) created. Run 'git push origin $(NEW_TAG)' to trigger release workflow."

version-minor:
	@echo "Creating new minor version tag..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval MINOR := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.([0-9]+)\.[0-9]+/\1/'))
	$(eval NEW_MINOR := $(shell echo $$(( $(MINOR) + 1 ))))
	$(eval NEW_TAG := v$(MAJOR).$(NEW_MINOR).0)
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Tag $(NEW_TAG) created. Run 'git push origin $(NEW_TAG)' to trigger release workflow."

version-patch:
	@echo "Creating new patch version tag..."
	$(eval CURRENT := $(shell git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0))
	$(eval MAJOR := $(shell echo $(CURRENT) | sed -E 's/v([0-9]+)\.[0-9]+\.[0-9]+/\1/'))
	$(eval MINOR := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.([0-9]+)\.[0-9]+/\1/'))
	$(eval PATCH := $(shell echo $(CURRENT) | sed -E 's/v[0-9]+\.[0-9]+\.([0-9]+)/\1/'))
	$(eval NEW_PATCH := $(shell echo $$(( $(PATCH) + 1 ))))
	$(eval NEW_TAG := v$(MAJOR).$(MINOR).$(NEW_PATCH))
	@echo "Current: $(CURRENT) -> New: $(NEW_TAG)"
	@git tag -a $(NEW_TAG) -m "Release $(NEW_TAG)"
	@echo "Tag $(NEW_TAG) created. Run 'git push origin $(NEW_TAG)' to trigger release workflow."

# Target to push the new tag automatically
version-push:
	$(eval LATEST_TAG := $(shell git describe --tags --abbrev=0))
	@echo "Pushing tag $(LATEST_TAG) to origin..."
	@git push origin $(LATEST_TAG)
	@echo "Release workflow triggered for $(LATEST_TAG)"

# Combined targets for one-step release
release-major: version-major version-push
release-minor: version-minor version-push
release-patch: version-patch version-push 