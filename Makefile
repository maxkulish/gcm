# gcm Makefile
# AI git commit tool - a pure-Rust CLI. Build, test, and package the `gcm` binary.

BIN     := gcm
VERSION := $(shell grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')
HOST    := $(shell rustc -vV | sed -n 's/host: //p')
PREFIX  ?= $(HOME)/.local
DISTDIR := dist

# Release targets shipped by .github/workflows/release.yml - keep this list in sync.
TARGETS := \
	aarch64-apple-darwin \
	x86_64-apple-darwin \
	aarch64-unknown-linux-musl \
	x86_64-unknown-linux-musl

.PHONY: help build release run install uninstall test test-verbose \
        lint fmt fmt-check check cover cover-open clean version \
        dist dist-all _package \
        release-patch release-minor release-major _bump-and-tag pi-init

# Default target
help:
	@echo "gcm - Makefile"
	@echo ""
	@echo "Build:"
	@echo "  make build         - Debug build (./target/debug/$(BIN))"
	@echo "  make release       - Optimized release build (./target/release/$(BIN))"
	@echo "  make run ARGS=...  - Run the debug binary (e.g. make run ARGS=status)"
	@echo "  make install       - Install the release binary to $(PREFIX)/bin"
	@echo "  make uninstall     - Remove it from $(PREFIX)/bin"
	@echo ""
	@echo "Package (mirrors CI .github/workflows/release.yml):"
	@echo "  make dist          - Package the host release build ($(HOST)) into $(DISTDIR)/"
	@echo "  make dist-all      - Cross-build + package all release targets (needs cargo-zigbuild)"
	@echo ""
	@echo "Release (bump Cargo.toml + commit + tag; push to trigger CI build):"
	@echo "  make release-patch - $(VERSION) -> next patch, commit, tag vX.Y.(Z+1)"
	@echo "  make release-minor - $(VERSION) -> next minor, commit, tag vX.(Y+1).0"
	@echo "  make release-major - $(VERSION) -> next major, commit, tag v(X+1).0.0"
	@echo ""
	@echo "Testing:"
	@echo "  make test          - Run all tests"
	@echo "  make test-verbose  - Run tests with output (--nocapture)"
	@echo "  make cover         - Coverage report (terminal + HTML)"
	@echo "  make cover-open    - Coverage report, then open the HTML"
	@echo ""
	@echo "Code quality:"
	@echo "  make lint          - clippy (-D warnings)"
	@echo "  make fmt           - rustfmt (write)"
	@echo "  make fmt-check     - rustfmt (check only)"
	@echo "  make check         - fmt-check + lint + test"
	@echo ""
	@echo "Misc:"
	@echo "  make version       - Print the version the built binary reports"
	@echo "  make clean         - cargo clean + remove $(DISTDIR)/"
	@echo "  make pi-init       - Install npm deps for all .pi/extensions/*"
	@echo ""

# ── build ────────────────────────────────────────────────────────────────────

# Debug build. build.rs stamps the git short SHA into `gcm --version`.
build:
	cargo build

# Optimized release build for the host platform. --locked: build exactly the
# committed Cargo.lock (matches CI).
release:
	cargo build --release --locked

# Run the debug binary, e.g. `make run ARGS="status --json"`.
run:
	cargo run -- $(ARGS)

# Install the release binary onto PATH (override location with PREFIX=...).
install: release
	@mkdir -p "$(PREFIX)/bin"
	install -m 0755 target/release/$(BIN) "$(PREFIX)/bin/$(BIN)"
	@echo "Installed $(BIN) -> $(PREFIX)/bin/$(BIN)"

uninstall:
	rm -f "$(PREFIX)/bin/$(BIN)"
	@echo "Removed $(PREFIX)/bin/$(BIN)"

# ── test & quality ─────────────────────────────────────────────────────────────

test:
	cargo test

test-verbose:
	cargo test -- --nocapture

# Coverage (requires: cargo install cargo-llvm-cov)
cover:
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { \
		echo "Error: cargo-llvm-cov is not installed."; \
		echo "Install it with: cargo install cargo-llvm-cov"; \
		exit 1; \
	}
	cargo llvm-cov --html --ignore-filename-regex '(/tests/|target/)'
	@echo ""
	cargo llvm-cov report

cover-open: cover
	open target/llvm-cov/html/index.html

lint:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

# Everything CI gates on, in one shot (useful before commit).
check: fmt-check lint test
	@echo "All checks passed!"

version: release
	@./target/release/$(BIN) --version

clean:
	cargo clean
	rm -rf $(DISTDIR)

# ── package / distribute ───────────────────────────────────────────────────────

# Package one target's release binary into
# $(DISTDIR)/gcm-v<version>-<target>.tar.gz (+ .sha256), bundling LICENSE -
# the same artifact layout as the release.yml "Package" step.
# Internal helper: make _package TARGET=<triple> BINPATH=<path-to-binary>
_package:
	@mkdir -p "$(DISTDIR)"
	@DIST="$(BIN)-v$(VERSION)-$(TARGET)"; \
	STAGE="$$(mktemp -d)"; \
	cp "$(BINPATH)" "$$STAGE/"; \
	cp LICENSE "$$STAGE/"; \
	tar czf "$(DISTDIR)/$$DIST.tar.gz" -C "$$STAGE" .; \
	( cd "$(DISTDIR)" && { command -v sha256sum >/dev/null 2>&1 && sha256sum "$$DIST.tar.gz" || shasum -a 256 "$$DIST.tar.gz"; } > "$$DIST.tar.gz.sha256" ); \
	rm -rf "$$STAGE"; \
	echo "Packaged $(DISTDIR)/$$DIST.tar.gz"

# Package the host release build (single platform, no cross toolchain needed).
dist: release
	@$(MAKE) --no-print-directory _package TARGET=$(HOST) BINPATH=target/release/$(BIN)

# Build + package every release target. Cross-compiling Linux musl from macOS
# needs a cross linker, so this uses cargo-zigbuild. The canonical multi-platform
# artifacts are produced by CI (.github/workflows/release.yml on a `v*` tag);
# this target is for local rehearsal.
dist-all:
	@command -v cargo-zigbuild >/dev/null 2>&1 || { \
		echo "Error: cargo-zigbuild is required to cross-build all targets locally."; \
		echo "  Install: cargo install cargo-zigbuild && pip install ziglang"; \
		echo "  (Or just push a 'v*' tag - CI builds all targets via release.yml.)"; \
		exit 1; \
	}
	@for t in $(TARGETS); do \
		echo "==> building $$t"; \
		rustup target add "$$t" >/dev/null 2>&1 || true; \
		cargo zigbuild --release --locked --target "$$t" || exit 1; \
		$(MAKE) --no-print-directory _package TARGET="$$t" BINPATH="target/$$t/release/$(BIN)"; \
	done
	@echo "All artifacts in $(DISTDIR)/"

# ── release (version bump + tag) ───────────────────────────────────────────────

# Bump Cargo.toml, sync Cargo.lock, commit, and create an annotated `vX.Y.Z` tag.
# Building + publishing the binaries is CI's job: pushing the tag triggers
# .github/workflows/release.yml (which asserts the tag matches Cargo.toml). This
# target stops before the push so the release is your explicit, reviewable action.
release-patch:
	@$(MAKE) --no-print-directory _bump-and-tag BUMP=patch

release-minor:
	@$(MAKE) --no-print-directory _bump-and-tag BUMP=minor

release-major:
	@$(MAKE) --no-print-directory _bump-and-tag BUMP=major

_bump-and-tag:
	@if [ -n "$$(git status --porcelain)" ]; then \
		echo "Error: working tree is dirty - commit or stash before cutting a release."; \
		exit 1; \
	fi
	@NEW=$$(echo "$(VERSION)" | awk -F. -v b="$(BUMP)" '{ \
		if (b=="major") {$$1++; $$2=0; $$3=0} \
		else if (b=="minor") {$$2++; $$3=0} \
		else {$$3++} \
		print $$1"."$$2"."$$3 }'); \
	echo "Bumping $(VERSION) -> $$NEW ($(BUMP))"; \
	awk -v v="$$NEW" 'BEGIN{d=0} /^version = "/ && !d {sub(/"[^"]*"/, "\"" v "\""); d=1} {print}' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml; \
	cargo check --quiet;  \
	git add Cargo.toml Cargo.lock; \
	git commit -q -m "release: v$$NEW"; \
	git tag -a "v$$NEW" -m "v$$NEW"; \
	echo ""; \
	echo "Created commit + annotated tag v$$NEW."; \
	echo "Push to trigger the release build (.github/workflows/release.yml):"; \
	echo "    git push --follow-tags"

# ── pi ─────────────────────────────────────────────────────────────────────────

# Install npm deps for every .pi/extensions/* (node_modules is gitignored).
pi-init:
	@for ext in .pi/extensions/*/package.json; do \
		[ -f "$$ext" ] || continue; \
		dir=$$(dirname "$$ext"); \
		echo "Installing deps in $$dir..."; \
		if [ -f "$$dir/package-lock.json" ]; then \
			(cd "$$dir" && npm ci --silent); \
		else \
			(cd "$$dir" && npm install --silent); \
		fi; \
	done
	@echo "Pi extensions ready"
