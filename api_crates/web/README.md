# egui_mara_web

Small WebAssembly helpers for `egui_mara`.

The full browser demo no longer lives in this API crate. It moved to
the repo-root `example/` crate so the example consumes Mara exactly
like an external application:

```sh
make serve-web
make build-web
```

This crate remains available for web-specific helper code, such as
pinning eframe/wgpu to WebGL2 with `force_webgl2`.

On native targets this crate has no runner.
