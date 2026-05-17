SHELL := /bin/bash

PROJECT_NAME := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | head -1 | tr -d '[:space:]')
PROJECT_VERSION := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | sed -n '2p' | tr -d '[:space:]')
ifeq ($(PROJECT_NAME),)
    $(error Error: PROJECT file not found or invalid)
endif

TOP_DIR := $(CURDIR)
CARGO := cargo
XDG_RUNTIME_DIR ?= /run/user/$(shell id -u)
# Resolve a usable Wayland socket. $WAYLAND_DISPLAY is trusted ONLY when
# it names a live socket — over SSH / waypipe / nix shells it routinely
# leaks through pointing at nothing, which is exactly what makes winit
# panic with `NoCompositor`. Otherwise fall back to the conventional
# `wayland-0`. Override the socket with `make run WL_SOCKET=wayland-1`.
WL_SOCKET ?= $(shell [ -S "$(XDG_RUNTIME_DIR)/$$WAYLAND_DISPLAY" ] && printf '%s' "$$WAYLAND_DISPLAY" || printf wayland-0)
# Windowing backend winit talks to. Auto-detected: `wayland` when that
# socket actually exists, otherwise `x11` (covers plain Xorg and
# XWayland). Force it with `make run BACKEND=x11` (or `BACKEND=wayland`).
BACKEND ?= $(shell [ -S "$(XDG_RUNTIME_DIR)/$(WL_SOCKET)" ] && echo wayland || echo x11)
# DISPLAY pins which X server receives the window when BACKEND=x11
# (matches the XWayland / Nvidia GL display on multi-X setups). Override
# if you need `:0` or similar: `make run DISPLAY=:0`.
DISPLAY ?= :1
# Wrapper that forwards GPU/display access. `nixVulkan` = Bevy/wgpu path.
# Override with `make run RUN_WITH=nixGL` or `RUN_WITH=` for native.
RUN_WITH ?= nixVulkan
# Example binary that `make run` targets. Override with `EXAMPLE=other`.
EXAMPLE ?= demo
# Fast interactive loop target. Keep build/run/check/test pointed at the
# SAME package + example so `make check` doesn't drag the whole workspace
# and then make `run` feel like a cold build. Full-workspace gates live
# under `check-all` / `test-all` / `harden`.
APP_PKG ?= bevy_mara
APP_TARGET := -p $(APP_PKG) --example $(EXAMPLE)

# Per-backend launch env. X11: clear the Wayland vars so winit can't be
# lured onto an unreachable socket, pin $DISPLAY. Wayland: pin
# $WAYLAND_DISPLAY to the resolved socket, clear $WAYLAND_SOCKET (an fd
# handoff that would otherwise override it) and clear $DISPLAY so the
# GPU driver doesn't waste time probing — and warn about — a dead X server.
X11_ENV := env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET DISPLAY=$(DISPLAY)
ifeq ($(BACKEND),wayland)
RUN_ENV := env -u WAYLAND_SOCKET -u DISPLAY WAYLAND_DISPLAY=$(WL_SOCKET)
else
RUN_ENV := $(X11_ENV)
endif

# Native Wayland Vulkan present allocates buffers via GBM on a DRM render
# node (/dev/dri/renderD*). If that node isn't writable — typically the
# user is missing from the `render` group — the NVIDIA driver reports
# zero present modes and bevy panics deep in `create_surfaces`. Catch it
# up front with an actionable message instead of a core dump.
WL_PREFLIGHT = if [ "$(BACKEND)" = wayland ] && \
	[ -z "$$(find /dev/dri -maxdepth 1 -name 'renderD*' -readable -writable 2>/dev/null)" ]; then \
		echo "make: native Wayland needs a writable DRM render node (/dev/dri/renderD*)."; \
		echo "      you appear not to be in the 'render' group. fix with:"; \
		echo "        sudo usermod -aG render,video $$USER   (then log out and back in)"; \
		echo "      or run via XWayland instead:  make run BACKEND=x11"; \
		exit 1; \
	fi

$(info ------------------------------------------)
$(info Project: $(PROJECT_NAME) v$(PROJECT_VERSION))
$(info Display: $(BACKEND) backend$(if $(filter wayland,$(BACKEND)), ($(WL_SOCKET))))
$(info ------------------------------------------)

.PHONY: build b compile c run r smoke-gui serve-web build-web test t test-all check check-all fmt harden harden-gui bench clean help h

build:
	@$(CARGO) build $(APP_TARGET)

b: build

compile:
	@$(CARGO) clean
	@$(MAKE) build

c: compile

run:
	@$(WL_PREFLIGHT)
	@$(RUN_ENV) $(RUN_WITH) $(CARGO) run $(APP_TARGET)

