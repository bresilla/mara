{
  description = "bevy_mara — reusable glass-themed Bevy + egui editor UI kit, development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    # nixGL wraps a command with the host's GPU drivers so OpenGL / Vulkan
    # apps (e.g. Bevy via wgpu) work inside a nix devShell on non-NixOS hosts.
    nixgl.url = "github:nix-community/nixGL";
  };

  outputs =
    { self, nixpkgs, rust-overlay, flake-utils, nixgl, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];

        pkgs = import nixpkgs {
          inherit system overlays;
          config = {
            allowUnfree = true;
            nvidia.acceptLicense = true;
          };
        };

        # .envrc exports MARA_NVIDIA_VERSION from /proc/driver/nvidia/version
        # before direnv loads the flake. We read it via getEnv (works because
        # --impure is wired into .envrc). Reading from a file under .direnv/
        # doesn't work: flakes in a git repo only expose git-tracked files to
        # the evaluator, and .direnv/ is globally gitignored.
        nvidiaVersion = let v = builtins.getEnv "MARA_NVIDIA_VERSION";
        in if v != "" then v
           else throw "bevy_mara: MARA_NVIDIA_VERSION is unset — is direnv loaded and is the NVIDIA driver running?";

        # Build nixGL pinned to the detected version. `nvidiaHash = null`
        # makes it fetch the matching .run impurely (--impure is wired into
        # .envrc), so this stays automatic as the host driver changes.
        # Note: nixGL still refs xorg.libX11/libxcb/libxshmfence internally,
        # which prints deprecation warnings during eval — upstream bug.
        nixglPkgs = import "${nixgl}/default.nix" {
          inherit pkgs nvidiaVersion;
          nvidiaHash = null;
        };

        # Stable, unversioned `nixGL` / `nixVulkan` aliases — the underlying
        # binaries have the detected driver version baked into their names.
        nixGLAlias = pkgs.runCommand "nixGL" { } ''
          mkdir -p $out/bin
          ln -s ${nixglPkgs.nixGLNvidia}/bin/nixGLNvidia-${nvidiaVersion} $out/bin/nixGL
        '';
        nixVulkanAlias = pkgs.runCommand "nixVulkan" { } ''
          mkdir -p $out/bin
          ln -s ${nixglPkgs.nixVulkanNvidia}/bin/nixVulkanNvidia-${nvidiaVersion} $out/bin/nixVulkan
        '';

        # Runtime libs Bevy needs on Linux (audio, input, windowing, GPU).
        bevyLibs = with pkgs; [
          alsa-lib
          udev
          vulkan-loader
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxi
          libxrandr
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rustfmt" "clippy" ];
              # `wasm32-unknown-unknown` std for the `egui_mara_web`
              # browser host (`api_crates/web`). Without it `trunk` /
              # `cargo --target wasm32-unknown-unknown` fail with
              # "can't find crate for `core`".
              targets = [ "wasm32-unknown-unknown" ];
            })
            pkgs.clang
            pkgs.mold
            pkgs.pkg-config

            # `trunk` — bundles the `egui_mara_web` crate to wasm and
            # serves it in a browser (`make serve-web`). Drives cargo
            # for the `wasm32-unknown-unknown` target and fetches a
            # matching `wasm-bindgen-cli` itself.
            pkgs.trunk

            # Font tooling — `pyftsubset` (fontTools + brotli) is used to
            # extract a single face from an Iosevka `.ttc` collection
            # and trim it down to the Latin + symbol subset that ships
            # embedded inside `mara_core` via `include_bytes!`.
            (pkgs.python3.withPackages (ps: with ps; [ fonttools brotli ]))

            # GPU wrappers.
            nixGLAlias
            nixVulkanAlias
            nixglPkgs.nixGLNvidia
            nixglPkgs.nixVulkanNvidia
            nixglPkgs.nixGLIntel      # Mesa fallback (AMD / Intel iGPU)
            nixglPkgs.nixVulkanIntel
          ] ++ bevyLibs;

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath bevyLibs;
          # Silence wgpu's validation-layer spam. wgpu's generated SPIR-V
          # uses relaxed atomic ordering that Vulkan 1.3 validation
          # rejects — naga/wgpu upstream bug, harmless at runtime.
          WGPU_VALIDATION = "0";
          WGPU_DEBUG = "0";
        };
      }
    );
}
