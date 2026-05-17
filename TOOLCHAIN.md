# Toolchain

Use the repo's Nix development shell for all build, check, test, and
run commands:

```sh
nix develop --impure -c make check
nix develop --impure -c make check-all
nix develop --impure -c make test-all
```

The ambient system Rust toolchain may be too old for this workspace's
current Bevy, egui, and iconflow dependency set. The flake provides a
compatible stable Rust toolchain, the `wasm32-unknown-unknown` target,
`trunk`, GPU wrapper aliases, and runtime libraries used by the native
Bevy/eframe demos.

When working in this repo, prefer Makefile targets over ad-hoc `cargo`
commands so the Bevy demo target, web target, display wrapper, and
verification gates stay consistent.