smoke-gui:
	@set -euo pipefail; \
	log="$${TMPDIR:-/tmp}/bevy_mara_gui_smoke.log"; \
	rm -f "$$log"; \
	( $(X11_ENV) $(RUN_WITH) $(CARGO) run $(APP_TARGET) >"$$log" 2>&1 ) & \
	pid=$$!; \
	trap 'kill $$pid >/dev/null 2>&1 || true; wait $$pid >/dev/null 2>&1 || true' EXIT; \
	deadline=$$((SECONDS + 20)); \
	found=""; \
	while [ $$SECONDS -lt $$deadline ]; do \
		if $(X11_ENV) xwininfo -root -tree 2>/dev/null | grep -F "bevy_mara — $(EXAMPLE)" >/dev/null; then \
			found=1; \
			break; \
		fi; \
		if ! kill -0 $$pid >/dev/null 2>&1; then \
			break; \
		fi; \
		sleep 1; \
	done; \
	if [ -z "$$found" ]; then \
		echo "GUI smoke failed: window not found for example $(EXAMPLE)"; \
		cat "$$log"; \
		exit 1; \
	fi; \
	sleep 5; \
	if ! kill -0 $$pid >/dev/null 2>&1; then \
		echo "GUI smoke failed: example exited before stability window"; \
		cat "$$log"; \
		exit 1; \
	fi; \
	if grep -E "panicked at|Encountered a panic|\\bERROR\\b|\\bWARN\\b|Failed to find replacement characters" "$$log" >/dev/null; then \
		echo "GUI smoke failed: fatal log output detected"; \
		cat "$$log"; \
		exit 1; \
	fi; \
	echo "GUI smoke passed: bevy_mara — $(EXAMPLE) window appeared and stayed alive"; \
	cat "$$log"

# Phase 2 of `PLAN_NEWUI.md` — flex-based pane2 example. Empty
# panes (title strip + empty body) at all 12 anchor positions plus
# theme/mode cycle buttons. Doesn't touch the existing demo.
run-newui:
	@$(WL_PREFLIGHT)
	@$(RUN_ENV) $(RUN_WITH) $(CARGO) run -p $(APP_PKG) --example newui

# Plain-egui (no Bevy) demo — `eframe` with the `wgpu` backend,
# same Vulkan path Bevy uses. Runs under the `nixVulkan` wrapper
# out of the box on nix systems; override with `RUN_WITH=` on
# distros with a native Vulkan driver.
run-egui:
	@DISPLAY=$(DISPLAY) $(RUN_WITH) $(CARGO) run -p egui_mara --example egui_demo

# Web (wasm32) target — `egui_mara` compiled to WebAssembly and
# served in a browser via `trunk`. The mara UI core is host-agnostic
# egui, so the same ribbons / panes / widgets `run-egui` shows
# natively render here unchanged. The nix devshell provides the
# `wasm32-unknown-unknown` target + `trunk`; outside nix, run once:
#   rustup target add wasm32-unknown-unknown && cargo install --locked trunk
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

fmt:
	@$(CARGO) fmt --all

harden:
	@git diff --check
	@$(CARGO) fmt --all -- --check
	@$(CARGO) check --workspace --no-default-features
	@$(CARGO) test --workspace --no-default-features
	@$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings
	@$(CARGO) test --workspace --all-targets --all-features

harden-gui: harden smoke-gui

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
	@echo "  smoke-gui    Run the example briefly and verify its X11 window appears"
	@echo "  test         Test the same app target as build/run ($(APP_PKG) example $(EXAMPLE))"
	@echo "  test-all     Run the full workspace all-target test suite"
	@echo "  check        Check the same app target as build/run ($(APP_PKG) example $(EXAMPLE))"
	@echo "  check-all    Check the full workspace all-target suite"
	@echo "  fmt          Format the crate"
	@echo "  harden       Run diff whitespace check + fmt/check + strict clippy + all-feature tests"
	@echo "  harden-gui   Run harden, then GUI smoke (requires X display)"
	@echo "  bench        Run benchmarks"
	@echo "  clean        Remove Cargo build artifacts"
	@echo
	@echo "Examples:"
	@echo "  make run"
	@echo "  make run EXAMPLE=other        # run a different example"
	@echo "  make run BACKEND=x11          # force X11 / XWayland (auto-detects Wayland)"
	@echo "  make run BACKEND=wayland      # force native Wayland"
	@echo "  make run DISPLAY=:0           # target a different X server (BACKEND=x11)"
	@echo "  make run RUN_WITH=nixGL       # OpenGL wrapper instead of Vulkan"
	@echo "  make run RUN_WITH=            # no wrapper (native run)"
	@echo

h: help
