# Plan 001: Enforce the existing verification gates (CI, fmt target, lint config, agent docs)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat bcf6600..HEAD -- Makefile README.md .github/ TOOLCHAIN.md`
> If any of these changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `bcf6600`, 2026-07-20

## Why this matters

This repo has an unusually strong local verification baseline — `make check`
runs a workspace check **plus** a battery of grep gates that enforce the
sealed-API boundary, and `make harden` runs fmt-check, no-default-features
builds, clippy `-D warnings`, and all-features tests. But **nothing runs any
of it automatically**: the only GitHub workflow builds the mdBook site. The
README also documents a `make fmt` target that does not exist, and there is
no `rustfmt.toml`/`.editorconfig`/root `CLAUDE.md`, so both humans and agents
re-derive conventions every session. Every other plan in this directory
assumes these gates hold — this plan makes them held automatically.

## Current state

- `Makefile` — targets: `build`, `run`, `serve`, `test` (:60), `test-all`
  (:65 → `cargo test --workspace --all-targets`), `check` (:68 → workspace
  check + sealed-example check + ~15 sealed-API grep gates), `harden` (:346 →
  `git diff --check`, `cargo fmt --all -- --check`, no-default-features
  check+test, `clippy --workspace --all-targets --all-features -- -D warnings`,
  all-features test). **There is no `fmt` target** (`grep -n '^fmt:' Makefile`
  → no match).
- `README.md:74` — lists `make fmt` in the "Full workspace gates" block.
- `.github/workflows/` — contains only `book.yml` (mdBook build on `develop`).
- Repo root has **no** `rustfmt.toml`, `.rustfmt.toml`, `clippy.toml`,
  `.editorconfig`, `CLAUDE.md`, or `AGENTS.md` (verified by `ls -a`).
- `TOOLCHAIN.md` documents that the ambient Rust toolchain may be too old and
  gates must run inside the Nix shell: `nix develop --impure -c make check`.
- `cargo audit` currently reports (all **transitive**, none in Mara's direct
  control): `quick-xml 0.39.4` RUSTSEC-2026-0194 and RUSTSEC-2026-0195 (both
  HIGH, fixed in ≥0.41.0, reached only through the **build-time** chain
  `wayland-scanner → smithay → egui-winit`); `cgmath 0.18.0` RUSTSEC-2026-0196
  (unmaintained) and RUSTSEC-2026-0197 (unsound `swap_columns`) via
  `three-d 0.19`; warnings for unmaintained `instant 0.1.13`,
  `ttf-parser 0.25.1`, and yanked `spin 0.10.0`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Check + sealed gates | `nix develop --impure -c make check` | exit 0 |
| Full test suite | `nix develop --impure -c make test-all` | exit 0, all pass |
| Full gate | `nix develop --impure -c make harden` | exit 0 |
| Format | `nix develop --impure -c cargo fmt --all` | exit 0 |

## Scope

**In scope** (the only files you should create/modify):
- `Makefile` (add `fmt` target only — do not touch other targets)
- `README.md` (only if the gates block needs a wording fix after the target exists)
- `.github/workflows/ci.yml` (create)
- `rustfmt.toml`, `.editorconfig` (create)
- `.cargo/audit.toml` (create)
- `CLAUDE.md` at repo root (create)

**Out of scope** (do NOT touch):
- `.github/workflows/book.yml` — working docs pipeline.
- Any Rust source file. This plan is pure tooling/docs.
- `flake.nix` / `flake.lock` — the Nix shell works; CI consumes it as-is.
- Fixing any advisory by bumping dependencies — recorded as policy only here.

## Git workflow

- Branch from `develop`: `feature/001-verification-gates`
- Conventional commits, **title only, max 50 chars, no body, no signature or
  Co-Authored-By line** (repo owner requirement). Example from history:
  `chore: android build`. Suggested: `chore(dx): add ci, fmt target, lint config`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add the missing `fmt` target

In `Makefile`, next to the `test`/`check` targets, add:

```make
fmt:
	@$(CARGO) fmt --all

f: fmt
```

Add `fmt` and `f` to the existing `.PHONY` line (it currently lists
`build b compile c run r serve test t test-all check harden bench clean docs release help h`).

**Verify**: `nix develop --impure -c make fmt` → exits 0.
**Verify**: `grep -n 'make fmt' README.md` → the documented command now exists; no README edit needed unless wording is wrong.

### Step 2: Pin format/editor config

Create `rustfmt.toml` containing only a comment declaring the project uses
rustfmt defaults (`# Mara uses rustfmt defaults; this file pins that decision.`)
— the codebase is already formatted with defaults, so adding options would
create churn. Create `.editorconfig` with `root = true`, 4-space indent for
`*.rs`, 2-space for `*.{yml,yaml,toml,md}`, LF endings, final newline.

