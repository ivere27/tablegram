#!/usr/bin/make -f
#
# tablegram Makefile
# WebAssembly + browser demo helpers
#
# Usage:
#   make            Build wasm package (docker)
#   make build      Build wasm package (docker)
#   make web        Build wasm and serve web demo on $(WEB_PORT) (local by default)
#                    Set WASM_BUILD_MODE=server for Docker/container build
#   make release    Build Rust crate and npm package artifacts
#   make publish_github Create/update GitHub release and upload release artifacts
#   make publish_npm Publish staged npm package
#   make docker-image Rebuild docker image used by docker build target
#   make clean      Remove generated wasm artifacts
#   make help       Print this help

# =============================================================================
# Variables
# =============================================================================
DOCKERFILE ?= docker/Dockerfile.web
DOCKER_IMAGE ?= tablegram-wasm

HOST_CPU_COUNT ?= $(shell nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)
HOST_UID ?= $(shell id -u 2>/dev/null || echo 1000)
HOST_GID ?= $(shell id -g 2>/dev/null || echo 1000)
BUILD_JOBS_DEFAULT := $(shell c=$$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1); \
	if [ "$$c" -gt 2 ] 2>/dev/null; then echo $$((c-2)); else echo 1; fi)
BUILD_JOBS ?= $(BUILD_JOBS_DEFAULT)
WASM_BUILD_MODE ?= local
SAMPLES_SRC_DIR ?= corpus/generated
SAMPLES_PUBLIC_DIR ?= web/public/samples
SAMPLES_MANIFEST := web/public/samples/manifest.json

WEB_HOST ?= 127.0.0.1
WEB_PORT ?= 4173
ROOT_DIR := $(patsubst %/,%,$(abspath $(dir $(lastword $(MAKEFILE_LIST)))))
TABLEGRAM_VERSION ?= $(shell sed -n 's/^version = "\(.*\)"/\1/p' "$(ROOT_DIR)/Cargo.toml" | head -n 1)
RELEASE_TAG ?= v$(TABLEGRAM_VERSION)
GIT_REMOTE ?= origin
GITHUB_REPO ?= ivere27/tablegram
NPM_PACKAGE_NAME ?= tablegram
NPM_PUBLISH_FLAGS ?=
NPM_STAGE_DIR := dist/npm/$(NPM_PACKAGE_NAME)
NPM_TARBALL := dist/npm/$(NPM_PACKAGE_NAME)-$(TABLEGRAM_VERSION).tgz
CRATE_TARBALL := target/package/tablegram-$(TABLEGRAM_VERSION).crate
ALLOW_DIRTY_RELEASE ?= 0

WASM_JS := web/pkg/tablegram_wasm.js
RUST_WEB_SERVER ?= cargo run --manifest-path "$(ROOT_DIR)/Cargo.toml" --bin tablegram-web-server -- --root "$(ROOT_DIR)/web" --host "$(WEB_HOST)" --port "$(WEB_PORT)"

.PHONY: all build web build-local web-local web-server sync-web-samples serve-local docker-image release-check-clean release-check-tag release publish_github publish_npm clean help

all: build

build: docker-image
	@echo "Building wasm (jobs=$(BUILD_JOBS), host_cpus=$(HOST_CPU_COUNT))..."
	docker run --rm \
		-e ADO_DOCKER_TARGET=wasm \
		-e CPU_COUNT=$(HOST_CPU_COUNT) \
		-e HOST_UID=$(HOST_UID) \
		-e HOST_GID=$(HOST_GID) \
		-e WASM_BUILD_JOBS=$(BUILD_JOBS) \
		-v "$(ROOT_DIR):/workspace/tablegram" \
		"$(DOCKER_IMAGE)"

build-local:
	@command -v rustup >/dev/null 2>&1 || { echo "Error: rustup not found."; exit 1; }
	@command -v wasm-pack >/dev/null 2>&1 || { echo "Error: wasm-pack not found. Install with: cargo install wasm-pack"; exit 1; }
	@echo "Building wasm locally (jobs=$(BUILD_JOBS), host_cpus=$(HOST_CPU_COUNT))..."
	cd wasm_bindings && \
		CARGO_BUILD_JOBS="$(BUILD_JOBS)" \
		rustup run nightly wasm-pack build . \
			--target web \
			--out-dir ../web/pkg \
			--out-name tablegram_wasm \
			--release \
			--no-opt \
			-- --jobs "$(BUILD_JOBS)"

serve-local:
	@echo "Starting local browser demo at http://127.0.0.1:$(WEB_PORT)/ ..."
	$(RUST_WEB_SERVER)

