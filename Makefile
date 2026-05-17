SHELL := /bin/bash

PROJECT_NAME := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | head -1 | tr -d '[:space:]')
PROJECT_VERSION := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | sed -n '2p' | tr -d '[:space:]')
ifeq ($(PROJECT_NAME),)
    $(error Error: PROJECT file not found or invalid)
endif

TOP_DIR := $(CURDIR)
CARGO := cargo
BACKEND ?= x11
DISPLAY ?= :1
EXAMPLE ?= demo
APP_PKG ?= bevy_mara
APP_TARGET := -p $(APP_PKG) --example $(EXAMPLE)
RUN_WITH ?= nixVulkan

$(info ------------------------------------------)
$(info Project: $(PROJECT_NAME) v$(PROJECT_VERSION))
$(info Display: $(BACKEND) backend)
$(info ------------------------------------------)

.PHONY: build b compile c run r serve-web build-web test t test-all check check-all harden bench clean help h

build:
	@$(CARGO) build $(APP_TARGET)

b: build

compile:
	@$(CARGO) clean
	@$(MAKE) build

c: compile

run:
	@$(RUN_WITH) $(CARGO) run $(APP_TARGET)

run-newui:
	@$(RUN_WITH) $(CARGO) run -p $(APP_PKG) --example newui

run-egui:
	@DISPLAY=$(DISPLAY) $(RUN_WITH) $(CARGO) run -p egui_mara --example egui_demo

WEB_DIR := api_crates/web

serve-web:
	@cd $(WEB_DIR) && trunk serve --open

build-web:
	@cd $(WEB_DIR) && trunk build --release

r: run

test:
	@$(CARGO) test $(APP_TARGET)

t: test

test-all:
	@$(CARGO) test --workspace --all-targets

check:
	@$(CARGO) check $(APP_TARGET)

check-all:
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

clean:
	@$(CARGO) clean

help:
	@echo
	@echo "Usage: make [target]"
	@echo
	@echo "Available targets:"
	@echo "  build        Build the $(EXAMPLE) example"
	@echo "  compile      Clean and rebuild"
	@echo "  run          Run the $(EXAMPLE) example ($(BACKEND) backend, $(RUN_WITH) wrapper)"
	@echo "  serve-web    Serve the egui_mara UI in a browser (trunk, wasm32)"
	@echo "  build-web    Build the wasm bundle to api_crates/web/dist"
	@echo "  test         Test the same app target as build/run ($(APP_PKG) example $(EXAMPLE))"
	@echo "  test-all     Run the full workspace all-target test suite"
	@echo "  check        Check the same app target as build/run ($(APP_PKG) example $(EXAMPLE))"
	@echo "  check-all    Check the full workspace all-target suite"
	@echo "  harden       Run diff whitespace check + fmt/check + strict clippy + all-feature tests"
	@echo "  bench        Run benchmarks"
	@echo "  clean        Remove Cargo build artifacts"
	@echo
	@echo "Examples:"
	@echo "  make run"
	@echo "  make run EXAMPLE=other        # run a different example"
	@echo "  make run BACKEND=x11          # force X11 / XWayland (.envrc auto-detects)"
	@echo "  make run BACKEND=wayland      # force native Wayland"
	@echo "  make run DISPLAY=:0           # target a different X server (BACKEND=x11)"
	@echo "  make run RUN_WITH=nixGL       # OpenGL wrapper instead of Vulkan"
	@echo "  make run RUN_WITH=            # no wrapper (native run)"
	@echo

h: help