**Verify**: `nix develop --impure -c cargo fmt --all -- --check` → exit 0 (no reformat needed). If this fails, STOP — defaults drifted, do not commit a reformat.

### Step 3: Add the CI workflow

Create `.github/workflows/ci.yml`: trigger on `push` to `develop`/`main` and
`pull_request`. One job (`ubuntu-latest`) using
`DeterminateSystems/nix-installer-action` + `DeterminateSystems/magic-nix-cache-action`,
then run in order:

1. `nix develop --impure -c make check`
2. `nix develop --impure -c make test-all`
3. `nix develop --impure -c make harden`

Add a second independent job `audit` running `cargo audit` (via
`rustsec/audit-check` action or `nix develop --impure -c cargo audit` if the
devshell provides it) with `continue-on-error: true` — advisory, not blocking,
because all current findings are transitive (see Current state).

**Verify**: `nix develop --impure -c make check && nix develop --impure -c make test-all && nix develop --impure -c make harden` all exit 0 locally (this is exactly what CI will run). If `harden` fails on pre-existing warnings, STOP and report which step fails — do not fix source files under this plan.

### Step 4: Record the audit policy

Create `.cargo/audit.toml` with an `[advisories] ignore = [...]` list for
RUSTSEC-2026-0194, RUSTSEC-2026-0195 (quick-xml — build-time-only path via
wayland-scanner; revisit when egui-winit's stack bumps quick-xml ≥0.41), and
RUSTSEC-2026-0196/0197 (cgmath via three-d — `swap_columns` not called by
Mara; tracked until three-d migrates). **Each ignore must carry a comment with
the reason and the revisit condition.**

**Verify**: `cargo audit` (in the devshell) → exit 0 with the ignores applied, or exits listing only the ignored advisories.

### Step 5: Write the root `CLAUDE.md`

Create `CLAUDE.md` at repo root covering, briefly (≤60 lines):

- Toolchain: all gates run via `nix develop --impure -c make <target>`; the
  ambient toolchain may be too old (point at `TOOLCHAIN.md`).
- Canonical gates: `make check` (includes sealed-API grep gates — **never
  weaken or delete a grep line to make a change pass**), `make test-all`,
  `make harden`, `make fmt`.
- The sealed-API rule: app-facing code sees `MaraUi` + `vocab` types, never
  `egui::Ui`/`egui::Rect`; egui access lives in `crates/core/src/backend/` and
  `__internal_*` entry points; the `raw-egui` feature must never be enabled in
  library crates.
- Layering: reusable behavior goes in `mara_core`; `mara/` and
  `mara/plugin/*` are thin host adapters; module crates under `crates/modules/`.
- Commit style: conventional commits, title only, ≤50 chars, no signatures.

**Verify**: file exists; `wc -l CLAUDE.md` ≤ ~70.

## Test plan

No new Rust tests — this plan is tooling. The verification is that all three
gates pass locally (Step 3's verify) and the workflow file is well-formed
(`actionlint .github/workflows/ci.yml` if `actionlint` is available; otherwise
YAML-parse it with `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))"`).

## Done criteria

- [ ] `nix develop --impure -c make fmt` exits 0
- [ ] `nix develop --impure -c make check` exits 0
- [ ] `nix develop --impure -c make test-all` exits 0
- [ ] `nix develop --impure -c make harden` exits 0
- [ ] `.github/workflows/ci.yml`, `rustfmt.toml`, `.editorconfig`, `.cargo/audit.toml`, `CLAUDE.md` exist
- [ ] `git status` shows no modified files outside the in-scope list
- [ ] `plans/README.md` status row updated

## STOP conditions

- `make harden` fails on the untouched tree (pre-existing fmt/clippy debt) —
  report the exact failing step; fixing source is out of scope here.
- `cargo audit` is unavailable in the devshell and cannot run — skip Step 4's
  verify, note it in the report, still commit the policy file.
- The Nix devshell fails to evaluate on your machine — report; do not
  substitute an ambient toolchain for the gate runs.

## Maintenance notes

- When egui-winit/smithay bump `quick-xml` ≥0.41, delete the two ignores in
  `.cargo/audit.toml`. When `three-d` drops `cgmath`, delete those two.
- Plans 002–014 all cite these gates as their verification; if a Makefile
  target is renamed, update the plans' command tables.
- Reviewers: check the CI workflow runs the *make targets*, not re-derived
  cargo invocations — the sealed-API greps in `make check` are the point.