sync-web-samples:
	@rm -rf "$(SAMPLES_PUBLIC_DIR)"
	@mkdir -p "$(SAMPLES_PUBLIC_DIR)"
	@rm -f "$(SAMPLES_MANIFEST)"
	@{ \
	  echo "["; \
	  first=1; \
	  for src in $(SAMPLES_SRC_DIR)/*.adtg $(SAMPLES_SRC_DIR)/*.xml; do \
	    if [ ! -f "$$src" ]; then \
	      continue; \
	    fi; \
	    base=$$(basename "$$src"); \
	    cp "$$src" "$(SAMPLES_PUBLIC_DIR)/$$base"; \
	    if [ "$$first" -ne 1 ]; then \
	      echo ","; \
	    fi; \
	    first=0; \
	    ext=$${base##*.}; \
	    label=$${base%.*}; \
	    printf '  {"name":"%s","label":"%s","format":"%s","path":"/public/samples/%s"}' "$$base" "$$label" "$$ext" "$$base"; \
		  done; \
		  echo ""; \
		  echo "]"; \
		} > "$(SAMPLES_MANIFEST)"

ifeq ($(WASM_BUILD_MODE),server)
web: web-server
else
web: web-local
endif

docker-image:
	@echo "Building docker image $(DOCKER_IMAGE) ..."
	docker build -f "$(DOCKERFILE)" -t "$(DOCKER_IMAGE)" .

web-local: sync-web-samples build-local serve-local

web-server: sync-web-samples docker-image
	@echo "Starting browser demo at http://127.0.0.1:$(WEB_PORT)/ using server build (jobs=$(BUILD_JOBS))..."
	docker run --rm -p "$(WEB_PORT):$(WEB_PORT)" \
		-e ADO_DOCKER_TARGET=web \
		-e CPU_COUNT=$(HOST_CPU_COUNT) \
		-e HOST_UID=$(HOST_UID) \
		-e HOST_GID=$(HOST_GID) \
		-e WASM_BUILD_JOBS=$(BUILD_JOBS) \
		-e WEB_PORT=$(WEB_PORT) \
		-v "$(ROOT_DIR):/workspace/tablegram" \
		"$(DOCKER_IMAGE)"

release: build
	@echo "Packaging Rust crate $(TABLEGRAM_VERSION)..."
	cargo package --quiet
	@test -f "$(CRATE_TARBALL)" || { echo "Error: crate package not found: $(CRATE_TARBALL)"; exit 1; }
	@echo "Rust crate ready: $(CRATE_TARBALL)"
	@command -v npm >/dev/null 2>&1 || { echo "Error: npm not found in PATH."; exit 1; }
	@command -v node >/dev/null 2>&1 || { echo "Error: node not found in PATH."; exit 1; }
	@echo "Staging npm package $(NPM_PACKAGE_NAME)@$(TABLEGRAM_VERSION)..."
	@rm -rf "$(NPM_STAGE_DIR)"
	@mkdir -p "$(NPM_STAGE_DIR)"
	@cp web/pkg/tablegram_wasm.js web/pkg/tablegram_wasm_bg.wasm "$(NPM_STAGE_DIR)/"
	@if [ -f web/pkg/tablegram_wasm.d.ts ]; then cp web/pkg/tablegram_wasm.d.ts "$(NPM_STAGE_DIR)/"; fi
	@if [ -f web/pkg/tablegram_wasm_bg.wasm.d.ts ]; then cp web/pkg/tablegram_wasm_bg.wasm.d.ts "$(NPM_STAGE_DIR)/"; fi
	@if [ -d web/pkg/snippets ]; then cp -a web/pkg/snippets "$(NPM_STAGE_DIR)/"; fi
	@cp README.md LICENSE "$(NPM_STAGE_DIR)/"
	@NPM_PACKAGE_NAME="$(NPM_PACKAGE_NAME)" \
		TABLEGRAM_VERSION="$(TABLEGRAM_VERSION)" \
		NPM_STAGE_DIR="$(NPM_STAGE_DIR)" \
		node -e 'const fs = require("fs"); const pkg = { name: process.env.NPM_PACKAGE_NAME, type: "module", version: process.env.TABLEGRAM_VERSION, description: "ADO Recordset persistence parser and writer", license: "Apache-2.0", repository: { type: "git", url: "git+https://github.com/ivere27/tablegram.git" }, homepage: "https://github.com/ivere27/tablegram", bugs: { url: "https://github.com/ivere27/tablegram/issues" }, files: ["tablegram_wasm_bg.wasm", "tablegram_wasm.js", "tablegram_wasm.d.ts", "tablegram_wasm_bg.wasm.d.ts", "snippets/**/*"], main: "tablegram_wasm.js", types: "tablegram_wasm.d.ts", exports: { ".": { types: "./tablegram_wasm.d.ts", import: "./tablegram_wasm.js", default: "./tablegram_wasm.js" } }, scripts: {} }; fs.writeFileSync(process.env.NPM_STAGE_DIR + "/package.json", JSON.stringify(pkg, null, 2) + "\n");'
	@rm -f "$(NPM_TARBALL)"
	@PACKED=$$(cd "$(NPM_STAGE_DIR)" && npm pack --pack-destination "$(ROOT_DIR)/dist/npm" --silent); \
		test -f "$(ROOT_DIR)/dist/npm/$$PACKED" || { echo "Error: npm pack did not create $$PACKED"; exit 1; }; \
		if [ "$(ROOT_DIR)/dist/npm/$$PACKED" != "$(ROOT_DIR)/$(NPM_TARBALL)" ]; then mv "$(ROOT_DIR)/dist/npm/$$PACKED" "$(NPM_TARBALL)"; fi
	@echo "npm package ready: $(NPM_TARBALL)"

release-check-clean:
	@if [ "$(ALLOW_DIRTY_RELEASE)" != "1" ]; then \
		git diff --quiet --ignore-submodules -- && git diff --cached --quiet --ignore-submodules -- || { \
			echo "Error: tracked changes are present. Commit first or run with ALLOW_DIRTY_RELEASE=1."; \
			exit 1; \
		}; \
	fi

release-check-tag:
	@TAG="$(RELEASE_TAG)"; \
	if ! git rev-parse -q --verify "refs/tags/$$TAG" >/dev/null; then \
		echo "Error: tag $$TAG not found. Create it with: git tag -a $$TAG -m $$TAG"; \
		exit 1; \
	fi; \
	if [ "$$(git rev-parse "$$TAG^{commit}")" != "$$(git rev-parse HEAD)" ]; then \
		echo "Error: tag $$TAG does not point to HEAD."; \
		exit 1; \
	fi; \
	if ! git ls-remote --exit-code --tags "$(GIT_REMOTE)" "refs/tags/$$TAG" >/dev/null 2>&1; then \
		echo "Error: tag $$TAG is not pushed to $(GIT_REMOTE). Run: git push $(GIT_REMOTE) $$TAG"; \
		exit 1; \
	fi

publish_github: release-check-clean release-check-tag release
	@command -v gh >/dev/null 2>&1 || { echo "Error: gh (GitHub CLI) not found in PATH."; exit 1; }
	@TAG="$(RELEASE_TAG)"; \
	echo "Creating/updating GitHub release $$TAG in $(GITHUB_REPO)..."; \
	gh release view "$$TAG" --repo "$(GITHUB_REPO)" >/dev/null 2>&1 || \
		gh release create "$$TAG" --repo "$(GITHUB_REPO)" --title "$$TAG" --notes "Release $$TAG" || exit 1; \
	gh release upload "$$TAG" "$(CRATE_TARBALL)" "$(NPM_TARBALL)" --repo "$(GITHUB_REPO)" --clobber

publish_npm: release-check-clean release-check-tag release
	@command -v npm >/dev/null 2>&1 || { echo "Error: npm not found in PATH."; exit 1; }
	@echo "Publishing $(NPM_PACKAGE_NAME)@$(TABLEGRAM_VERSION) to npm..."
	cd "$(NPM_STAGE_DIR)" && npm publish $(NPM_PUBLISH_FLAGS)

clean:
	@echo "Cleaning wasm artifacts..."
	rm -rf web/pkg web/public/samples dist/npm

help:
	@echo "tablegram Makefile"
	@echo ""
	@echo "  make            Build wasm package (docker)"
	@echo "  make build      Build wasm package (docker)"
	@echo "  make web        Build wasm and serve web demo (default: local)"
	@echo "  make web WASM_BUILD_MODE=server  Use server/container build for web"
	@echo "  make release    Build Rust crate and npm package artifacts"
	@echo "  make publish_github Upload $(RELEASE_TAG) artifacts to GitHub Releases"
	@echo "  make publish_npm Publish $(NPM_PACKAGE_NAME)@$(TABLEGRAM_VERSION) to npm"
	@echo "  make docker-image Rebuild docker image"
	@echo "  make clean      Remove web/pkg"
	@echo "  make help       Show this help"
	@echo ""
	@echo "Tuning:"
	@echo "  HOST_CPU_COUNT ?= $$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)"
	@echo "  BUILD_JOBS ?= $(BUILD_JOBS)"
	@echo "  WEB_HOST ?= 127.0.0.1"
	@echo "  WEB_PORT ?= 4173"
	@echo "  WASM_BUILD_MODE ?= local (server to force Docker/container)"
	@echo "  HOST_UID/HOST_GID ?= current user for Docker artifact ownership"
	@echo "  GITHUB_REPO ?= $(GITHUB_REPO)"
	@echo "  RELEASE_TAG ?= $(RELEASE_TAG)"
	@echo "  NPM_PACKAGE_NAME ?= $(NPM_PACKAGE_NAME)"
