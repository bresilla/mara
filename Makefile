SHELL := /bin/bash

PROJECT_NAME := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | head -1 | tr -d '[:space:]')
PROJECT_VERSION := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | sed -n '2p' | tr -d '[:space:]')
ifeq ($(PROJECT_NAME),)
    $(error Error: PROJECT file not found or invalid)
endif

TOP_DIR := $(CURDIR)
CARGO := cargo
# Native windowing backend used by the root winit-owned example.
BACKEND ?= wayland
DISPLAY ?= :1
APP_BIN ?= native
APP_PKG ?= mara_example
APP_TARGET := -p $(APP_PKG) --bin $(APP_BIN)
TARGET ?= native
RUN_WITH ?= nixVulkan
TYPE ?= patch
HAS_REL := $(shell command -v git-rel 2>/dev/null)

$(info ------------------------------------------)
$(info Project: $(PROJECT_NAME) v$(PROJECT_VERSION))
$(info Display: $(BACKEND) backend)
$(info ------------------------------------------)

.PHONY: build b compile c run r serve test t test-all check harden bench clean docs release help h

build:
	@if [ "$(TARGET)" = "native" ]; then \
		$(CARGO) build $(APP_TARGET); \
	elif [ "$(TARGET)" = "web" ]; then \
		cd $(WEB_DIR) && env -u NO_COLOR trunk build --release; \
	elif [ "$(TARGET)" = "apk" ]; then \
		echo "TARGET=apk is reserved for Android builds; not implemented yet."; \
		exit 1; \
	else \
		echo "Unknown TARGET=$(TARGET). Use TARGET=native, TARGET=web, or TARGET=apk."; \
		exit 2; \
	fi

b: build

compile:
	@$(CARGO) clean
	@$(MAKE) build

c: compile

run:
	@WINIT_UNIX_BACKEND=$(BACKEND) $(RUN_WITH) $(CARGO) run $(APP_TARGET)

WEB_DIR := example

serve:
	@$(MAKE) build TARGET=web
	@cd $(WEB_DIR) && trunk serve --open

r: run

test:
	@$(CARGO) test $(APP_TARGET)

t: test

test-all:
	@$(CARGO) test --workspace --all-targets

check:
	@$(CARGO) check --workspace --all-targets

harden:
	@git diff --check
	@$(CARGO) fmt --all -- --check
	@$(CARGO) check --workspace --no-default-features
	@$(CARGO) test --workspace --no-default-features
	@$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings
	@$(CARGO) test --workspace --all-targets --all-features

bench:
	@$(CARGO) bench

docs:
	@command -v mdbook >/dev/null 2>&1 || { echo "mdbook is not installed. Please install it first."; exit 1; }
	@mdbook build $(TOP_DIR)/book --dest-dir $(TOP_DIR)/docs
	@git add --all && git commit -m "docs: building website/mdbook"

release:
	@if [ -z "$(HAS_REL)" ]; then \
		echo "git-rel is not installed. Please install it first."; \
		exit 1; \
	fi
	@if [ -z "$(TYPE)" ]; then \
		echo "Release type not specified. Use 'make release TYPE=[patch|minor|major|m.m.p]'"; \
		exit 1; \
	fi
	@git rel $(TYPE)

clean:
	@$(CARGO) clean

help:
	@echo
	@echo "Usage: make [target]"
	@echo
	@echo "Available targets:"
	@echo "  build        Build TARGET=native (default), TARGET=web, or TARGET=apk"
	@echo "  compile      Clean and rebuild"
	@echo "  run          Run the root $(APP_PKG) $(APP_BIN) app ($(BACKEND) backend, $(RUN_WITH) wrapper)"
	@echo "  serve        Build TARGET=web, then serve the root example UI in a browser"
	@echo "  test         Test the same app target as build/run ($(APP_PKG) bin $(APP_BIN))"
	@echo "  test-all     Run the full workspace all-target test suite"
	@echo "  check        Check the full workspace all-target suite"
	@echo "  harden       Run diff whitespace check + fmt/check + strict clippy + all-feature tests"
	@echo "  bench        Run benchmarks"
	@echo "  docs         Build documentation with mdbook"
	@echo "  release      Create a new release (TYPE=patch|minor|major|m.m.p)"
	@echo "  clean        Remove Cargo build artifacts"
	@echo
	@echo "Examples:"
	@echo "  make run"
	@echo "  make build TARGET=native"
	@echo "  make build TARGET=web"
	@echo "  make build TARGET=apk        # reserved for Android"
	@echo "  make serve"
	@echo "  make run APP_BIN=native       # run a different root example binary"
	@echo "  make run BACKEND=x11          # force X11 / XWayland (.envrc auto-detects)"
	@echo "  make run BACKEND=wayland      # force native Wayland"
	@echo "  make run DISPLAY=:0           # target a different X server (BACKEND=x11)"
	@echo "  make run RUN_WITH=nixGL       # OpenGL wrapper instead of Vulkan"
	@echo "  make run RUN_WITH=            # no wrapper (native run)"
	@echo

h: help
